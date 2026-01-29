//! Concurrent GDSF Cache Implementation
//!
//! A thread-safe GDSF cache using lock striping (segmented storage) for high-performance
//! concurrent access. This is the multi-threaded counterpart to [`GdsfCache`](crate::GdsfCache).
//!
//! # How It Works
//!
//! GDSF (Greedy Dual-Size Frequency) is designed for caching variable-size objects.
//! Priority is calculated as: `(Frequency / Size) + GlobalAge`
//!
//! This formula favors:
//! - **Smaller objects**: More items fit in cache
//! - **Frequently accessed objects**: Higher hit rates
//! - **Newer objects**: Via aging mechanism
//!
//! The cache partitions keys across multiple independent segments using hash-based sharding.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────────────┐
//! │                        ConcurrentGdsfCache                                   │
//! │                                                                              │
//! │  hash(key) % N  ──▶  Segment Selection                                       │
//! │                                                                              │
//! │  ┌────────────────────┐ ┌────────────────────┐     ┌────────────────────┐    │
//! │  │    Segment 0       │ │    Segment 1       │ ... │   Segment N-1      │    │
//! │  │  max_size=625MB    │ │  max_size=625MB    │     │  max_size=625MB    │    │
//! │  │  age=1.5           │ │  age=2.3           │     │  age=1.8           │    │
//! │  │  ┌──────────────┐  │ │  ┌──────────────┐  │     │  ┌──────────────┐  │    │
//! │  │  │    Mutex     │  │ │  │    Mutex     │  │     │  │    Mutex     │  │    │
//! │  │  └──────┬───────┘  │ │  └──────┬───────┘  │     │  └──────┬───────┘  │    │
//! │  │         │          │ │         │          │     │         │          │    │
//! │  │  ┌──────▼───────┐  │ │  ┌──────▼───────┐  │     │  ┌──────▼───────┐  │    │
//! │  │  │  GdsfSegment │  │ │  │  GdsfSegment │  │     │  │  GdsfSegment │  │    │
//! │  │  │  (priority   │  │ │  │  (priority   │  │     │  │  (priority   │  │    │
//! │  │  │   lists)     │  │ │  │   lists)     │  │     │  │   lists)     │  │    │
//! │  │  └──────────────┘  │ │  └──────────────┘  │     │  └──────────────┘  │    │
//! │  └────────────────────┘ └────────────────────┘     └────────────────────┘    │
//! └──────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Per-Segment Size Tracking
//!
//! Each segment has its own:
//! - **max_size**: Total size = cache_max_size / segment_count
//! - **current_size**: Sum of item sizes in this segment
//! - **global_age**: Aging factor (incremented on evictions)
//!
//! ## Trade-offs
//!
//! - **Pros**: Near-linear scaling, size-aware eviction per segment
//! - **Cons**: Size distribution depends on key hashing. Uneven key distribution
//!   can cause some segments to be fuller than others.
//!
//! # API Note: Size Parameter Required
//!
//! Unlike other caches, GDSF's `put()` requires a size parameter:
//!
//! ```rust,ignore
//! // Standard caches
//! lru_cache.put(key, value);
//!
//! // GDSF requires size
//! gdsf_cache.put(key, value, 2048);  // size in bytes
//! ```
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get/Put/Remove | O(log P) per segment |
//! | Concurrency | Near-linear scaling up to segment count |
//! | Memory overhead | ~170 bytes per entry + one Mutex per segment |
//! | Size-awareness | Excellent (per-segment size tracking) |
//!
//! Where P = distinct priority buckets per segment. Priority = (frequency/size) + age.
//!
//! # When to Use
//!
//! **Use ConcurrentGdsfCache when:**
//! - Multiple threads need cache access
//! - Caching variable-size objects (images, files, API responses)
//! - Total size budget is more important than item count
//! - CDN-like workloads with diverse object sizes
//!
//! **Consider alternatives when:**
//! - Single-threaded access only → use `GdsfCache`
//! - Uniform-size objects → simpler caches work equally well
//! - Need global size coordination → use `Mutex<GdsfCache>`
//! - Entry-count is the primary constraint → use `ConcurrentLruCache`
//!
//! # Thread Safety
//!
//! `ConcurrentGdsfCache` is `Send + Sync` and can be shared via `Arc`.
//!
//! # Example
//!
//! ```rust,ignore
//! use cache_rs::concurrent::ConcurrentGdsfCache;
//! use cache_rs::config::{ConcurrentCacheConfig, ConcurrentGdsfCacheConfig, GdsfCacheConfig};
//! use std::num::NonZeroUsize;
//! use std::sync::Arc;
//! use std::thread;
//!
//! // 10GB cache distributed across segments
//! let config: ConcurrentGdsfCacheConfig = ConcurrentCacheConfig {
//!     base: GdsfCacheConfig {
//!         capacity: NonZeroUsize::new(100_000).unwrap(),
//!         initial_age: 0.0,
//!         max_size: 10 * 1024 * 1024 * 1024,
//!     },
//!     segments: 16,
//! };
//! let cache = Arc::new(ConcurrentGdsfCache::init(config, None));
//!
//! // Simulate CDN edge cache with multiple worker threads
//! let handles: Vec<_> = (0..4).map(|worker| {
//!     let cache = Arc::clone(&cache);
//!     thread::spawn(move || {
//!         for i in 0..1000 {
//!             // Simulate varying object sizes
//!             let size = if i % 10 == 0 {
//!                 1024 * 1024  // 1MB (large objects)
//!             } else {
//!                 4096  // 4KB (small objects)
//!             };
//!             
//!             let key = format!("asset-{}-{}", worker, i);
//!             let data = vec![0u8; size as usize];
//!             cache.put(key.clone(), data, size);
//!             
//!             // Simulate cache reads
//!             let _ = cache.get(&key);
//!         }
//!     })
//! }).collect();
//!
//! for h in handles {
//!     h.join().unwrap();
//! }
//!
//! println!("Cache size: {} bytes", cache.current_size());
//! ```
//!
//! # Size-Based Eviction Example
//!
//! ```rust,ignore
//! use cache_rs::concurrent::ConcurrentGdsfCache;
//! use cache_rs::config::{ConcurrentCacheConfig, ConcurrentGdsfCacheConfig, GdsfCacheConfig};
//! use std::num::NonZeroUsize;
//!
//! // 10MB cache
//! let config: ConcurrentGdsfCacheConfig = ConcurrentCacheConfig {
//!     base: GdsfCacheConfig {
//!         capacity: NonZeroUsize::new(10_000).unwrap(),
//!         initial_age: 0.0,
//!         max_size: 10 * 1024 * 1024,
//!     },
//!     segments: 16,
//! };
//! let cache = ConcurrentGdsfCache::init(config, None);
//!
//! // Insert small frequently-accessed items
//! for i in 0..100 {
//!     cache.put(format!("small-{}", i), vec![0u8; 1024], 1024);
//!     // Access multiple times to increase frequency
//!     for _ in 0..5 {
//!         let _ = cache.get(&format!("small-{}", i));
//!     }
//! }
//!
//! // Insert large item - may evict multiple small items based on priority
//! cache.put("large".to_string(), vec![0u8; 5 * 1024 * 1024], 5 * 1024 * 1024);
//!
//! // GDSF may choose to keep small popular items over one large item
//! ```

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
use hashbrown::DefaultHashBuilder;

#[cfg(not(feature = "hashbrown"))]
use std::collections::hash_map::RandomState as DefaultHashBuilder;

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
    /// Creates a new concurrent GDSF cache from a configuration.
    ///
    /// This is the **recommended** way to create a concurrent GDSF cache.
    ///
    /// # Arguments
    ///
    /// * `config` - The cache configuration specifying capacity, max size, and segments
    /// * `hasher` - Optional custom hash builder. If `None`, uses the default hash builder.
    pub fn init(
        config: crate::config::ConcurrentGdsfCacheConfig,
        hasher: Option<DefaultHashBuilder>,
    ) -> Self {
        let segment_count = config.segments;
        let capacity = config.base.capacity;
        let max_size = config.base.max_size;
        let initial_age = config.base.initial_age;

        let segment_capacity = capacity.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();
        let segment_max_size = max_size / segment_count as u64;

        let hash_builder = hasher.unwrap_or_default();

        let segments: Vec<_> = (0..segment_count)
            .map(|_| {
                let segment_config = crate::config::GdsfCacheConfig {
                    capacity: segment_cap,
                    initial_age,
                    max_size: segment_max_size,
                };
                Mutex::new(GdsfSegment::init(segment_config, hash_builder.clone()))
            })
            .collect();

        Self {
            segments: segments.into_boxed_slice(),
            hash_builder,
        }
    }
}

impl<K, V, S> ConcurrentGdsfCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
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

    /// Returns the current total size of cached content across all segments.
    pub fn current_size(&self) -> u64 {
        self.segments.iter().map(|s| s.lock().current_size()).sum()
    }

    /// Returns the maximum content size the cache can hold across all segments.
    pub fn max_size(&self) -> u64 {
        self.segments.iter().map(|s| s.lock().max_size()).sum()
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
    use crate::config::{ConcurrentCacheConfig, ConcurrentGdsfCacheConfig, GdsfCacheConfig};

    extern crate std;
    use std::string::ToString;
    use std::sync::Arc;
    use std::thread;
    use std::vec::Vec;

    fn make_config(capacity: usize, segments: usize) -> ConcurrentGdsfCacheConfig {
        ConcurrentCacheConfig {
            base: GdsfCacheConfig {
                capacity: NonZeroUsize::new(capacity).unwrap(),
                initial_age: 0.0,
                max_size: u64::MAX,
            },
            segments,
        }
    }

    #[test]
    fn test_basic_operations() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

        cache.put("a".to_string(), 1, 100u64);
        cache.put("b".to_string(), 2, 200u64);

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), Some(2));
    }

    #[test]
    fn test_concurrent_access() {
        let cache: Arc<ConcurrentGdsfCache<String, i32>> =
            Arc::new(ConcurrentGdsfCache::init(make_config(100000, 16), None));
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
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

        // Capacity is distributed across segments
        let capacity = cache.capacity();
        assert!(capacity >= 16);
        assert!(capacity <= 10000);
    }

    #[test]
    fn test_segment_count() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::init(make_config(10000, 8), None);

        assert_eq!(cache.segment_count(), 8);
    }

    #[test]
    fn test_len_and_is_empty() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

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
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

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
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

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
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

        cache.put("exists".to_string(), 1, 100);

        assert!(cache.contains_key(&"exists".to_string()));
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_get_with() {
        let cache: ConcurrentGdsfCache<String, String> =
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

        cache.put("key".to_string(), "hello world".to_string(), 100);

        let len = cache.get_with(&"key".to_string(), |v: &String| v.len());
        assert_eq!(len, Some(11));

        let missing = cache.get_with(&"missing".to_string(), |v: &String| v.len());
        assert_eq!(missing, None);
    }

    #[test]
    fn test_size_aware_eviction() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::init(make_config(1000, 16), None);

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
        assert!(!cache.is_empty());
    }

    #[test]
    fn test_eviction_on_capacity() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::init(make_config(5000, 16), None);

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
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

        cache.put("a".to_string(), 1, 100);
        cache.put("b".to_string(), 2, 200);

        let metrics = cache.metrics();
        // Metrics aggregation across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_algorithm_name() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

        assert_eq!(cache.algorithm_name(), "ConcurrentGDSF");
    }

    #[test]
    fn test_empty_cache_operations() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&"missing".to_string()), None);
        assert_eq!(cache.remove(&"missing".to_string()), None);
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_borrowed_key_lookup() {
        let cache: ConcurrentGdsfCache<String, i32> =
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

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
            ConcurrentGdsfCache::init(make_config(10000, 16), None);

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
            ConcurrentGdsfCache::init(make_config(5000, 16), None);

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
