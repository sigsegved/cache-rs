//! Concurrent LRU Cache Implementation
//!
//! A thread-safe LRU cache using lock striping (segmented storage) for high-performance
//! concurrent access. This is the multi-threaded counterpart to [`LruCache`](crate::LruCache).
//!
//! # How It Works
//!
//! The cache partitions keys across multiple independent segments, each with its own lock.
//! This allows concurrent operations on different segments without contention.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────┐
//! │                      ConcurrentLruCache                              │
//! │                                                                      │
//! │  hash(key) % N  ──▶  Segment Selection                               │
//! │                                                                      │
//! │  ┌──────────────┐ ┌──────────────┐     ┌──────────────┐              │
//! │  │  Segment 0   │ │  Segment 1   │ ... │  Segment N-1 │              │
//! │  │  ┌────────┐  │ │  ┌────────┐  │     │  ┌────────┐  │              │
//! │  │  │ Mutex  │  │ │  │ Mutex  │  │     │  │ Mutex  │  │              │
//! │  │  └────┬───┘  │ │  └────┬───┘  │     │  └────┬───┘  │              │
//! │  │       │      │ │       │      │     │       │      │              │
//! │  │  ┌────▼───┐  │ │  ┌────▼───┐  │     │  ┌────▼───┐  │              │
//! │  │  │LruCache│  │ │  │LruCache│  │     │  │LruCache│  │              │
//! │  │  └────────┘  │ │  └────────┘  │     │  └────────┘  │              │
//! │  └──────────────┘ └──────────────┘     └──────────────┘              │
//! └──────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Segment Count
//!
//! The default segment count is based on available CPU cores (typically 16).
//! More segments = less contention but more memory overhead.
//!
//! ## Trade-offs
//!
//! - **Pros**: Near-linear scaling with thread count, no global lock
//! - **Cons**: LRU ordering is per-segment, not global. An item might be evicted
//!   from one segment while another segment has older items.
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get/Put/Remove | O(1) average |
//! | Concurrency | Near-linear scaling up to segment count |
//! | Memory overhead | ~150 bytes per entry + one Mutex per segment |
//!
//! # When to Use
//!
//! **Use ConcurrentLruCache when:**
//! - Multiple threads need cache access
//! - You need better throughput than `Mutex<LruCache>`
//! - Keys distribute evenly (hot keys in one segment will still contend)
//!
//! **Consider alternatives when:**
//! - Single-threaded access only → use `LruCache`
//! - Need strict global LRU ordering → use `Mutex<LruCache>`
//! - Very hot keys → consider per-key caching or request coalescing
//!
//! # Thread Safety
//!
//! `ConcurrentLruCache` is `Send + Sync` and can be shared via `Arc`.
//!
//! # Example
//!
//! ```rust,ignore
//! use cache_rs::concurrent::ConcurrentLruCache;
//! use cache_rs::config::{ConcurrentLruCacheConfig, ConcurrentCacheConfig, LruCacheConfig};
//! use std::num::NonZeroUsize;
//! use std::sync::Arc;
//! use std::thread;
//!
//! let config: ConcurrentLruCacheConfig = ConcurrentCacheConfig {
//!     base: LruCacheConfig {
//!         capacity: NonZeroUsize::new(10_000).unwrap(),
//!         max_size: u64::MAX,
//!     },
//!     segments: 16,
//! };
//! let cache = Arc::new(ConcurrentLruCache::init(config, None));
//!
//! let handles: Vec<_> = (0..4).map(|i| {
//!     let cache = Arc::clone(&cache);
//!     thread::spawn(move || {
//!         for j in 0..1000 {
//!             cache.put(format!("key-{}-{}", i, j), j);
//!         }
//!     })
//! }).collect();
//!
//! for h in handles {
//!     h.join().unwrap();
//! }
//!
//! println!("Total entries: {}", cache.len());
//! ```

extern crate alloc;

use crate::lru::LruSegment;
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

/// A thread-safe LRU cache with segmented storage for high concurrency.
///
/// Keys are partitioned across multiple segments using hash-based sharding.
/// Each segment has its own lock, allowing concurrent access to different
/// segments without blocking.
///
/// # Type Parameters
///
/// - `K`: Key type. Must implement `Hash + Eq + Clone + Send`.
/// - `V`: Value type. Must implement `Clone + Send`.
/// - `S`: Hash builder type. Defaults to `DefaultHashBuilder`.
///
/// # Note on LRU Semantics
///
/// LRU ordering is maintained **per-segment**, not globally. This means an item
/// in segment A might be evicted while segment B has items that were accessed
/// less recently in wall-clock time. For most workloads with good key distribution,
/// this approximation works well.
///
/// # Example
///
/// ```rust,ignore
/// use cache_rs::concurrent::ConcurrentLruCache;
/// use std::sync::Arc;
///
/// let cache = Arc::new(ConcurrentLruCache::new(1000));
///
/// // Safe to use from multiple threads
/// cache.put("key".to_string(), 42);
/// assert_eq!(cache.get(&"key".to_string()), Some(42));
/// ```
pub struct ConcurrentLruCache<K, V, S = DefaultHashBuilder> {
    segments: Box<[Mutex<LruSegment<K, V, S>>]>,
    hash_builder: S,
}

impl<K, V> ConcurrentLruCache<K, V, DefaultHashBuilder>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    /// Creates a new concurrent LRU cache from a configuration with an optional hasher.
    ///
    /// This is the **recommended** way to create a concurrent LRU cache.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity, segments, and optional size limit
    /// * `hasher` - Optional custom hash builder. If `None`, uses `DefaultHashBuilder`
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use cache_rs::concurrent::ConcurrentLruCache;
    /// use cache_rs::config::{ConcurrentLruCacheConfig, ConcurrentCacheConfig, LruCacheConfig};
    /// use core::num::NonZeroUsize;
    ///
    /// // Simple capacity-only cache with default segments
    /// let config: ConcurrentLruCacheConfig = ConcurrentCacheConfig {
    ///     base: LruCacheConfig {
    ///         capacity: NonZeroUsize::new(10000).unwrap(),
    ///         max_size: u64::MAX,
    ///     },
    ///     segments: 16,
    /// };
    /// let cache: ConcurrentLruCache<String, i32> = ConcurrentLruCache::init(config, None);
    ///
    /// // With custom segments and size limit
    /// let config: ConcurrentLruCacheConfig = ConcurrentCacheConfig {
    ///     base: LruCacheConfig {
    ///         capacity: NonZeroUsize::new(10000).unwrap(),
    ///         max_size: 100 * 1024 * 1024,  // 100MB
    ///     },
    ///     segments: 32,
    /// };
    /// let cache: ConcurrentLruCache<String, Vec<u8>> = ConcurrentLruCache::init(config, None);
    /// ```
    pub fn init(
        config: crate::config::ConcurrentLruCacheConfig,
        hasher: Option<DefaultHashBuilder>,
    ) -> Self {
        let segment_count = config.segments;
        let capacity = config.base.capacity;
        let max_size = config.base.max_size;

        let segment_capacity = capacity.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();
        let segment_max_size = max_size / segment_count as u64;

        let hash_builder = hasher.unwrap_or_default();
        let segments: Vec<_> = (0..segment_count)
            .map(|_| {
                Mutex::new(crate::lru::LruSegment::with_hasher_and_size(
                    segment_cap,
                    hash_builder.clone(),
                    segment_max_size,
                ))
            })
            .collect();

        Self {
            segments: segments.into_boxed_slice(),
            hash_builder,
        }
    }
}

impl<K, V, S> ConcurrentLruCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    /// Creates a concurrent LRU cache with a custom hash builder.
    ///
    /// Use this for deterministic hashing or DoS-resistant hashers.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity and segments
    /// * `hash_builder` - Custom hash builder (will be cloned for each segment)
    pub fn init_with_hasher(
        config: crate::config::ConcurrentLruCacheConfig,
        hash_builder: S,
    ) -> Self {
        let segment_count = config.segments;
        let capacity = config.base.capacity;
        let max_size = config.base.max_size;

        let segment_capacity = capacity.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();
        let segment_max_size = max_size / segment_count as u64;

        let segments: Vec<_> = (0..segment_count)
            .map(|_| {
                Mutex::new(LruSegment::with_hasher_and_size(
                    segment_cap,
                    hash_builder.clone(),
                    segment_max_size,
                ))
            })
            .collect();

        Self {
            segments: segments.into_boxed_slice(),
            hash_builder,
        }
    }

    /// Returns the segment index for the given key.
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
    ///
    /// Note: This acquires a lock on each segment sequentially, so the
    /// returned value may be slightly stale in high-concurrency scenarios.
    pub fn len(&self) -> usize {
        self.segments.iter().map(|s| s.lock().len()).sum()
    }

    /// Returns `true` if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|s| s.lock().is_empty())
    }

    /// Retrieves a value from the cache.
    ///
    /// Returns a **clone** of the value to avoid holding the lock. For operations
    /// that don't need ownership, use [`get_with()`](Self::get_with) instead.
    ///
    /// If the key exists, it is moved to the MRU position within its segment.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let value = cache.get(&"key".to_string());
    /// ```
    pub fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.get(key).cloned()
    }

    /// Retrieves a value and applies a function to it while holding the lock.
    ///
    /// More efficient than `get()` when you only need to read from the value,
    /// as it avoids cloning. The lock is released after `f` returns.
    ///
    /// # Type Parameters
    ///
    /// - `F`: Function that takes `&V` and returns `R`
    /// - `R`: Return type of the function
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Get length without cloning the whole string
    /// let len = cache.get_with(&key, |value| value.len());
    /// ```
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

    /// Retrieves a mutable reference and applies a function to it.
    ///
    /// Allows in-place modification of cached values without removing them.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Increment a counter in-place
    /// cache.get_mut_with(&"counter".to_string(), |value| *value += 1);
    /// ```
    pub fn get_mut_with<Q, F, R>(&self, key: &Q, f: F) -> Option<R>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        F: FnOnce(&mut V) -> R,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.get_mut(key).map(f)
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the key exists, the value is updated and moved to MRU position.
    /// If at capacity, the LRU entry in the target segment is evicted.
    ///
    /// # Returns
    ///
    /// - `Some((old_key, old_value))` if key existed or entry was evicted
    /// - `None` if inserted with available capacity
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// cache.put("key".to_string(), 42);
    /// ```
    pub fn put(&self, key: K, value: V) -> Option<(K, V)> {
        let idx = self.segment_index(&key);
        let mut segment = self.segments[idx].lock();
        segment.put(key, value)
    }

    /// Inserts a key-value pair with explicit size tracking.
    ///
    /// Use this for size-aware caching. The size is used for `max_size` tracking
    /// and eviction decisions.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to insert
    /// * `value` - The value to cache
    /// * `size` - Size of this entry (in your chosen unit)
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let data = vec![0u8; 1024];
    /// cache.put_with_size("file".to_string(), data, 1024);
    /// ```
    pub fn put_with_size(&self, key: K, value: V, size: u64) -> Option<(K, V)> {
        let idx = self.segment_index(&key);
        let mut segment = self.segments[idx].lock();
        segment.put_with_size(key, value, size)
    }

    /// Removes a key from the cache.
    ///
    /// # Returns
    ///
    /// - `Some(value)` if the key existed
    /// - `None` if the key was not found
    pub fn remove<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.remove(key)
    }

    /// Checks if the cache contains a key.
    ///
    /// Note: This **does** update the entry's recency (moves to MRU position).
    /// If you need a pure existence check without side effects, this isn't it.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let mut segment = self.segments[idx].lock();
        segment.get(key).is_some()
    }

    /// Removes all entries from all segments.
    ///
    /// Acquires locks on each segment sequentially.
    pub fn clear(&self) {
        for segment in self.segments.iter() {
            segment.lock().clear();
        }
    }

    /// Returns the current total size across all segments.
    ///
    /// This is the sum of all `size` values from `put_with_size()` calls.
    pub fn current_size(&self) -> u64 {
        self.segments.iter().map(|s| s.lock().current_size()).sum()
    }

    /// Returns the maximum total content size across all segments.
    pub fn max_size(&self) -> u64 {
        self.segments.iter().map(|s| s.lock().max_size()).sum()
    }

    /// Records a cache miss for metrics tracking.
    ///
    /// Call this after a failed `get()` when you fetch from the origin.
    pub fn record_miss(&self, object_size: u64) {
        // Record on the first segment (metrics are aggregated anyway)
        if let Some(segment) = self.segments.first() {
            segment.lock().record_miss(object_size);
        }
    }
}

impl<K, V, S> CacheMetrics for ConcurrentLruCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    fn metrics(&self) -> BTreeMap<String, f64> {
        // Aggregate metrics from all segments
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
        "ConcurrentLRU"
    }
}

// SAFETY: ConcurrentLruCache uses Mutex for synchronization, making it safe to
// send and share across threads when K and V are Send.
unsafe impl<K: Send, V: Send, S: Send> Send for ConcurrentLruCache<K, V, S> {}
unsafe impl<K: Send, V: Send, S: Send + Sync> Sync for ConcurrentLruCache<K, V, S> {}

impl<K, V, S> core::fmt::Debug for ConcurrentLruCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConcurrentLruCache")
            .field("segment_count", &self.segments.len())
            .field("total_len", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConcurrentCacheConfig, ConcurrentLruCacheConfig, LruCacheConfig};

    extern crate std;
    use std::string::ToString;
    use std::sync::Arc;
    use std::thread;
    use std::vec::Vec;

    fn make_config(capacity: usize, segments: usize) -> ConcurrentLruCacheConfig {
        ConcurrentCacheConfig {
            base: LruCacheConfig {
                capacity: NonZeroUsize::new(capacity).unwrap(),
                max_size: u64::MAX,
            },
            segments,
        }
    }

    #[test]
    fn test_basic_operations() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);

        assert_eq!(cache.len(), 3);
        assert!(!cache.is_empty());

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), Some(2));
        assert_eq!(cache.get(&"c".to_string()), Some(3));
        assert_eq!(cache.get(&"d".to_string()), None);
    }

    #[test]
    fn test_get_with() {
        let cache: ConcurrentLruCache<String, String> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.put("key".to_string(), "hello world".to_string());

        let len = cache.get_with(&"key".to_string(), |v: &String| v.len());
        assert_eq!(len, Some(11));

        let missing = cache.get_with(&"missing".to_string(), |v: &String| v.len());
        assert_eq!(missing, None);
    }

    #[test]
    fn test_get_mut_with() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.put("counter".to_string(), 0);

        cache.get_mut_with(&"counter".to_string(), |v: &mut i32| *v += 1);
        cache.get_mut_with(&"counter".to_string(), |v: &mut i32| *v += 1);

        assert_eq!(cache.get(&"counter".to_string()), Some(2));
    }

    #[test]
    fn test_remove() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        assert_eq!(cache.remove(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"a".to_string()), None);
        assert_eq!(cache.len(), 1);

        assert_eq!(cache.remove(&"nonexistent".to_string()), None);
    }

    #[test]
    fn test_clear() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);

        assert_eq!(cache.len(), 3);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_contains_key() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.put("exists".to_string(), 1);

        assert!(cache.contains_key(&"exists".to_string()));
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_concurrent_access() {
        let cache: Arc<ConcurrentLruCache<String, i32>> =
            Arc::new(ConcurrentLruCache::init(make_config(1000, 16), None));
        let num_threads = 8;
        let ops_per_thread = 1000;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("thread_{}_key_{}", t, i);
                    cache.put(key.clone(), t * 1000 + i);
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
    fn test_concurrent_mixed_operations() {
        let cache: Arc<ConcurrentLruCache<String, i32>> =
            Arc::new(ConcurrentLruCache::init(make_config(100, 16), None));
        let num_threads = 8;
        let ops_per_thread = 500;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}", i % 200);

                    match i % 4 {
                        0 => {
                            cache.put(key, i);
                        }
                        1 => {
                            let _ = cache.get(&key);
                        }
                        2 => {
                            cache.get_mut_with(&key, |v: &mut i32| *v += 1);
                        }
                        3 => {
                            let _ = cache.remove(&key);
                        }
                        _ => unreachable!(),
                    }

                    if i == 250 && t == 0 {
                        cache.clear();
                    }
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        // Cache should be in valid state
        assert!(cache.len() <= 100);
    }

    #[test]
    fn test_segment_count() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 8), None);

        assert_eq!(cache.segment_count(), 8);
    }

    #[test]
    fn test_capacity() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        // Capacity is distributed across segments, so it may not be exactly 100
        // It should be close to the requested capacity
        let capacity = cache.capacity();
        assert!(capacity >= 16); // At least 1 per segment
        assert!(capacity <= 100); // Not more than requested
    }

    #[test]
    fn test_eviction_on_capacity() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(48, 16), None);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);

        assert_eq!(cache.len(), 3);

        // This should evict the LRU item
        cache.put("d".to_string(), 4);

        assert!(cache.len() <= 48);
        assert!(cache.contains_key(&"d".to_string()));
    }

    #[test]
    fn test_update_existing_key() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.put("key".to_string(), 1);
        assert_eq!(cache.get(&"key".to_string()), Some(1));

        cache.put("key".to_string(), 2);
        assert_eq!(cache.get(&"key".to_string()), Some(2));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_lru_ordering() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(48, 16), None);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);

        // Access "a" to make it recently used
        let _ = cache.get(&"a".to_string());

        // Add a new item
        cache.put("d".to_string(), 4);

        assert!(cache.contains_key(&"a".to_string()));
        assert!(cache.contains_key(&"d".to_string()));
    }

    #[test]
    fn test_metrics() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        let metrics = cache.metrics();
        // Metrics aggregation across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_record_miss() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.record_miss(100);
        cache.record_miss(200);

        let metrics = cache.metrics();
        // Metrics are aggregated across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_empty_cache_operations() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&"missing".to_string()), None);
        assert_eq!(cache.remove(&"missing".to_string()), None);
        assert!(!cache.contains_key(&"missing".to_string()));

        let result = cache.get_with(&"missing".to_string(), |v: &i32| *v);
        assert_eq!(result, None);
    }

    #[test]
    fn test_single_item_cache() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(16, 16), None);

        cache.put("a".to_string(), 1);
        assert!(!cache.is_empty());

        cache.put("b".to_string(), 2);
        assert!(cache.len() <= 16);
    }

    #[test]
    fn test_init_with_hasher() {
        let hasher = DefaultHashBuilder::default();
        let config = make_config(100, 4);
        let cache: ConcurrentLruCache<String, i32, _> =
            ConcurrentLruCache::init_with_hasher(config, hasher);

        cache.put("test".to_string(), 42);
        assert_eq!(cache.get(&"test".to_string()), Some(42));
        assert_eq!(cache.segment_count(), 4);
    }

    #[test]
    fn test_borrowed_key_lookup() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        cache.put("test_key".to_string(), 42);

        // Test with borrowed key
        let key_str = "test_key";
        assert_eq!(cache.get(key_str), Some(42));
        assert!(cache.contains_key(key_str));
        assert_eq!(cache.remove(key_str), Some(42));
    }

    #[test]
    fn test_algorithm_name() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::init(make_config(100, 16), None);

        assert_eq!(cache.algorithm_name(), "ConcurrentLRU");
    }
}
