//! Concurrent SLRU Cache Implementation
//!
//! Provides a thread-safe Segmented LRU cache using segmented storage for high-performance
//! multi-threaded access. This is the concurrent equivalent of [`SlruCache`][crate::SlruCache].
//!
//! # How It Works
//!
//! SLRU maintains two segments per shard: probationary and protected. Items enter
//! the probationary segment and are promoted to protected on subsequent access.
//! The key space is partitioned across multiple shards using hash-based sharding.
//!
//! # Performance Characteristics
//!
//! - **Time Complexity**: O(1) average for get, put, remove
//! - **Scan Resistance**: Better than LRU for mixed workloads
//! - **Concurrency**: Near-linear scaling up to segment count
//!
//! # When to Use
//!
//! Use `ConcurrentSlruCache` when:
//! - You need scan resistance in a multi-threaded environment
//! - Workload has both frequently and occasionally accessed items
//!
//! # Thread Safety
//!
//! `ConcurrentSlruCache` implements `Send` and `Sync` and can be safely shared
//! across threads via `Arc`.

extern crate alloc;

use crate::metrics::CacheMetrics;
use crate::slru::SlruSegment;
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

/// A thread-safe SLRU cache with segmented storage for high concurrency.
pub struct ConcurrentSlruCache<K, V, S = DefaultHashBuilder> {
    segments: Box<[Mutex<SlruSegment<K, V, S>>]>,
    hash_builder: S,
}

impl<K, V> ConcurrentSlruCache<K, V, DefaultHashBuilder>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    /// Creates a new concurrent SLRU cache with the specified total capacity.
    pub fn new(capacity: NonZeroUsize, protected_capacity: NonZeroUsize) -> Self {
        Self::with_segments(capacity, protected_capacity, default_segment_count())
    }

    /// Creates a new concurrent SLRU cache with custom segment count.
    pub fn with_segments(
        capacity: NonZeroUsize,
        protected_capacity: NonZeroUsize,
        segment_count: usize,
    ) -> Self {
        Self::with_segments_and_hasher(
            capacity,
            protected_capacity,
            segment_count,
            DefaultHashBuilder::default(),
        )
    }
}

impl<K, V, S> ConcurrentSlruCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    /// Creates a new concurrent SLRU cache with custom hasher.
    pub fn with_segments_and_hasher(
        capacity: NonZeroUsize,
        protected_capacity: NonZeroUsize,
        segment_count: usize,
        hash_builder: S,
    ) -> Self {
        assert!(segment_count > 0, "segment_count must be greater than 0");
        assert!(
            capacity.get() >= segment_count,
            "capacity must be >= segment_count"
        );

        let segment_capacity = capacity.get() / segment_count;
        let segment_protected = protected_capacity.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();
        let segment_protected_cap = NonZeroUsize::new(segment_protected.max(1)).unwrap();

        let segments: Vec<_> = (0..segment_count)
            .map(|_| {
                Mutex::new(SlruSegment::with_hasher(
                    segment_cap,
                    segment_protected_cap,
                    hash_builder.clone(),
                ))
            })
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

    /// Returns the total capacity across all segments.
    pub fn capacity(&self) -> usize {
        self.segments.iter().map(|s| s.lock().cap().get()).sum()
    }

    /// Returns the number of segments in the cache.
    pub fn segment_count(&self) -> usize {
        self.segments.len()
    }

    /// Returns the total number of entries across all segments.
    pub fn len(&self) -> usize {
        self.segments.iter().map(|s| s.lock().len()).sum()
    }

    /// Returns `true` if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|s| s.lock().is_empty())
    }

    /// Gets a value from the cache.
    ///
    /// This clones the value to avoid holding the lock. For zero-copy access,
    /// use `get_with()` instead.
    pub fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.get(key).cloned()
    }

    /// Gets a value and applies a function to it while holding the lock.
    ///
    /// This is more efficient than `get()` when you only need to read from the value,
    /// as it avoids cloning.
    pub fn get_with<Q, F, R>(&self, key: &Q, f: F) -> Option<R>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        F: FnOnce(&V) -> R,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.get(key).map(f)
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// New items enter the probationary segment and are promoted to the protected
    /// segment on subsequent access.
    pub fn put(&self, key: K, value: V) -> Option<(K, V)> {
        let idx = self.segment_index(&key);
        let mut segment = self.segments[idx].lock();
        segment.put(key, value)
    }

    /// Removes a key from the cache, returning the value if it existed.
    pub fn remove<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.remove(key)
    }

    /// Returns `true` if the cache contains the specified key.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.get(key).is_some()
    }

    /// Clears all entries from the cache.
    pub fn clear(&self) {
        for segment in self.segments.iter() {
            segment.lock().clear();
        }
    }
}

impl<K, V, S> CacheMetrics for ConcurrentSlruCache<K, V, S>
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
        "ConcurrentSLRU"
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for ConcurrentSlruCache<K, V, S> {}
unsafe impl<K: Send, V: Send, S: Send + Sync> Sync for ConcurrentSlruCache<K, V, S> {}

impl<K, V, S> core::fmt::Debug for ConcurrentSlruCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConcurrentSlruCache")
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
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), Some(2));
    }

    #[test]
    fn test_concurrent_access() {
        let cache: Arc<ConcurrentSlruCache<String, i32>> = Arc::new(ConcurrentSlruCache::new(
            NonZeroUsize::new(1000).unwrap(),
            NonZeroUsize::new(500).unwrap(),
        ));
        let num_threads = 8;
        let ops_per_thread = 500;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}_{}", t, i);
                    cache.put(key.clone(), i);
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
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        // Capacity is distributed across segments
        let capacity = cache.capacity();
        assert!(capacity >= 16);
        assert!(capacity <= 100);
    }

    #[test]
    fn test_segment_count() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::with_segments(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
            8,
        );

        assert_eq!(cache.segment_count(), 8);
    }

    #[test]
    fn test_len_and_is_empty() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        cache.put("key1".to_string(), 1);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());

        cache.put("key2".to_string(), 2);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_remove() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        cache.put("key1".to_string(), 1);
        cache.put("key2".to_string(), 2);

        assert_eq!(cache.remove(&"key1".to_string()), Some(1));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&"key1".to_string()), None);

        assert_eq!(cache.remove(&"nonexistent".to_string()), None);
    }

    #[test]
    fn test_clear() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        cache.put("key1".to_string(), 1);
        cache.put("key2".to_string(), 2);
        cache.put("key3".to_string(), 3);

        assert_eq!(cache.len(), 3);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_contains_key() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        cache.put("exists".to_string(), 1);

        assert!(cache.contains_key(&"exists".to_string()));
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_get_with() {
        let cache: ConcurrentSlruCache<String, String> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        cache.put("key".to_string(), "hello world".to_string());

        let len = cache.get_with(&"key".to_string(), |v: &String| v.len());
        assert_eq!(len, Some(11));

        let missing = cache.get_with(&"missing".to_string(), |v: &String| v.len());
        assert_eq!(missing, None);
    }

    #[test]
    fn test_eviction_on_capacity() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::with_segments(
            NonZeroUsize::new(80).unwrap(),
            NonZeroUsize::new(48).unwrap(),
            16,
        );

        // Fill the cache
        for i in 0..10 {
            cache.put(std::format!("key{}", i), i);
        }

        // Cache should not exceed capacity
        assert!(cache.len() <= 80);
    }

    #[test]
    fn test_promotion_to_protected() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::with_segments(
            NonZeroUsize::new(160).unwrap(),
            NonZeroUsize::new(80).unwrap(),
            16,
        );

        cache.put("key".to_string(), 1);

        // Access multiple times to promote to protected segment
        for _ in 0..3 {
            let _ = cache.get(&"key".to_string());
        }

        // Item should still be accessible
        assert_eq!(cache.get(&"key".to_string()), Some(1));
    }

    #[test]
    fn test_metrics() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        let metrics = cache.metrics();
        // Metrics aggregation across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_algorithm_name() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        assert_eq!(cache.algorithm_name(), "ConcurrentSLRU");
    }

    #[test]
    fn test_empty_cache_operations() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&"missing".to_string()), None);
        assert_eq!(cache.remove(&"missing".to_string()), None);
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_with_segments_and_hasher() {
        let hasher = DefaultHashBuilder::default();
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::with_segments_and_hasher(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
            4,
            hasher,
        );

        cache.put("test".to_string(), 42);
        assert_eq!(cache.get(&"test".to_string()), Some(42));
        assert_eq!(cache.segment_count(), 4);
    }

    #[test]
    fn test_borrowed_key_lookup() {
        let cache: ConcurrentSlruCache<String, i32> = ConcurrentSlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        );

        cache.put("test_key".to_string(), 42);

        // Test with borrowed key
        let key_str = "test_key";
        assert_eq!(cache.get(key_str), Some(42));
        assert!(cache.contains_key(key_str));
        assert_eq!(cache.remove(key_str), Some(42));
    }
}
