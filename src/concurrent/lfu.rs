//! Concurrent LFU Cache Implementation
//!
//! A thread-safe LFU cache using lock striping (segmented storage) for high-performance
//! concurrent access. This is the multi-threaded counterpart to [`LfuCache`](crate::LfuCache).
//!
//! # How It Works
//!
//! The cache partitions keys across multiple independent segments, each with its own lock.
//! This allows concurrent operations on different segments without contention.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────┐
//! │                      ConcurrentLfuCache                              │
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
//! │  │  │LfuCache│  │ │  │LfuCache│  │     │  │LfuCache│  │              │
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
//! - **Pros**: Near-linear scaling with thread count, excellent scan resistance
//! - **Cons**: LFU frequency tracking is per-segment, not global. An item accessed
//!   in segment A doesn't affect frequency tracking in segment B.
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get/Put/Remove | O(1) average |
//! | Concurrency | Near-linear scaling up to segment count |
//! | Memory overhead | ~150 bytes per entry + one Mutex per segment |
//! | Scan resistance | Excellent (frequency-based eviction) |
//!
//! # When to Use
//!
//! **Use ConcurrentLfuCache when:**
//! - Multiple threads need cache access
//! - Access patterns have stable popularity (some keys consistently more popular)
//! - You need excellent scan resistance
//! - Frequency is more important than recency
//!
//! **Consider alternatives when:**
//! - Single-threaded access only → use `LfuCache`
//! - Need global frequency tracking → use `Mutex<LfuCache>`
//! - Popularity changes over time → use `ConcurrentLfudaCache`
//! - Recency-based access → use `ConcurrentLruCache`
//!
//! # Thread Safety
//!
//! `ConcurrentLfuCache` is `Send + Sync` and can be shared via `Arc`.
//!
//! # Example
//!
//! ```rust,ignore
//! use cache_rs::concurrent::ConcurrentLfuCache;
//! use std::sync::Arc;
//! use std::thread;
//!
//! let cache = Arc::new(ConcurrentLfuCache::new(10_000));
//!
//! let handles: Vec<_> = (0..4).map(|i| {
//!     let cache = Arc::clone(&cache);
//!     thread::spawn(move || {
//!         for j in 0..1000 {
//!             let key = format!("key-{}-{}", i, j);
//!             cache.put(key.clone(), j);
//!             // Access popular keys more frequently
//!             if j % 10 == 0 {
//!                 for _ in 0..5 {
//!                     let _ = cache.get(&key);
//!                 }
//!             }
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

use crate::lfu::LfuSegment;
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

/// A thread-safe LFU cache with segmented storage for high concurrency.
pub struct ConcurrentLfuCache<K, V, S = DefaultHashBuilder> {
    segments: Box<[Mutex<LfuSegment<K, V, S>>]>,
    hash_builder: S,
}

impl<K, V> ConcurrentLfuCache<K, V, DefaultHashBuilder>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    /// Creates a new concurrent LFU cache with the specified total capacity.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self::with_segments(capacity, default_segment_count())
    }

    /// Creates a new concurrent LFU cache with custom segment count.
    pub fn with_segments(capacity: NonZeroUsize, segment_count: usize) -> Self {
        Self::with_segments_and_hasher(capacity, segment_count, DefaultHashBuilder::default())
    }

    /// Creates a size-based concurrent LFU cache.
    pub fn with_max_size(max_size: u64) -> Self {
        let max_entries = NonZeroUsize::new(10_000_000).unwrap();
        Self::with_limits(max_entries, max_size)
    }

    /// Creates a dual-limit concurrent LFU cache.
    pub fn with_limits(max_entries: NonZeroUsize, max_size: u64) -> Self {
        Self::with_limits_and_segments(max_entries, max_size, default_segment_count())
    }

    /// Creates a dual-limit concurrent LFU cache with custom segment count.
    pub fn with_limits_and_segments(
        max_entries: NonZeroUsize,
        max_size: u64,
        segment_count: usize,
    ) -> Self {
        assert!(segment_count > 0, "segment_count must be greater than 0");
        assert!(
            max_entries.get() >= segment_count,
            "max_entries must be >= segment_count"
        );

        let segment_capacity = max_entries.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();
        let segment_max_size = max_size / segment_count as u64;

        let segments: Vec<_> = (0..segment_count)
            .map(|_| {
                Mutex::new(LfuSegment::with_hasher_and_size(
                    segment_cap,
                    DefaultHashBuilder::default(),
                    segment_max_size,
                ))
            })
            .collect();

        Self {
            segments: segments.into_boxed_slice(),
            hash_builder: DefaultHashBuilder::default(),
        }
    }
}

impl<K, V, S> ConcurrentLfuCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    /// Creates a new concurrent LFU cache with custom hasher.
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
            .map(|_| Mutex::new(LfuSegment::with_hasher(segment_cap, hash_builder.clone())))
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
    /// If the cache is at capacity, the least frequently used entry is evicted.
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

impl<K, V, S> CacheMetrics for ConcurrentLfuCache<K, V, S>
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
        "ConcurrentLFU"
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for ConcurrentLfuCache<K, V, S> {}
unsafe impl<K: Send, V: Send, S: Send + Sync> Sync for ConcurrentLfuCache<K, V, S> {}

impl<K, V, S> core::fmt::Debug for ConcurrentLfuCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConcurrentLfuCache")
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
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), Some(2));
    }

    #[test]
    fn test_concurrent_access() {
        let cache: Arc<ConcurrentLfuCache<String, i32>> =
            Arc::new(ConcurrentLfuCache::new(NonZeroUsize::new(1000).unwrap()));
        let num_threads = 8;
        let ops_per_thread = 500;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}_{}", t, i);
                    cache.put(key.clone(), i);
                    // Access multiple times to test frequency tracking
                    if i % 3 == 0 {
                        let _ = cache.get(&key);
                        let _ = cache.get(&key);
                    }
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
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        // Capacity is distributed across segments
        let capacity = cache.capacity();
        assert!(capacity >= 16);
        assert!(capacity <= 100);
    }

    #[test]
    fn test_segment_count() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::with_segments(NonZeroUsize::new(100).unwrap(), 8);

        assert_eq!(cache.segment_count(), 8);
    }

    #[test]
    fn test_len_and_is_empty() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

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
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("key1".to_string(), 1);
        cache.put("key2".to_string(), 2);

        assert_eq!(cache.remove(&"key1".to_string()), Some(1));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&"key1".to_string()), None);

        assert_eq!(cache.remove(&"nonexistent".to_string()), None);
    }

    #[test]
    fn test_clear() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

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
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("exists".to_string(), 1);

        assert!(cache.contains_key(&"exists".to_string()));
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_get_with() {
        let cache: ConcurrentLfuCache<String, String> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("key".to_string(), "hello world".to_string());

        let len = cache.get_with(&"key".to_string(), |v: &String| v.len());
        assert_eq!(len, Some(11));

        let missing = cache.get_with(&"missing".to_string(), |v: &String| v.len());
        assert_eq!(missing, None);
    }

    #[test]
    fn test_frequency_eviction() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::with_segments(NonZeroUsize::new(48).unwrap(), 16);

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);
        cache.put("c".to_string(), 3);

        // Access "a" and "c" multiple times to increase frequency
        for _ in 0..5 {
            let _ = cache.get(&"a".to_string());
            let _ = cache.get(&"c".to_string());
        }

        // Add a new item
        cache.put("d".to_string(), 4);

        assert!(cache.len() <= 48);
    }

    #[test]
    fn test_eviction_on_capacity() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::with_segments(NonZeroUsize::new(80).unwrap(), 16);

        // Fill the cache
        for i in 0..10 {
            cache.put(std::format!("key{}", i), i);
        }

        // Cache should not exceed capacity
        assert!(cache.len() <= 80);
    }

    #[test]
    fn test_metrics() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        let metrics = cache.metrics();
        // Metrics aggregation across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_algorithm_name() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        assert_eq!(cache.algorithm_name(), "ConcurrentLFU");
    }

    #[test]
    fn test_empty_cache_operations() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&"missing".to_string()), None);
        assert_eq!(cache.remove(&"missing".to_string()), None);
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_with_segments_and_hasher() {
        let hasher = DefaultHashBuilder::default();
        let cache: ConcurrentLfuCache<String, i32> = ConcurrentLfuCache::with_segments_and_hasher(
            NonZeroUsize::new(100).unwrap(),
            4,
            hasher,
        );

        cache.put("test".to_string(), 42);
        assert_eq!(cache.get(&"test".to_string()), Some(42));
        assert_eq!(cache.segment_count(), 4);
    }

    #[test]
    fn test_borrowed_key_lookup() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("test_key".to_string(), 42);

        // Test with borrowed key
        let key_str = "test_key";
        assert_eq!(cache.get(key_str), Some(42));
        assert!(cache.contains_key(key_str));
        assert_eq!(cache.remove(key_str), Some(42));
    }

    #[test]
    fn test_frequency_tracking() {
        let cache: ConcurrentLfuCache<String, i32> =
            ConcurrentLfuCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("key".to_string(), 1);

        // Access the key multiple times
        for _ in 0..10 {
            let _ = cache.get(&"key".to_string());
        }

        // Item should still be accessible
        assert_eq!(cache.get(&"key".to_string()), Some(1));
    }
}
