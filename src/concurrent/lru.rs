//! Concurrent LRU Cache Implementation
//!
//! Provides a thread-safe LRU cache using segmented storage for high-performance
//! multi-threaded access. This is the concurrent equivalent of [`LruCache`][crate::LruCache].
//!
//! # How It Works
//!
//! The key space is partitioned across multiple segments using hash-based sharding.
//! Each segment is protected by its own lock, allowing
//! concurrent access to different segments without contention.
//!
//! # Performance Characteristics
//!
//! - **Time Complexity**: O(1) average for get, put, remove
//! - **Concurrency**: Near-linear scaling up to segment count
//! - **Overhead**: One Mutex per segment (~16 by default)
//!
//! # When to Use
//!
//! Use `ConcurrentLruCache` when:
//! - Multiple threads need to access the same cache
//! - You need higher throughput than a single `Mutex<LruCache>`
//! - Your workload has good key distribution
//!
//! # Thread Safety
//!
//! `ConcurrentLruCache` implements `Send` and `Sync` and can be safely shared
//! across threads via `Arc`.

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
use hashbrown::hash_map::DefaultHashBuilder;

#[cfg(not(feature = "hashbrown"))]
use std::collections::hash_map::RandomState as DefaultHashBuilder;

use super::default_segment_count;

/// A thread-safe LRU cache with segmented storage for high concurrency.
///
/// This cache partitions the key space across multiple segments, each protected
/// by its own lock. This allows concurrent access to different segments while
/// maintaining LRU semantics within each segment.
///
/// # Type Parameters
///
/// - `K`: Key type, must implement `Hash + Eq + Clone + Send`
/// - `V`: Value type, must implement `Clone + Send`
/// - `S`: Hash builder type, defaults to `DefaultHashBuilder`
///
/// # Example
///
/// ```rust,ignore
/// use cache_rs::concurrent::ConcurrentLruCache;
/// use std::sync::Arc;
/// use std::thread;
///
/// let cache = Arc::new(ConcurrentLruCache::new(1000));
///
/// // Use from multiple threads
/// let cache_clone = Arc::clone(&cache);
/// thread::spawn(move || {
///     cache_clone.put("key".to_string(), 42);
/// });
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
    /// Creates a new concurrent LRU cache with the specified total capacity.
    ///
    /// The capacity is distributed evenly across the default number of segments.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Total capacity across all segments
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use cache_rs::concurrent::ConcurrentLruCache;
    ///
    /// let cache: ConcurrentLruCache<String, i32> = ConcurrentLruCache::new(1000);
    /// ```
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self::with_segments(capacity, default_segment_count())
    }

    /// Creates a new concurrent LRU cache with the specified capacity and segment count.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Total capacity across all segments
    /// * `segment_count` - Number of segments to use (should be a power of 2 for best performance)
    ///
    /// # Panics
    ///
    /// Panics if `segment_count` is 0 or if `capacity < segment_count`.
    pub fn with_segments(capacity: NonZeroUsize, segment_count: usize) -> Self {
        assert!(segment_count > 0, "segment_count must be greater than 0");
        assert!(
            capacity.get() >= segment_count,
            "capacity must be >= segment_count"
        );

        Self::with_segments_and_hasher(capacity, segment_count, DefaultHashBuilder::default())
    }
}

impl<K, V, S> ConcurrentLruCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    /// Creates a new concurrent LRU cache with custom hasher.
    pub fn with_segments_and_hasher(
        capacity: NonZeroUsize,
        segment_count: usize,
        hash_builder: S,
    ) -> Self {
        let segment_capacity = capacity.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();

        let segments: Vec<_> = (0..segment_count)
            .map(|_| Mutex::new(LruSegment::with_hasher(segment_cap, hash_builder.clone())))
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
    ///
    /// # Returns
    ///
    /// Returns `Some(value)` if the key exists, `None` otherwise.
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
    ///
    /// # Example
    ///
    /// ```rust,ignore
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

    /// Gets a mutable reference to a value and applies a function to it.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// cache.get_mut_with(&key, |value| *value += 1);
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
    /// If the key already exists, the value is updated and the old value is returned.
    /// If the cache is at capacity, the least recently used entry is evicted.
    ///
    /// # Returns
    ///
    /// Returns `Some((key, value))` if an entry was evicted or updated, `None` otherwise.
    pub fn put(&self, key: K, value: V) -> Option<(K, V)> {
        let idx = self.segment_index(&key);
        let mut segment = self.segments[idx].lock();
        segment.put(key, value)
    }

    /// Removes a key from the cache.
    ///
    /// # Returns
    ///
    /// Returns `Some(value)` if the key existed, `None` otherwise.
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

    /// Removes and returns an eviction candidate from any segment.
    ///
    /// This iterates through segments and returns the first item that can be popped.
    /// Note that in a concurrent setting, this may not return the globally "oldest"
    /// item, but rather the oldest item from the first non-empty segment.
    ///
    /// # Returns
    ///
    /// Returns `Some((key, value))` if an item was popped, `None` if all segments are empty.
    pub fn pop(&self) -> Option<(K, V)> {
        for segment in self.segments.iter() {
            let mut guard = segment.lock();
            if let Some(item) = guard.pop() {
                return Some(item);
            }
        }
        None
    }

    /// Removes and returns the most recently used item from any segment (reverse of pop).
    ///
    /// This is the opposite of `pop()` - it iterates through segments and returns
    /// the first MRU item found instead of the LRU item.
    ///
    /// # Returns
    ///
    /// Returns `Some((key, value))` if an item was popped, `None` if all segments are empty.
    pub fn popr(&self) -> Option<(K, V)> {
        for segment in self.segments.iter() {
            let mut guard = segment.lock();
            if let Some(item) = guard.popr() {
                return Some(item);
            }
        }
        None
    }

    /// Records a cache miss for metrics purposes.
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

    extern crate std;
    use std::string::ToString;
    use std::sync::Arc;
    use std::thread;
    use std::vec::Vec;

    #[test]
    fn test_basic_operations() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

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
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("key".to_string(), "hello world".to_string());

        let len = cache.get_with(&"key".to_string(), |v: &String| v.len());
        assert_eq!(len, Some(11));

        let missing = cache.get_with(&"missing".to_string(), |v: &String| v.len());
        assert_eq!(missing, None);
    }

    #[test]
    fn test_get_mut_with() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("counter".to_string(), 0);

        cache.get_mut_with(&"counter".to_string(), |v: &mut i32| *v += 1);
        cache.get_mut_with(&"counter".to_string(), |v: &mut i32| *v += 1);

        assert_eq!(cache.get(&"counter".to_string()), Some(2));
    }

    #[test]
    fn test_remove() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

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
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

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
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("exists".to_string(), 1);

        assert!(cache.contains_key(&"exists".to_string()));
        assert!(!cache.contains_key(&"missing".to_string()));
    }

    #[test]
    fn test_concurrent_access() {
        let cache: Arc<ConcurrentLruCache<String, i32>> =
            Arc::new(ConcurrentLruCache::new(NonZeroUsize::new(1000).unwrap()));
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
            Arc::new(ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap()));
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
            ConcurrentLruCache::with_segments(NonZeroUsize::new(100).unwrap(), 8);

        assert_eq!(cache.segment_count(), 8);
    }

    #[test]
    fn test_capacity() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

        // Capacity is distributed across segments, so it may not be exactly 100
        // It should be close to the requested capacity
        let capacity = cache.capacity();
        assert!(capacity >= 16); // At least 1 per segment
        assert!(capacity <= 100); // Not more than requested
    }

    #[test]
    fn test_eviction_on_capacity() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::with_segments(NonZeroUsize::new(48).unwrap(), 16);

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
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("key".to_string(), 1);
        assert_eq!(cache.get(&"key".to_string()), Some(1));

        cache.put("key".to_string(), 2);
        assert_eq!(cache.get(&"key".to_string()), Some(2));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_lru_ordering() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::with_segments(NonZeroUsize::new(48).unwrap(), 16);

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
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

        cache.put("a".to_string(), 1);
        cache.put("b".to_string(), 2);

        let metrics = cache.metrics();
        // Metrics aggregation across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_record_miss() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

        cache.record_miss(100);
        cache.record_miss(200);

        let metrics = cache.metrics();
        // Metrics are aggregated across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_empty_cache_operations() {
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

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
            ConcurrentLruCache::with_segments(NonZeroUsize::new(16).unwrap(), 16);

        cache.put("a".to_string(), 1);
        assert!(!cache.is_empty());

        cache.put("b".to_string(), 2);
        assert!(cache.len() <= 16);
    }

    #[test]
    fn test_with_segments_and_hasher() {
        let hasher = DefaultHashBuilder::default();
        let cache: ConcurrentLruCache<String, i32> = ConcurrentLruCache::with_segments_and_hasher(
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
        let cache: ConcurrentLruCache<String, i32> =
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

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
            ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

        assert_eq!(cache.algorithm_name(), "ConcurrentLRU");
    }
}
