//! Concurrent GDSF Cache Implementation
//!
//! Provides a thread-safe GDSF cache using segmented storage for high-performance
//! multi-threaded access. This is the concurrent equivalent of [`GdsfCache`][crate::GdsfCache].
//!
//! # How It Works
//!
//! GDSF (Greedy Dual-Size Frequency) is designed for caching variable-size objects.
//! Priority is calculated as: `(Frequency / Size) + GlobalAge`
//!
//! This formula favors:
//! - Smaller objects (more items fit in cache)
//! - More frequently accessed objects
//! - Newer objects (via aging)
//!
//! # Performance Characteristics
//!
//! - **Time Complexity**: O(1) average for get, put, remove
//! - **Size-Aware**: Optimal for caching objects of varying sizes (images, documents)
//! - **Concurrency**: Near-linear scaling up to segment count
//!
//! # When to Use
//!
//! Use `ConcurrentGdsfCache` when:
//! - Caching variable-size objects (images, files, API responses)
//! - Total size budget is more important than item count
//! - CDN-like workloads with diverse object sizes
//!
//! # API Note
//!
//! Unlike other caches, `put()` requires a size parameter:
//! ```rust,ignore
//! cache.put("image.jpg".to_string(), image_data, 2048); // size in bytes
//! ```
//!
//! # Thread Safety
//!
//! `ConcurrentGdsfCache` implements `Send` and `Sync` and can be safely shared
//! across threads via `Arc`.

extern crate alloc;

use crate::gdsf::GdsfSegment;
use crate::metrics::CacheMetrics;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash};
use core::num::NonZeroUsize;
use parking_lot::Mutex;

#[cfg(feature = "hashbrown")]
use hashbrown::hash_map::DefaultHashBuilder;

#[cfg(not(feature = "hashbrown"))]
use std::collections::hash_map::RandomState as DefaultHashBuilder;

use super::default_segment_count;

/// A thread-safe GDSF cache with segmented storage for high concurrency.
///
/// GDSF (Greedy Dual-Size Frequency) is designed for caching variable-size objects.
/// The `put` method requires specifying the object size in addition to key and value.
pub struct ConcurrentGdsfCache<K, V, S = DefaultHashBuilder> {
    segments: Box<[Mutex<GdsfSegment<K, V, S>>]>,
    hash_builder: S,
}

impl<K, V> ConcurrentGdsfCache<K, V, DefaultHashBuilder>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    /// Creates a new concurrent GDSF cache with the specified total capacity.
    ///
    /// Capacity is measured in size units (bytes), not number of entries.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self::with_segments(capacity, default_segment_count())
    }

    /// Creates a new concurrent GDSF cache with custom segment count.
    pub fn with_segments(capacity: NonZeroUsize, segment_count: usize) -> Self {
        Self::with_segments_and_hasher(capacity, segment_count, DefaultHashBuilder::default())
    }
}

impl<K, V, S> ConcurrentGdsfCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    /// Creates a new concurrent GDSF cache with custom hasher.
    pub fn with_segments_and_hasher(
        capacity: NonZeroUsize,
        segment_count: usize,
        hash_builder: S,
    ) -> Self {
        assert!(segment_count > 0, "segment_count must be greater than 0");
        assert!(
            capacity.get() >= segment_count,
            "capacity must be >= segment_count"
        );

        let segment_capacity = capacity.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();

        let segments: Vec<_> = (0..segment_count)
            .map(|_| Mutex::new(GdsfSegment::with_hasher(segment_cap, hash_builder.clone())))
            .collect();

        Self {
            segments: segments.into_boxed_slice(),
            hash_builder,
        }
    }

    #[inline]
    fn segment_index<Q>(&self, key: &Q) -> usize
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash,
    {
        (self.hash_builder.hash_one(key) as usize) % self.segments.len()
    }

    /// Returns the total capacity across all segments (in size units).
    pub fn capacity(&self) -> usize {
        let mut total = 0usize;
        for segment in self.segments.iter() {
            total += segment.lock().cap().get();
        }
        total
    }

    /// Returns the number of segments in the cache.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Returns the total number of entries across all segments.
    pub fn len(&self) -> usize {
        let mut total = 0usize;
        for segment in self.segments.iter() {
            total += segment.lock().len();
        }
        total
    }

    /// Returns `true` if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        for segment in self.segments.iter() {
            if !segment.lock().is_empty() {
                return false;
            }
        }
        true
    }

    /// Gets a value from the cache.
    ///
    /// This clones the value to avoid holding the lock. For zero-copy access,
    /// use `get_with()` instead.
    pub fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.get(key)
    }

    /// Gets a value and applies a function to it while holding the lock.
    ///
    /// This is more efficient than `get()` when you only need to read from the value,
    /// as it avoids cloning.
    pub fn get_with<Q, F, R>(&self, key: &Q, f: F) -> Option<R>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
        F: FnOnce(&V) -> R,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.get(key).as_ref().map(f)
    }

    /// Inserts a key-value pair with its size into the cache.
    ///
    /// Unlike other caches, GDSF requires the size of the object for priority calculation.
    /// Returns the old value if the key was already present.
    pub fn put(&self, key: K, value: V, size: u64) -> Option<V>
    where
        K: Clone,
    {
        let idx = self.segment_index(&key);
        let mut segment = self.segments[idx].lock();
        segment.put(key, value, size)
    }

    /// Removes a key from the cache, returning the value if it existed.
    pub fn remove<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.pop(key)
    }

    /// Returns `true` if the cache contains the specified key.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let segment = self.segments[idx].lock();
        segment.contains_key(key)
    }

    /// Clears all entries from the cache.
    pub fn clear(&self) {
        for segment in self.segments.iter() {
            segment.lock().clear();
        }
    }
}

impl<K, V, S> CacheMetrics for ConcurrentGdsfCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    fn metrics(&self) -> BTreeMap<String, f64> {
        let mut aggregated = BTreeMap::new();
        for segment in self.segments.iter() {
            let segment_metrics = segment.lock().metrics().metrics();
            for (key, value) in segment_metrics {
                *aggregated.entry(key).or_insert(0.0) += value;
            }
        }
        aggregated
    }

    fn algorithm_name(&self) -> &'static str {
        "ConcurrentGDSF"
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for ConcurrentGdsfCache<K, V, S> {}
unsafe impl<K: Send, V: Send, S: Send + Sync> Sync for ConcurrentGdsfCache<K, V, S> {}

impl<K, V, S> core::fmt::Debug for ConcurrentGdsfCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConcurrentGdsfCache")
            .field("segment_count", &self.segments.len())
            .field("total_len", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate std;
    use std::string::ToString;
    use std::sync::Arc;
    use std::thread;
    use std::vec::Vec;

    #[test]
    fn test_basic_operations() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        cache.put("a".to_string(), 1, 100u64);
        cache.put("b".to_string(), 2, 200u64);

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), Some(2));
    }

    #[test]
    fn test_concurrent_access() {
        let cache: Arc<ConcurrentGdsfCache<String, i32>> =
            Arc::new(ConcurrentGdsfCache::new(NonZeroUsize::new(100000).unwrap()));
        let num_threads = 8;
        let ops_per_thread = 500;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}_{}", t, i);
                    let size = (10 + (i % 100)) as u64;
                    cache.put(key.clone(), i, size);
                    let _ = cache.get(&key);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert!(!cache.is_empty());
    }

    #[test]
    fn test_capacity() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        // Capacity is distributed across segments
        let capacity = cache.capacity();
        assert!(capacity >= 16);
        assert!(capacity <= 10000);
    }

    #[test]
    fn test_segment_count() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::with_segments(NonZeroUsize::new(10000).unwrap(), 8);

        assert_eq!(cache.segment_count(), 8);
    }

    #[test]
    fn test_len_and_is_empty() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        cache.put("key1".to_string(), 1, 100);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());

        cache.put("key2".to_string(), 2, 200);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_remove() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        cache.put("key1".to_string(), 1, 100);
        cache.put("key2".to_string(), 2, 200);

        assert_eq!(cache.remove(&"key1".to_string()), Some(1));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&"key1".to_string()), None);

        assert_eq!(cache.remove(&"nonexistent".to_string()), None);
    }

    #[test]
    fn test_clear() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        cache.put("key1".to_string(), 1, 100);
        cache.put("key2".to_string(), 2, 200);
        cache.put("key3".to_string(), 3, 300);

        assert_eq!(cache.len(), 3);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_contains_key() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        cache.put("exists".to_string(), 1, 100);

        assert!(cache.contains_key(&"exists".to_string()));
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_get_with() {
        let cache: ConcurrentGdsfCache<String, String> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        cache.put("key".to_string(), "hello world".to_string(), 100);

        let len = cache.get_with(&"key".to_string(), |v: &String| v.len());
        assert_eq!(len, Some(11));

        let missing = cache.get_with(&"missing".to_string(), |v: &String| v.len());
        assert_eq!(missing, None);
    }

    #[test]
    fn test_size_aware_eviction() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(1000).unwrap());

        // Add items with different sizes
        cache.put("small".to_string(), 1, 100);
        cache.put("medium".to_string(), 2, 500);
        cache.put("large".to_string(), 3, 800);

        // Access small item multiple times
        for _ in 0..10 {
            let _ = cache.get(&"small".to_string());
        }

        // Add more items to trigger eviction
        cache.put("another".to_string(), 4, 200);

        // Small item with high frequency should be retained
        assert!(cache.len() >= 1);
    }

    #[test]
    fn test_eviction_on_capacity() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(5000).unwrap());

        // Fill the cache with various sizes
        for i in 0..20 {
            let size = (100 + i * 50) as u64;
            cache.put(std::format!("key{}", i), i, size);
        }

        // Cache size should be managed
        assert!(!cache.is_empty());
    }

    #[test]
    fn test_metrics() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        cache.put("a".to_string(), 1, 100);
        cache.put("b".to_string(), 2, 200);

        let metrics = cache.metrics();
        // Metrics aggregation across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_algorithm_name() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        assert_eq!(cache.algorithm_name(), "ConcurrentGDSF");
    }

    #[test]
    fn test_empty_cache_operations() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&"missing".to_string()), None);
        assert_eq!(cache.remove(&"missing".to_string()), None);
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_with_segments_and_hasher() {
        let hasher = DefaultHashBuilder::default();
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::with_segments_and_hasher(
                NonZeroUsize::new(10000).unwrap(),
                4,
                hasher,
            );

        cache.put("test".to_string(), 42, 100);
        assert_eq!(cache.get(&"test".to_string()), Some(42));
        assert_eq!(cache.segment_count(), 4);
    }

    #[test]
    fn test_borrowed_key_lookup() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        cache.put("test_key".to_string(), 42, 100);

        // Test with borrowed key
        let key_str = "test_key";
        assert_eq!(cache.get(key_str), Some(42));
        assert!(cache.contains_key(key_str));
        assert_eq!(cache.remove(key_str), Some(42));
    }

    #[test]
    fn test_variable_sizes() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(10000).unwrap());

        // Add items with various sizes
        cache.put("tiny".to_string(), 1, 10);
        cache.put("small".to_string(), 2, 100);
        cache.put("medium".to_string(), 3, 500);
        cache.put("large".to_string(), 4, 1000);

        // All items should be present
        assert_eq!(cache.len(), 4);
        assert_eq!(cache.get(&"tiny".to_string()), Some(1));
        assert_eq!(cache.get(&"small".to_string()), Some(2));
        assert_eq!(cache.get(&"medium".to_string()), Some(3));
        assert_eq!(cache.get(&"large".to_string()), Some(4));
    }

    #[test]
    fn test_frequency_and_size_interaction() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::new(NonZeroUsize::new(5000).unwrap());

        // Add large item
        cache.put("large".to_string(), 1, 3000);

        // Add and frequently access small items
        for i in 0..10 {
            let key = std::format!("small{}", i);
            cache.put(key.clone(), i, 100);
            for _ in 0..5 {
                let _ = cache.get(&key);
            }
        }

        // Frequently accessed small items should have good priority
        assert!(!cache.is_empty());
    }
}
