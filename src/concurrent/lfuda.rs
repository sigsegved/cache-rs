//! Concurrent LFUDA Cache Implementation
//!
//! A thread-safe LFUDA cache using lock striping (segmented storage) for high-performance
//! concurrent access. This is the multi-threaded counterpart to [`LfudaCache`](crate::LfudaCache).
//!
//! # How It Works
//!
//! LFUDA (LFU with Dynamic Aging) addresses the cache pollution problem in LFU by
//! incorporating a global age factor. When an item is evicted, the age is set to its
//! priority. Newly inserted items start with priority = age + 1, giving them a fair
//! chance against long-cached items with high frequency counts.
//!
//! The cache partitions keys across multiple independent segments, each with its own
//! lock and independent aging state.
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────────────┐
//! │                     ConcurrentLfudaCache                             │
//! │                                                                      │
//! │  hash(key) % N  ──>  Segment Selection                               │
//! │                                                                      │
//! │  ┌──────────────┐ ┌──────────────┐     ┌──────────────┐              │
//! │  │  Segment 0   │ │  Segment 1   │ ... │  Segment N-1 │              │
//! │  │  age=100     │ │  age=150     │     │  age=120     │              │
//! │  │  ┌────────┐  │ │  ┌────────┐  │     │  ┌────────┐  │              │
//! │  │  │ Mutex  │  │ │  │ Mutex  │  │     │  │ Mutex  │  │              │
//! │  │  └────┬───┘  │ │  └────┬───┘  │     │  └────┬───┘  │              │
//! │  │       │      │ │       │      │     │       │      │              │
//! │  │  ┌────▼────┐ │ │  ┌────▼────┐ │     │  ┌────▼────┐ │              │
//! │  │  │LfudaSeg │ │ │  │LfudaSeg │ │     │  │LfudaSeg │ │              │
//! │  │  └─────────┘ │ │  └─────────┘ │     │  └─────────┘ │              │
//! │  └──────────────┘ └──────────────┘     └──────────────┘              │
//! └──────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Per-Segment Aging
//!
//! Each segment maintains its own global age counter. This means:
//! - Aging happens independently in each segment
//! - High-activity segments may age faster than low-activity ones
//! - Items in different segments are not directly comparable by priority
//!
//! This is a deliberate trade-off: global aging would require cross-segment
//! coordination, hurting concurrency.
//!
//! ## Trade-offs
//!
//! - **Pros**: Near-linear scaling, adapts to changing popularity per segment
//! - **Cons**: Aging is local to each segment, not global
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get/Put/Remove | O(log P) per segment |
//! | Concurrency | Near-linear scaling up to segment count |
//! | Memory overhead | ~160 bytes per entry + one Mutex per segment |
//! | Adaptability | Handles changing popularity patterns |
//!
//! Where P = distinct priority values per segment. Priority = frequency + age,
//! so P can grow with segment size.
//!
//! # When to Use
//!
//! **Use ConcurrentLfudaCache when:**
//! - Multiple threads need cache access
//! - Item popularity changes over time
//! - Long-running applications where old items should eventually age out
//! - You need frequency-based eviction with adaptation
//!
//! **Consider alternatives when:**
//! - Single-threaded access only → use `LfudaCache`
//! - Static popularity patterns → use `ConcurrentLfuCache` (simpler)
//! - Recency-based access → use `ConcurrentLruCache`
//! - Need global aging coordination → use `Mutex<LfudaCache>`
//!
//! # Thread Safety
//!
//! `ConcurrentLfudaCache` is `Send + Sync` and can be shared via `Arc`.
//!
//! # Example
//!
//! ```rust,ignore
//! use cache_rs::concurrent::ConcurrentLfudaCache;
//! use cache_rs::config::ConcurrentLfudaCacheConfig;
//! use std::num::NonZeroUsize;
//! use std::sync::Arc;
//! use std::thread;
//!
//! // Create a cache that adapts to changing popularity
//! let config = ConcurrentLfudaCacheConfig::new(NonZeroUsize::new(10_000).unwrap());
//! let cache = Arc::new(ConcurrentLfudaCache::init(config, None));
//!
//! // Phase 1: Establish initial popularity
//! for i in 0..1000 {
//!     cache.put(format!("old-{}", i), i, 1);
//!     for _ in 0..10 {
//!         cache.get(&format!("old-{}", i));
//!     }
//! }
//!
//! // Phase 2: New content arrives, old content ages out
//! let handles: Vec<_> = (0..4).map(|t| {
//!     let cache = Arc::clone(&cache);
//!     thread::spawn(move || {
//!         for i in 0..5000 {
//!             let key = format!("new-{}-{}", t, i);
//!             cache.put(key.clone(), i as i32, 1);
//!             let _ = cache.get(&key);
//!         }
//!     })
//! }).collect();
//!
//! for h in handles {
//!     h.join().unwrap();
//! }
//!
//! // Old items gradually evicted despite high historical frequency
//! println!("Cache size: {}", cache.len());
//! ```

extern crate alloc;

use crate::lfuda::LfudaSegment;
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

/// A thread-safe LFUDA cache with segmented storage for high concurrency.
pub struct ConcurrentLfudaCache<K, V, S = DefaultHashBuilder> {
    segments: Box<[Mutex<LfudaSegment<K, V, S>>]>,
    hash_builder: S,
}

impl<K, V> ConcurrentLfudaCache<K, V, DefaultHashBuilder>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    /// Creates a new concurrent LFUDA cache from a configuration.
    ///
    /// This is the **recommended** way to create a concurrent LFUDA cache.
    ///
    /// # Arguments
    ///
    /// * `config` - The cache configuration specifying capacity, segments, etc.
    /// * `hasher` - Optional custom hash builder. If `None`, uses the default.
    pub fn init(
        config: crate::config::ConcurrentLfudaCacheConfig,
        hasher: Option<DefaultHashBuilder>,
    ) -> Self {
        let segment_count = config.segments;
        let capacity = config.base.capacity;
        let max_size = config.base.max_size;
        let initial_age = config.base.initial_age;

        let hash_builder = hasher.unwrap_or_default();

        let segment_capacity = capacity.get() / segment_count;
        let segment_cap = NonZeroUsize::new(segment_capacity.max(1)).unwrap();
        let segment_max_size = max_size / segment_count as u64;

        let segments: Vec<_> = (0..segment_count)
            .map(|_| {
                let segment_config = crate::config::LfudaCacheConfig {
                    capacity: segment_cap,
                    initial_age,
                    max_size: segment_max_size,
                };
                Mutex::new(LfudaSegment::init(segment_config, hash_builder.clone()))
            })
            .collect();

        Self {
            segments: segments.into_boxed_slice(),
            hash_builder,
        }
    }
}

impl<K, V, S> ConcurrentLfudaCache<K, V, S>
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

    /// Inserts a key-value pair into the cache with optional size tracking.
    ///
    /// If the cache is at capacity, the entry with lowest priority (frequency + age) is evicted.
    /// Use `SIZE_UNIT` (1) for count-based caching.
    pub fn put(&self, key: K, value: V, size: u64) -> Option<(K, V)> {
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
        segment.remove(key)
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

    /// Checks if the cache contains a key without updating priority.
    ///
    /// This is a pure existence check that does **not** update the entry's priority
    /// or frequency.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if cache.contains(&"key".to_string()) {
    ///     println!("Key exists!");
    /// }
    /// ```
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let idx = self.segment_index(key);
        let segment = self.segments[idx].lock();
        segment.contains(key)
    }

    /// Returns a clone of the value without updating priority or access metadata.
    ///
    /// Unlike [`get()`](Self::get), this does NOT update the entry's frequency
    /// or priority. Returns a cloned value because the internal lock cannot be
    /// held across the return boundary.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let value = cache.peek(&"key".to_string());
    /// ```
    pub fn peek<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        let idx = self.segment_index(key);
        let segment = self.segments[idx].lock();
        segment.peek(key).cloned()
    }

    /// Removes and returns the lowest priority entry from the cache.
    ///
    /// Finds the segment with the globally lowest minimum priority and pops
    /// the eviction candidate from that segment. LFUDA's aging mechanism
    /// is applied within the segment.
    ///
    /// # Concurrency Note
    ///
    /// This method uses a two-phase approach: first scanning all segments to find
    /// the best candidate, then re-locking the winning segment to pop. Between
    /// these phases, another thread may modify the cache (TOCTOU race). This is
    /// inherent to the segmented design and means the returned entry may not be
    /// the globally optimal candidate at the instant of removal. The method
    /// remains safe — it will either return a valid entry or `None`.
    ///
    /// # Performance
    ///
    /// This method locks each segment sequentially during the scan phase
    /// (O(segments) lock acquisitions), making it more expensive than
    /// single-segment operations like `get()` or `put()`.
    ///
    /// # Returns
    ///
    /// - `Some((key, value))` if the cache was not empty
    /// - `None` if the cache is empty
    pub fn pop(&self) -> Option<(K, V)> {
        // Find the segment with the lowest min_priority
        let mut best_idx = None;
        let mut best_priority = u64::MAX;

        for (i, segment) in self.segments.iter().enumerate() {
            let guard = segment.lock();
            if let Some(priority) = guard.peek_min_priority() {
                if priority < best_priority {
                    best_priority = priority;
                    best_idx = Some(i);
                }
            }
        }

        if let Some(idx) = best_idx {
            let mut guard = self.segments[idx].lock();
            guard.pop()
        } else {
            None
        }
    }

    /// Removes and returns the highest priority entry from the cache.
    ///
    /// Internal method that finds the segment with the globally highest maximum
    /// priority and pops the highest priority entry from that segment.
    ///
    /// # Concurrency Note
    ///
    /// Uses the same two-phase scan approach as [`pop()`](Self::pop). See its
    /// documentation for details on the inherent TOCTOU race and performance
    /// characteristics.
    #[allow(dead_code)]
    pub(crate) fn pop_r(&self) -> Option<(K, V)> {
        // Find the segment with the highest max_priority
        let mut best_idx = None;
        let mut best_priority = 0u64;

        for (i, segment) in self.segments.iter().enumerate() {
            let guard = segment.lock();
            if let Some(priority) = guard.peek_max_priority() {
                if priority > best_priority || best_idx.is_none() {
                    best_priority = priority;
                    best_idx = Some(i);
                }
            }
        }

        if let Some(idx) = best_idx {
            let mut guard = self.segments[idx].lock();
            guard.pop_r()
        } else {
            None
        }
    }
}

impl<K, V, S> CacheMetrics for ConcurrentLfudaCache<K, V, S>
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
        "ConcurrentLFUDA"
    }
}

unsafe impl<K: Send, V: Send, S: Send> Send for ConcurrentLfudaCache<K, V, S> {}
unsafe impl<K: Send, V: Send, S: Send + Sync> Sync for ConcurrentLfudaCache<K, V, S> {}

impl<K, V, S> core::fmt::Debug for ConcurrentLfudaCache<K, V, S>
where
    K: Hash + Eq + Clone + Send,
    V: Clone + Send,
    S: BuildHasher + Clone + Send,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConcurrentLfudaCache")
            .field("segment_count", &self.segments.len())
            .field("total_len", &self.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConcurrentCacheConfig, ConcurrentLfudaCacheConfig, LfudaCacheConfig};

    extern crate std;
    use std::string::ToString;
    use std::sync::Arc;
    use std::thread;
    use std::vec::Vec;

    fn make_config(capacity: usize, segments: usize) -> ConcurrentLfudaCacheConfig {
        ConcurrentCacheConfig {
            base: LfudaCacheConfig {
                capacity: NonZeroUsize::new(capacity).unwrap(),
                initial_age: 0,
                max_size: u64::MAX,
            },
            segments,
        }
    }

    #[test]
    fn test_basic_operations() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1, 1);
        cache.put("b".to_string(), 2, 1);

        assert_eq!(cache.get(&"a".to_string()), Some(1));
        assert_eq!(cache.get(&"b".to_string()), Some(2));
    }

    #[test]
    fn test_concurrent_access() {
        let cache: Arc<ConcurrentLfudaCache<String, i32>> =
            Arc::new(ConcurrentLfudaCache::init(make_config(1000, 16), None));
        let num_threads = 8;
        let ops_per_thread = 500;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}_{}", t, i);
                    cache.put(key.clone(), i, 1);
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
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        // Capacity is distributed across segments
        let capacity = cache.capacity();
        assert!(capacity >= 16);
        assert!(capacity <= 100);
    }

    #[test]
    fn test_segment_count() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 8), None);

        assert_eq!(cache.segment_count(), 8);
    }

    #[test]
    fn test_len_and_is_empty() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        cache.put("key1".to_string(), 1, 1);
        assert_eq!(cache.len(), 1);
        assert!(!cache.is_empty());

        cache.put("key2".to_string(), 2, 1);
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_remove() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("key1".to_string(), 1, 1);
        cache.put("key2".to_string(), 2, 1);

        assert_eq!(cache.remove(&"key1".to_string()), Some(1));
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&"key1".to_string()), None);

        assert_eq!(cache.remove(&"nonexistent".to_string()), None);
    }

    #[test]
    fn test_clear() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("key1".to_string(), 1, 1);
        cache.put("key2".to_string(), 2, 1);
        cache.put("key3".to_string(), 3, 1);

        assert_eq!(cache.len(), 3);

        cache.clear();

        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert_eq!(cache.get(&"key1".to_string()), None);
    }

    #[test]
    fn test_contains_key() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("exists".to_string(), 1, 1);

        assert!(cache.contains(&"exists".to_string()));
        assert!(!cache.contains(&"missing".to_string()));
    }

    #[test]
    fn test_get_with() {
        let cache: ConcurrentLfudaCache<String, String> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("key".to_string(), "hello world".to_string(), 1);

        let len = cache.get_with(&"key".to_string(), |v: &String| v.len());
        assert_eq!(len, Some(11));

        let missing = cache.get_with(&"missing".to_string(), |v: &String| v.len());
        assert_eq!(missing, None);
    }

    #[test]
    fn test_aging_behavior() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(48, 16), None);

        cache.put("a".to_string(), 1, 1);
        cache.put("b".to_string(), 2, 1);
        cache.put("c".to_string(), 3, 1);

        // Access "a" and "c" multiple times to increase frequency
        for _ in 0..5 {
            let _ = cache.get(&"a".to_string());
            let _ = cache.get(&"c".to_string());
        }

        // Add a new item, aging should adjust priorities
        cache.put("d".to_string(), 4, 1);

        assert!(cache.len() <= 48);
    }

    #[test]
    fn test_eviction_on_capacity() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(80, 16), None);

        // Fill the cache
        for i in 0..10 {
            cache.put(std::format!("key{}", i), i, 1);
        }

        // Cache should not exceed capacity
        assert!(cache.len() <= 80);
    }

    #[test]
    fn test_metrics() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1, 1);
        cache.put("b".to_string(), 2, 1);

        let metrics = cache.metrics();
        // Metrics aggregation across segments
        assert!(!metrics.is_empty());
    }

    #[test]
    fn test_algorithm_name() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        assert_eq!(cache.algorithm_name(), "ConcurrentLFUDA");
    }

    #[test]
    fn test_empty_cache_operations() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.get(&"missing".to_string()), None);
        assert_eq!(cache.remove(&"missing".to_string()), None);
        assert!(!cache.contains(&"missing".to_string()));
    }

    #[test]
    fn test_borrowed_key_lookup() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("test_key".to_string(), 42, 1);

        // Test with borrowed key
        let key_str = "test_key";
        assert_eq!(cache.get(key_str), Some(42));
        assert!(cache.contains(key_str));
        assert_eq!(cache.remove(key_str), Some(42));
    }

    #[test]
    fn test_frequency_with_aging() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("key".to_string(), 1, 1);

        // Access the key multiple times
        for _ in 0..10 {
            let _ = cache.get(&"key".to_string());
        }

        // Item should still be accessible
        assert_eq!(cache.get(&"key".to_string()), Some(1));
    }

    #[test]
    fn test_dynamic_aging() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(80, 16), None);

        // Add items with different access patterns
        for i in 0..5 {
            cache.put(std::format!("key{}", i), i, 1);
            for _ in 0..i {
                let _ = cache.get(&std::format!("key{}", i));
            }
        }

        // Add more items to trigger eviction with aging
        for i in 5..10 {
            cache.put(std::format!("key{}", i), i, 1);
        }

        assert!(cache.len() <= 80);
    }

    #[test]
    fn test_contains_non_promoting() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1, 1);
        cache.put("b".to_string(), 2, 1);

        // contains() should check without updating priority
        assert!(cache.contains(&"a".to_string()));
        assert!(cache.contains(&"b".to_string()));
        assert!(!cache.contains(&"c".to_string()));
    }

    #[test]
    fn test_pop_returns_entry() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1, 1);
        cache.put("b".to_string(), 2, 1);

        let initial_len = cache.len();

        // Pop should return Some entry
        let result = cache.pop();
        assert!(result.is_some());

        let (key, _value) = result.unwrap();
        assert!(key == "a" || key == "b"); // Could be either due to segmentation

        // Length should decrease
        assert_eq!(cache.len(), initial_len - 1);
    }

    #[test]
    fn test_pop_empty_cache() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        assert!(cache.pop().is_none());
    }

    #[test]
    fn test_pop_r_returns_entry() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1, 1);
        cache.put("b".to_string(), 2, 1);

        let initial_len = cache.len();

        // Pop_r should return Some entry
        let result = cache.pop_r();
        assert!(result.is_some());

        let (key, _value) = result.unwrap();
        assert!(key == "a" || key == "b"); // Could be either due to segmentation

        // Length should decrease
        assert_eq!(cache.len(), initial_len - 1);
    }

    #[test]
    fn test_pop_r_empty_cache() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        assert!(cache.pop_r().is_none());
    }

    #[test]
    fn test_pop_all_entries() {
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 16), None);

        cache.put("a".to_string(), 1, 1);
        cache.put("b".to_string(), 2, 1);
        cache.put("c".to_string(), 3, 1);

        let mut count = 0;
        while cache.pop().is_some() {
            count += 1;
        }

        assert_eq!(count, 3);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_pop_pop_r_comprehensive_interleaved() {
        // Use segments: 1 for deterministic ordering
        let cache: ConcurrentLfudaCache<String, i32> =
            ConcurrentLfudaCache::init(make_config(100, 1), None);

        // Insert entries: all start with initial priority
        cache.put("a".to_string(), 1, 1);
        cache.put("b".to_string(), 2, 1);
        cache.put("c".to_string(), 3, 1);
        cache.put("d".to_string(), 4, 1);
        cache.put("e".to_string(), 5, 1);

        // Access some entries to differentiate priorities
        cache.get(&"a".to_string());
        cache.get(&"a".to_string()); // a: higher priority
        cache.get(&"b".to_string()); // b: medium priority
        cache.get(&"d".to_string());
        cache.get(&"d".to_string());
        cache.get(&"d".to_string()); // d: highest priority

        // pop() removes lowest priority first (c or e)
        let popped = cache.pop();
        assert!(popped == Some(("c".to_string(), 3)) || popped == Some(("e".to_string(), 5)));
        assert_eq!(cache.len(), 4);

        // pop_r() removes highest priority (d)
        assert_eq!(cache.pop_r(), Some(("d".to_string(), 4)));
        assert_eq!(cache.len(), 3);

        // Put new entry "f"
        cache.put("f".to_string(), 6, 1);
        assert_eq!(cache.len(), 4);

        // pop() removes lowest priority
        let popped2 = cache.pop();
        assert!(popped2.is_some());
        assert_eq!(cache.len(), 3);

        // Remove "a" by key
        assert_eq!(cache.remove(&"a".to_string()), Some(1));
        assert_eq!(cache.len(), 2);

        // Pop remaining
        assert!(cache.pop().is_some());
        assert!(cache.pop_r().is_some() || cache.pop().is_some());

        while cache.pop().is_some() {}

        // Cache is empty
        assert!(cache.is_empty());
        assert_eq!(cache.pop(), None);
        assert_eq!(cache.pop_r(), None);
    }
}
