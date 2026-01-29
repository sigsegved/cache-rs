//! Concurrent SLRU Cache Implementation
//!
//! A thread-safe Segmented LRU cache using lock striping (segmented storage) for
//! high-performance concurrent access. This is the multi-threaded counterpart to
//! [`SlruCache`](crate::SlruCache).
//!
//! # How It Works
//!
//! SLRU maintains two segments per shard: **probationary** (for new items) and
//! **protected** (for frequently accessed items). Items enter probationary and
//! are promoted to protected on subsequent access.
//!
//! The cache partitions keys across multiple independent shards using hash-based sharding.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────────────┐
//! │                        ConcurrentSlruCache                                   │
//! │                                                                              │
//! │  hash(key) % N  ──▶  Shard Selection                                         │
//! │                                                                              │
//! │  ┌────────────────────┐ ┌────────────────────┐     ┌────────────────────┐    │
//! │  │     Shard 0        │ │     Shard 1        │ ... │    Shard N-1       │    │
//! │  │  ┌──────────────┐  │ │  ┌──────────────┐  │     │  ┌──────────────┐  │    │
//! │  │  │    Mutex     │  │ │  │    Mutex     │  │     │  │    Mutex     │  │    │
//! │  │  └──────┬───────┘  │ │  └──────┬───────┘  │     │  └──────┬───────┘  │    │
//! │  │         │          │ │         │          │     │         │          │    │
//! │  │  ┌──────▼───────┐  │ │  ┌──────▼───────┐  │     │  ┌──────▼───────┐  │    │
//! │  │  │ Protected    │  │ │  │ Protected    │  │     │  │ Protected    │  │    │
//! │  │  │ [hot items]  │  │ │  │ [hot items]  │  │     │  │ [hot items]  │  │    │
//! │  │  ├──────────────┤  │ │  ├──────────────┤  │     │  ├──────────────┤  │    │
//! │  │  │ Probationary │  │ │  │ Probationary │  │     │  │ Probationary │  │    │
//! │  │  │ [new items]  │  │ │  │ [new items]  │  │     │  │ [new items]  │  │    │
//! │  │  └──────────────┘  │ │  └──────────────┘  │     │  └──────────────┘  │    │
//! │  └────────────────────┘ └────────────────────┘     └────────────────────┘    │
//! └──────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Segment Structure
//!
//! Each shard contains:
//! - **Probationary segment**: New items enter here (default: 80% of shard capacity)
//! - **Protected segment**: Items promoted on second access (default: 20% of shard capacity)
//!
//! ## Trade-offs
//!
//! - **Pros**: Near-linear scaling, excellent scan resistance per shard
//! - **Cons**: Protection is per-shard, not global. An item hot in shard A
//!   doesn't influence protection in shard B.
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get/Put/Remove | O(1) average |
//! | Concurrency | Near-linear scaling up to shard count |
//! | Memory overhead | ~140 bytes per entry + one Mutex per shard |
//! | Scan resistance | Good (two-tier protection) |
//!
//! # When to Use
//!
//! **Use ConcurrentSlruCache when:**
//! - Multiple threads need cache access
//! - Workload mixes hot items with sequential scans
//! - You need scan resistance in a concurrent environment
//! - Some items are accessed repeatedly while others are one-time
//!
//! **Consider alternatives when:**
//! - Single-threaded access only → use `SlruCache`
//! - Pure recency patterns → use `ConcurrentLruCache` (simpler)
//! - Frequency-dominant patterns → use `ConcurrentLfuCache`
//! - Need global protection coordination → use `Mutex<SlruCache>`
//!
//! # Thread Safety
//!
//! `ConcurrentSlruCache` is `Send + Sync` and can be shared via `Arc`.
//!
//! # Example
//!
//! ```rust,ignore
//! use cache_rs::concurrent::ConcurrentSlruCache;
//! use cache_rs::config::{ConcurrentSlruCacheConfig, ConcurrentCacheConfig, SlruCacheConfig};
//! use std::num::NonZeroUsize;
//! use std::sync::Arc;
//! use std::thread;
//!
//! // Total capacity 10000, protected segment 2000 (20%)
//! let config: ConcurrentSlruCacheConfig = ConcurrentCacheConfig {
//!     base: SlruCacheConfig {
//!         capacity: NonZeroUsize::new(10_000).unwrap(),
//!         protected_capacity: NonZeroUsize::new(2_000).unwrap(),
//!         max_size: u64::MAX,
//!     },
//!     segments: 16,
//! };
//! let cache = Arc::new(ConcurrentSlruCache::init(config, None));
//!
//! // Thread 1: Establish hot items
//! let cache1 = Arc::clone(&cache);
//! let hot_handle = thread::spawn(move || {
//!     for i in 0..100 {
//!         let key = format!("hot-{}", i);
//!         cache1.put(key.clone(), i);
//!         // Second access promotes to protected
//!         let _ = cache1.get(&key);
//!     }
//! });
//!
//! // Thread 2: Sequential scan (shouldn't evict hot items)
//! let cache2 = Arc::clone(&cache);
//! let scan_handle = thread::spawn(move || {
//!     for i in 0..50000 {
//!         cache2.put(format!("scan-{}", i), i as i32);
//!         // No second access - stays in probationary
//!     }
//! });
//!
//! hot_handle.join().unwrap();
//! scan_handle.join().unwrap();
//!
//! // Hot items should still be accessible (protected from scan)
//! for i in 0..100 {
//!     assert!(cache.get(&format!("hot-{}", i)).is_some());
//! }
//! ```

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
use hashbrown::DefaultHashBuilder;

#[cfg(not(feature = "hashbrown"))]
use std::collections::hash_map::RandomState as DefaultHashBuilder;

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
    /// Creates a new concurrent SLRU cache from a configuration.
    ///
    /// This is the **recommended** way to create a concurrent SLRU cache.
    ///
    /// # Arguments
    /// * `config` - The cache configuration
    /// * `hasher` - Optional custom hasher. If `None`, uses the default hasher.
    pub fn init(
        config: crate::config::ConcurrentSlruCacheConfig,
        hasher: Option<DefaultHashBuilder>,
    ) -> Self {
        let segment_count = config.segments;
        let capacity = config.base.capacity;
        let protected_capacity = config.base.protected_capacity;
        let max_size = config.base.max_size;

        let hash_builder = hasher.unwrap_or_default();

        let segment_capacity = capacity.get() / segment_count;
        let segment_protected = protected_capacity.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();
        let segment_protected_cap = NonZeroUsize::new(segment_protected.max(1)).unwrap();
        let segment_max_size = max_size / segment_count as u64;

        let segments: Vec<_> = (0..segment_count)
            .map(|_| {
                let segment_config = crate::config::SlruCacheConfig {
                    capacity: segment_cap,
                    protected_capacity: segment_protected_cap,
                    max_size: segment_max_size,
                };
                Mutex::new(SlruSegment::init(segment_config, hash_builder.clone()))
            })
            .collect();

        Self {
            segments: segments.into_boxed_slice(),
            hash_builder,
        }
    }
}

impl<K, V, S> ConcurrentSlruCache<K, V, S>
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

    /// Inserts a key-value pair with explicit size tracking.
    pub fn put_with_size(&self, key: K, value: V, size: u64) -> Option<(K, V)> {
        let idx = self.segment_index(&key);
        let mut segment = self.segments[idx].lock();
        segment.put_with_size(key, value, size)
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

    /// Returns the current total size of cached content across all segments.
    pub fn current_size(&self) -> u64 {
        self.segments.iter().map(|s| s.lock().current_size()).sum()
    }

    /// Returns the maximum content size the cache can hold across all segments.
    pub fn max_size(&self) -> u64 {
        self.segments.iter().map(|s| s.lock().max_size()).sum()
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
    use crate::config::{ConcurrentCacheConfig, ConcurrentSlruCacheConfig, SlruCacheConfig};

    extern crate std;
    use std::string::ToString;
    use std::sync::Arc;
    use std::thread;
    use std::vec::Vec;

    fn make_config(
        capacity: usize,
        protected: usize,
        segments: usize,
    ) -> ConcurrentSlruCacheConfig {
        ConcurrentCacheConfig {
            base: SlruCacheConfig {
                capacity: NonZeroUsize::new(capacity).unwrap(),
                protected_capacity: NonZeroUsize::new(protected).unwrap(),
                max_size: u64::MAX,
            },
            segments,
        }
    }

    #[test]
    fn test_basic_operations() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), Some(2));
    }

    #[test]
    fn test_concurrent_access() {
        let cache: Arc<ConcurrentSlruCache<String, i32>> =
            Arc::new(ConcurrentSlruCache::init(make_config(1000, 500, 16), None));
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
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        // Capacity is distributed across segments
        let capacity = cache.capacity();
        assert!(capacity >= 16);
        assert!(capacity <= 100);
    }

    #[test]
    fn test_segment_count() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 8), None);

        assert_eq!(cache.segment_count(), 8);
    }

    #[test]
    fn test_len_and_is_empty() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

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
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        cache.put("key1".to_string(), 1);
        cache.put("key2".to_string(), 2);

        assert_eq!(cache.remove(&"key1".to_string()), Some(1));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&"key1".to_string()), None);

        assert_eq!(cache.remove(&"nonexistent".to_string()), None);
    }

    #[test]
    fn test_clear() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

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
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        cache.put("exists".to_string(), 1);

        assert!(cache.contains_key(&"exists".to_string()));
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_get_with() {
        let cache: ConcurrentSlruCache<String, String> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        cache.put("key".to_string(), "hello world".to_string());

        let len = cache.get_with(&"key".to_string(), |v: &String| v.len());
        assert_eq!(len, Some(11));

        let missing = cache.get_with(&"missing".to_string(), |v: &String| v.len());
        assert_eq!(missing, None);
    }

    #[test]
    fn test_eviction_on_capacity() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(80, 48, 16), None);

        // Fill the cache
        for i in 0..10 {
            cache.put(std::format!("key{}", i), i);
        }

        // Cache should not exceed capacity
        assert!(cache.len() <= 80);
    }

    #[test]
    fn test_promotion_to_protected() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(160, 80, 16), None);

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
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        let metrics = cache.metrics();
        // Metrics aggregation across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_algorithm_name() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        assert_eq!(cache.algorithm_name(), "ConcurrentSLRU");
    }

    #[test]
    fn test_empty_cache_operations() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&"missing".to_string()), None);
        assert_eq!(cache.remove(&"missing".to_string()), None);
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_borrowed_key_lookup() {
        let cache: ConcurrentSlruCache<String, i32> =
            ConcurrentSlruCache::init(make_config(100, 50, 16), None);

        cache.put("test_key".to_string(), 42);

        // Test with borrowed key
        let key_str = "test_key";
        assert_eq!(cache.get(key_str), Some(42));
        assert!(cache.contains_key(key_str));
        assert_eq!(cache.remove(key_str), Some(42));
    }
}
