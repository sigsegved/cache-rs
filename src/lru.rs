//! Least Recently Used (LRU) Cache Implementation
//!
//! An LRU cache evicts the least recently accessed item when capacity is reached.
//! This implementation provides O(1) time complexity for all operations using a
//! hash map combined with a doubly-linked list.
//!
//! # How the Algorithm Works
//!
//! The LRU algorithm is based on the principle of **temporal locality**: items accessed
//! recently are likely to be accessed again soon. The cache maintains items ordered by
//! their last access time.
//!
//! ## Data Structure
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                        LRU Cache                                │
//! │                                                                 │
//! │  HashMap<K, *Node>          Doubly-Linked List                  │
//! │  ┌──────────────┐          ┌──────────────────────────────┐    │
//! │  │ "apple" ──────────────▶ │ MRU ◀──▶ ... ◀──▶ LRU       │    │
//! │  │ "banana" ─────────────▶ │  ▲                    │      │    │
//! │  │ "cherry" ─────────────▶ │  │                    ▼      │    │
//! │  └──────────────┘          │ head              tail       │    │
//! │                            └──────────────────────────────┘    │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! - **HashMap**: Provides O(1) key lookup, storing pointers to list nodes
//! - **Doubly-Linked List**: Maintains access order (most recent at head, least recent at tail)
//!
//! ## Operations
//!
//! | Operation | Action | Time |
//! |-----------|--------|------|
//! | `get(key)` | Move accessed node to head (MRU position) | O(1) |
//! | `put(key, value)` | Insert at head, evict from tail if full | O(1) |
//! | `remove(key)` | Unlink node from list, remove from map | O(1) |
//!
//! ## Eviction Example
//!
//! ```text
//! Cache capacity: 3
//!
//! put("a", 1)  →  [a]
//! put("b", 2)  →  [b, a]
//! put("c", 3)  →  [c, b, a]
//! get("a")     →  [a, c, b]       // "a" moved to front (MRU)
//! put("d", 4)  →  [d, a, c]       // "b" evicted (was LRU)
//! ```
//!
//! # Dual-Limit Capacity
//!
//! This implementation supports two independent limits:
//!
//! - **`max_entries`**: Maximum number of items (bounds cache memory overhead)
//! - **`max_size`**: Maximum total size of content (sum of item sizes)
//!
//! Eviction occurs when **either** limit would be exceeded.
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get | O(1) |
//! | Put | O(1) |
//! | Remove | O(1) |
//! | Memory per entry | ~80 bytes overhead + key×2 + value |
//!
//! Memory overhead breakdown (64-bit): list node pointers (16B), `CacheEntry` metadata (24B),
//! HashMap bucket (~24B), allocator overhead (~16B). Key is stored twice (in entry and HashMap).
//!
//! # When to Use LRU
//!
//! **Good for:**
//! - General-purpose caching with temporal locality
//! - Web page caches, database query caches
//! - Any workload where recent items are accessed again soon
//!
//! **Not ideal for:**
//! - Frequency-based access patterns (use LFU instead)
//! - Scan-resistant workloads (use SLRU instead)
//! - Size-aware caching of variable-sized objects (use GDSF instead)
//!
//! # Thread Safety
//!
//! `LruCache` is **not thread-safe**. For concurrent access, either:
//! - Wrap with `Mutex` or `RwLock`
//! - Use `ConcurrentLruCache` (requires `concurrent` feature)
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```
//! use cache_rs::LruCache;
//! use cache_rs::config::LruCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = LruCacheConfig {
//!     capacity: NonZeroUsize::new(3).unwrap(),
//!     max_size: u64::MAX,
//! };
//! let mut cache = LruCache::init(config, None);
//!
//! cache.put("a", 1);
//! cache.put("b", 2);
//! cache.put("c", 3);
//!
//! assert_eq!(cache.get(&"a"), Some(&1));  // "a" is now MRU
//!
//! cache.put("d", 4);  // Evicts "b" (LRU)
//! assert_eq!(cache.get(&"b"), None);
//! ```
//!
//! ## Size-Aware Caching
//!
//! ```
//! use cache_rs::LruCache;
//! use cache_rs::config::LruCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! // Cache with max 1000 entries and 10MB total size
//! let config = LruCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     max_size: 10 * 1024 * 1024,
//! };
//! let mut cache: LruCache<String, Vec<u8>> = LruCache::init(config, None);
//!
//! let data = vec![0u8; 1024];  // 1KB
//! cache.put_with_size("file.bin".to_string(), data.clone(), 1024);
//! ```

extern crate alloc;

use crate::config::LruCacheConfig;
use crate::entry::CacheEntry;
use crate::list::{List, ListEntry};
use crate::metrics::{CacheMetrics, LruCacheMetrics};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash};
use core::mem;
use core::num::NonZeroUsize;

#[cfg(feature = "hashbrown")]
use hashbrown::DefaultHashBuilder;
#[cfg(feature = "hashbrown")]
use hashbrown::HashMap;

#[cfg(not(feature = "hashbrown"))]
use std::collections::hash_map::RandomState as DefaultHashBuilder;
#[cfg(not(feature = "hashbrown"))]
use std::collections::HashMap;

/// Internal LRU segment containing the actual cache algorithm.
///
/// This is shared between `LruCache` (single-threaded) and
/// `ConcurrentLruCache` (multi-threaded). All algorithm logic is
/// implemented here to avoid code duplication.
///
/// Uses `CacheEntry<K, V>` for unified entry management with built-in
/// size tracking and timestamps. LRU doesn't need per-entry metadata
/// since position in the list implicitly tracks recency.
///
/// # Safety
///
/// This struct contains raw pointers in the `map` field.
/// These pointers are always valid as long as:
/// - The pointer was obtained from a `list` entry's `add()` call
/// - The node has not been removed from the list
/// - The segment has not been dropped
pub(crate) struct LruSegment<K, V, S = DefaultHashBuilder> {
    /// Configuration for the LRU cache (includes capacity and max_size)
    config: LruCacheConfig,
    list: List<CacheEntry<K, V>>,
    map: HashMap<K, *mut ListEntry<CacheEntry<K, V>>, S>,
    metrics: LruCacheMetrics,
    /// Current total size of cached content (sum of entry.size values)
    current_size: u64,
}

// SAFETY: LruSegment owns all data and raw pointers point only to nodes owned by `list`.
// Concurrent access is safe when wrapped in proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for LruSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for LruSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LruSegment<K, V, S> {
    /// Creates a new LRU segment from a configuration.
    ///
    /// This is the **recommended** way to create an LRU segment. All configuration
    /// is specified through the [`LruCacheConfig`] struct.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity and optional size limit
    /// * `hasher` - Hash builder for the internal HashMap
    #[allow(dead_code)] // Used by concurrent module when feature is enabled
    pub(crate) fn init(config: LruCacheConfig, hasher: S) -> Self {
        let map_capacity = config.capacity.get().next_power_of_two();
        LruSegment {
            config,
            list: List::new(config.capacity),
            map: HashMap::with_capacity_and_hasher(map_capacity, hasher),
            metrics: LruCacheMetrics::new(config.max_size),
            current_size: 0,
        }
    }

    #[inline]
    pub(crate) fn cap(&self) -> NonZeroUsize {
        self.config.capacity
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the current total size of cached content.
    #[inline]
    pub(crate) fn current_size(&self) -> u64 {
        self.current_size
    }

    /// Returns the maximum content size the cache can hold.
    #[inline]
    pub(crate) fn max_size(&self) -> u64 {
        self.config.max_size
    }

    #[inline]
    pub(crate) fn metrics(&self) -> &LruCacheMetrics {
        &self.metrics
    }

    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(node) = self.map.get(key).copied() {
            unsafe {
                // SAFETY: node comes from our map
                self.list.move_to_front(node);
                let entry = (*node).get_value_mut();
                entry.touch(); // Update last_accessed timestamp
                self.metrics.core.record_hit(entry.size);
                Some(&entry.value)
            }
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn record_miss(&mut self, object_size: u64) {
        self.metrics.core.record_miss(object_size);
    }

    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let node = self.map.get(key).copied()?;
        unsafe {
            // SAFETY: node comes from our map
            self.list.move_to_front(node);
            let entry = (*node).get_value_mut();
            entry.touch(); // Update last_accessed timestamp
            self.metrics.core.record_hit(entry.size);
            Some(&mut entry.value)
        }
    }

    pub(crate) fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        K: Clone + Hash + Eq,
    {
        // Use estimated size for backward compatibility
        let object_size = self.estimate_object_size(&key, &value);
        self.put_with_size(key, value, object_size)
    }

    /// Insert a key-value pair with explicit size tracking.
    ///
    /// The `size` parameter specifies how much of `max_size` this entry consumes.
    /// Use `size=1` for count-based caches.
    pub(crate) fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>
    where
        K: Clone + Hash + Eq,
    {
        let mut evicted = None;

        if let Some(&node) = self.map.get(&key) {
            unsafe {
                // SAFETY: node comes from our map
                self.list.move_to_front(node);
                let entry = (*node).get_value_mut();

                // Update size tracking: remove old size, add new size
                let old_size = entry.size;
                self.current_size = self.current_size.saturating_sub(old_size);
                self.metrics.core.cache_size_bytes =
                    self.metrics.core.cache_size_bytes.saturating_sub(old_size);

                // Update entry fields
                let old_key = core::mem::replace(&mut entry.key, key);
                let old_value = core::mem::replace(&mut entry.value, value);
                entry.size = size;
                entry.touch();

                self.current_size += size;
                self.metrics.core.cache_size_bytes += size;

                return Some((old_key, old_value));
            }
        }

        // Evict while entry count limit OR size limit would be exceeded
        while self.map.len() >= self.cap().get()
            || (self.current_size + size > self.config.max_size && !self.map.is_empty())
        {
            if let Some(old_entry) = self.list.remove_last() {
                unsafe {
                    let entry_ptr = Box::into_raw(old_entry);
                    let cache_entry = &(*entry_ptr).get_value();
                    self.map.remove(&cache_entry.key);
                    let evicted_key = cache_entry.key.clone();
                    let evicted_value = cache_entry.value.clone();
                    let evicted_size = cache_entry.size;
                    self.current_size = self.current_size.saturating_sub(evicted_size);
                    self.metrics.core.record_eviction(evicted_size);
                    evicted = Some((evicted_key, evicted_value));
                    let _ = Box::from_raw(entry_ptr);
                }
            } else {
                break; // No more items to evict
            }
        }

        // Create new CacheEntry and add to list
        let cache_entry = CacheEntry::new(key.clone(), value, size);
        if let Some(node) = self.list.add(cache_entry) {
            self.map.insert(key, node);
            self.current_size += size;
            self.metrics.core.record_insertion(size);
        }

        evicted
    }

    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        let node = self.map.remove(key)?;
        unsafe {
            // SAFETY: node comes from our map
            let entry = (*node).get_value();
            let object_size = entry.size;
            let value = entry.value.clone();
            self.list.remove(node);
            self.current_size = self.current_size.saturating_sub(object_size);
            self.metrics.core.record_eviction(object_size);
            Some(value)
        }
    }

    pub(crate) fn clear(&mut self) {
        self.current_size = 0;
        self.metrics.core.cache_size_bytes = 0;
        self.map.clear();
        self.list.clear();
    }
}

impl<K, V, S> core::fmt::Debug for LruSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LruSegment")
            .field("capacity", &self.config.capacity)
            .field("len", &self.map.len())
            .finish()
    }
}

/// A Least Recently Used (LRU) cache with O(1) operations.
///
/// Maintains items in order of access recency. When capacity is reached,
/// the least recently accessed item is evicted to make room for new entries.
///
/// # Type Parameters
///
/// - `K`: Key type. Must implement `Hash + Eq`. For mutation operations, also needs `Clone`.
/// - `V`: Value type. Must implement `Clone` for retrieval operations.
/// - `S`: Hash builder type. Defaults to `DefaultHashBuilder`.
///
/// # Capacity Modes
///
/// - **Count-based**: `LruCache::new(cap)` - limits number of entries
/// - **Size-based**: `LruCache::init(config, None)` with `max_size` set - limits total content size
/// - **Dual-limit**: `LruCache::with_limits(cap, bytes)` - limits both
///
/// # Example
///
/// ```
/// use cache_rs::LruCache;
/// use cache_rs::config::LruCacheConfig;
/// use core::num::NonZeroUsize;
///
/// let config = LruCacheConfig {
///     capacity: NonZeroUsize::new(2).unwrap(),
///     max_size: u64::MAX,
/// };
/// let mut cache = LruCache::init(config, None);
///
/// cache.put("apple", 1);
/// cache.put("banana", 2);
/// assert_eq!(cache.get(&"apple"), Some(&1));
///
/// // "banana" is now LRU, so it gets evicted
/// cache.put("cherry", 3);
/// assert_eq!(cache.get(&"banana"), None);
/// ```
#[derive(Debug)]
pub struct LruCache<K, V, S = DefaultHashBuilder> {
    segment: LruSegment<K, V, S>,
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LruCache<K, V, S> {
    /// Returns the maximum number of entries the cache can hold.
    #[inline]
    pub fn cap(&self) -> NonZeroUsize {
        self.segment.cap()
    }

    /// Returns the current number of entries in the cache.
    #[inline]
    pub fn len(&self) -> usize {
        self.segment.len()
    }

    /// Returns `true` if the cache contains no entries.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.segment.is_empty()
    }

    /// Returns the current total size of all cached content.
    ///
    /// This is the sum of all `size` values passed to `put_with_size()`,
    /// or estimated sizes for entries added via `put()`.
    #[inline]
    pub fn current_size(&self) -> u64 {
        self.segment.current_size()
    }

    /// Returns the maximum total content size the cache can hold.
    ///
    /// Returns `u64::MAX` if no size limit was configured.
    #[inline]
    pub fn max_size(&self) -> u64 {
        self.segment.max_size()
    }

    /// Retrieves a reference to the value for the given key.
    ///
    /// If the key exists, it is moved to the most-recently-used (MRU) position.
    /// Returns `None` if the key is not present.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LruCache;
    /// use cache_rs::config::LruCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LruCacheConfig {
    ///     capacity: NonZeroUsize::new(10).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LruCache::init(config, None);
    /// cache.put("key", 42);
    ///
    /// assert_eq!(cache.get(&"key"), Some(&42));
    /// assert_eq!(cache.get(&"missing"), None);
    /// ```
    #[inline]
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get(key)
    }

    /// Records a cache miss for metrics tracking.
    ///
    /// Call this when you look up a key, find it missing, and fetch from
    /// the underlying data source. This updates the miss counter in metrics.
    ///
    /// # Arguments
    ///
    /// * `object_size` - Size of the object that was fetched (for byte tracking)
    #[inline]
    pub fn record_miss(&mut self, object_size: u64) {
        self.segment.record_miss(object_size);
    }

    /// Retrieves a mutable reference to the value for the given key.
    ///
    /// If the key exists, it is moved to the MRU position.
    /// Returns `None` if the key is not present.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LruCache;
    /// use cache_rs::config::LruCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LruCacheConfig {
    ///     capacity: NonZeroUsize::new(10).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LruCache::init(config, None);
    /// cache.put("counter", 0);
    ///
    /// if let Some(val) = cache.get_mut(&"counter") {
    ///     *val += 1;
    /// }
    /// assert_eq!(cache.get(&"counter"), Some(&1));
    /// ```
    #[inline]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get_mut(key)
    }
}

impl<K: Hash + Eq + Clone, V: Clone, S: BuildHasher> LruCache<K, V, S> {
    /// Inserts a key-value pair into the cache.
    ///
    /// If the key already exists, the value is updated and the entry moves
    /// to the MRU position. The old key-value pair is returned.
    ///
    /// If the cache is at capacity, the least recently used entry is evicted
    /// and returned.
    ///
    /// # Returns
    ///
    /// - `Some((old_key, old_value))` if the key existed or an entry was evicted
    /// - `None` if this was a new insertion with available capacity
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LruCache;
    /// use cache_rs::config::LruCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LruCacheConfig {
    ///     capacity: NonZeroUsize::new(2).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LruCache::init(config, None);
    ///
    /// assert_eq!(cache.put("a", 1), None);           // New entry
    /// assert_eq!(cache.put("b", 2), None);           // New entry
    /// assert_eq!(cache.put("a", 10), Some(("a", 1))); // Update existing
    /// assert_eq!(cache.put("c", 3), Some(("b", 2))); // Evicts "b"
    /// ```
    #[inline]
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        self.segment.put(key, value)
    }

    /// Inserts a key-value pair with an explicit size.
    ///
    /// Use this for size-aware caching where you want to track the actual
    /// memory or storage footprint of cached items.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to insert
    /// * `value` - The value to cache
    /// * `size` - The size this entry consumes (in your chosen unit, e.g., bytes)
    ///
    /// # Returns
    ///
    /// Same as `put()` - returns evicted or replaced entry if any.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LruCache;
    /// use cache_rs::config::LruCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LruCacheConfig {
    ///     capacity: NonZeroUsize::new(100).unwrap(),
    ///     max_size: 1024 * 1024,  // 1MB max
    /// };
    /// let mut cache: LruCache<String, Vec<u8>> = LruCache::init(config, None);
    ///
    /// let data = vec![0u8; 1000];
    /// cache.put_with_size("file".to_string(), data, 1000);
    ///
    /// assert_eq!(cache.current_size(), 1000);
    /// ```
    #[inline]
    pub fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)> {
        self.segment.put_with_size(key, value, size)
    }

    /// Removes a key from the cache.
    ///
    /// Returns the value if the key was present, `None` otherwise.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LruCache;
    /// use cache_rs::config::LruCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LruCacheConfig {
    ///     capacity: NonZeroUsize::new(10).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LruCache::init(config, None);
    /// cache.put("key", 42);
    ///
    /// assert_eq!(cache.remove(&"key"), Some(42));
    /// assert_eq!(cache.remove(&"key"), None);  // Already removed
    /// ```
    #[inline]
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.remove(key)
    }

    /// Removes all entries from the cache.
    ///
    /// Resets `current_size` to 0 and clears all metrics counters.
    #[inline]
    pub fn clear(&mut self) {
        self.segment.clear()
    }

    /// Returns an iterator over the cache entries.
    ///
    /// # Panics
    ///
    /// Not yet implemented.
    pub fn iter(&self) -> Iter<'_, K, V> {
        unimplemented!("Iteration not yet implemented")
    }

    /// Returns a mutable iterator over the cache entries.
    ///
    /// # Panics
    ///
    /// Not yet implemented.
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        unimplemented!("Mutable iteration not yet implemented")
    }
}

impl<K: Hash + Eq, V> LruCache<K, V>
where
    V: Clone,
{
    /// Creates a new LRU cache from a configuration with an optional hasher.
    ///
    /// This is the **only** way to create an LRU cache.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity and optional size limit
    /// * `hasher` - Optional custom hash builder. If `None`, uses `DefaultHashBuilder`
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LruCache;
    /// use cache_rs::config::LruCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// // Simple capacity-only cache
    /// let config = LruCacheConfig {
    ///     capacity: NonZeroUsize::new(100).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache: LruCache<&str, i32> = LruCache::init(config, None);
    /// cache.put("key", 42);
    ///
    /// // Cache with size limit
    /// let config = LruCacheConfig {
    ///     capacity: NonZeroUsize::new(1000).unwrap(),
    ///     max_size: 10 * 1024 * 1024,  // 10MB
    /// };
    /// let cache: LruCache<String, Vec<u8>> = LruCache::init(config, None);
    /// ```
    pub fn init(
        config: LruCacheConfig,
        hasher: Option<DefaultHashBuilder>,
    ) -> LruCache<K, V, DefaultHashBuilder> {
        LruCache {
            segment: LruSegment::init(config, hasher.unwrap_or_default()),
        }
    }
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> CacheMetrics for LruCache<K, V, S> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.segment.metrics().metrics()
    }

    fn algorithm_name(&self) -> &'static str {
        self.segment.metrics().algorithm_name()
    }
}

pub struct Iter<'a, K, V> {
    _marker: core::marker::PhantomData<&'a (K, V)>,
}

pub struct IterMut<'a, K, V> {
    _marker: core::marker::PhantomData<&'a mut (K, V)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::LruCacheConfig;
    use alloc::string::String;

    /// Helper to create an LruCache with the given capacity
    fn make_cache<K: Hash + Eq + Clone, V: Clone>(cap: usize) -> LruCache<K, V> {
        let config = LruCacheConfig {
            capacity: NonZeroUsize::new(cap).unwrap(),
            max_size: u64::MAX,
        };
        LruCache::init(config, None)
    }

    #[test]
    fn test_lru_get_put() {
        let mut cache = make_cache(2);
        assert_eq!(cache.put("apple", 1), None);
        assert_eq!(cache.put("banana", 2), None);
        assert_eq!(cache.get(&"apple"), Some(&1));
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), None);
        assert_eq!(cache.put("apple", 3).unwrap().1, 1);
        assert_eq!(cache.get(&"apple"), Some(&3));
        assert_eq!(cache.put("cherry", 4).unwrap().1, 2);
        assert_eq!(cache.get(&"banana"), None);
        assert_eq!(cache.get(&"apple"), Some(&3));
        assert_eq!(cache.get(&"cherry"), Some(&4));
    }

    #[test]
    fn test_lru_get_mut() {
        let mut cache = make_cache(2);
        cache.put("apple", 1);
        cache.put("banana", 2);
        if let Some(v) = cache.get_mut(&"apple") {
            *v = 3;
        }
        assert_eq!(cache.get(&"apple"), Some(&3));
        cache.put("cherry", 4);
        assert_eq!(cache.get(&"banana"), None);
        assert_eq!(cache.get(&"apple"), Some(&3));
        assert_eq!(cache.get(&"cherry"), Some(&4));
    }

    #[test]
    fn test_lru_remove() {
        let mut cache = make_cache(2);
        cache.put("apple", 1);
        cache.put("banana", 2);
        assert_eq!(cache.get(&"apple"), Some(&1));
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), None);
        assert_eq!(cache.remove(&"apple"), Some(1));
        assert_eq!(cache.get(&"apple"), None);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.remove(&"cherry"), None);
        let evicted = cache.put("cherry", 3);
        assert_eq!(evicted, None);
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), Some(&3));
    }

    #[test]
    fn test_lru_clear() {
        let mut cache = make_cache(2);
        cache.put("apple", 1);
        cache.put("banana", 2);
        assert_eq!(cache.len(), 2);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        cache.put("cherry", 3);
        assert_eq!(cache.get(&"cherry"), Some(&3));
    }

    #[test]
    fn test_lru_capacity_limits() {
        let mut cache = make_cache(2);
        cache.put("apple", 1);
        cache.put("banana", 2);
        cache.put("cherry", 3);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&"apple"), None);
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), Some(&3));
    }

    #[test]
    fn test_lru_string_keys() {
        let mut cache = make_cache(2);
        let key1 = String::from("apple");
        let key2 = String::from("banana");
        cache.put(key1.clone(), 1);
        cache.put(key2.clone(), 2);
        assert_eq!(cache.get(&key1), Some(&1));
        assert_eq!(cache.get(&key2), Some(&2));
        assert_eq!(cache.get("apple"), Some(&1));
        assert_eq!(cache.get("banana"), Some(&2));
        drop(cache);
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    struct ComplexValue {
        val: i32,
        description: String,
    }

    #[test]
    fn test_lru_complex_values() {
        let mut cache = make_cache(2);
        let key1 = String::from("apple");
        let key2 = String::from("banana");
        let fruit1 = ComplexValue {
            val: 1,
            description: String::from("First fruit"),
        };
        let fruit2 = ComplexValue {
            val: 2,
            description: String::from("Second fruit"),
        };
        let fruit3 = ComplexValue {
            val: 3,
            description: String::from("Third fruit"),
        };
        cache.put(key1.clone(), fruit1.clone());
        cache.put(key2.clone(), fruit2.clone());
        assert_eq!(cache.get(&key1).unwrap().val, fruit1.val);
        assert_eq!(cache.get(&key2).unwrap().val, fruit2.val);
        let evicted = cache.put(String::from("cherry"), fruit3.clone());
        let evicted_fruit = evicted.unwrap();
        assert_eq!(evicted_fruit.1, fruit1);
        let removed = cache.remove(&key1);
        assert_eq!(removed, None);
    }

    #[test]
    fn test_lru_metrics() {
        use crate::metrics::CacheMetrics;
        let mut cache = make_cache(2);
        let metrics = cache.metrics();
        assert_eq!(metrics.get("requests").unwrap(), &0.0);
        assert_eq!(metrics.get("cache_hits").unwrap(), &0.0);
        assert_eq!(metrics.get("cache_misses").unwrap(), &0.0);
        cache.put("apple", 1);
        cache.put("banana", 2);
        cache.get(&"apple");
        cache.get(&"banana");
        let metrics = cache.metrics();
        assert_eq!(metrics.get("cache_hits").unwrap(), &2.0);
        cache.record_miss(64);
        let metrics = cache.metrics();
        assert_eq!(metrics.get("cache_misses").unwrap(), &1.0);
        assert_eq!(metrics.get("requests").unwrap(), &3.0);
        cache.put("cherry", 3);
        let metrics = cache.metrics();
        assert_eq!(metrics.get("evictions").unwrap(), &1.0);
        assert!(metrics.get("bytes_written_to_cache").unwrap() > &0.0);
        assert_eq!(cache.algorithm_name(), "LRU");
    }

    #[test]
    fn test_lru_segment_directly() {
        let config = LruCacheConfig {
            capacity: NonZeroUsize::new(2).unwrap(),
            max_size: u64::MAX,
        };
        let mut segment: LruSegment<&str, i32, DefaultHashBuilder> =
            LruSegment::init(config, DefaultHashBuilder::default());
        assert_eq!(segment.len(), 0);
        assert!(segment.is_empty());
        assert_eq!(segment.cap().get(), 2);
        segment.put("a", 1);
        segment.put("b", 2);
        assert_eq!(segment.len(), 2);
        assert_eq!(segment.get(&"a"), Some(&1));
        assert_eq!(segment.get(&"b"), Some(&2));
    }

    #[test]
    fn test_lru_concurrent_access() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(make_cache::<String, i32>(100)));
        let num_threads = 4;
        let ops_per_thread = 100;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        // Spawn writer threads
        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("thread_{}_key_{}", t, i);
                    let mut guard = cache.lock().unwrap();
                    guard.put(key, t * 1000 + i);
                }
            }));
        }

        // Spawn reader threads
        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("thread_{}_key_{}", t, i);
                    let mut guard = cache.lock().unwrap();
                    let _ = guard.get(&key);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let mut guard = cache.lock().unwrap();
        assert!(guard.len() <= 100);
        assert!(!guard.is_empty());
        guard.clear(); // Clean up for MIRI
    }

    #[test]
    fn test_lru_high_contention() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(make_cache::<String, i32>(50)));
        let num_threads = 8;
        let ops_per_thread = 500;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}", i % 100); // Overlapping keys
                    let mut guard = cache.lock().unwrap();
                    if i % 2 == 0 {
                        guard.put(key, t * 1000 + i);
                    } else {
                        let _ = guard.get(&key);
                    }
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let mut guard = cache.lock().unwrap();
        assert!(guard.len() <= 50);
        guard.clear(); // Clean up for MIRI
    }

    #[test]
    fn test_lru_concurrent_mixed_operations() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(make_cache::<String, i32>(100)));
        let num_threads = 8;
        let ops_per_thread = 1000;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}", i % 200);
                    let mut guard = cache.lock().unwrap();

                    match i % 4 {
                        0 => {
                            guard.put(key, i);
                        }
                        1 => {
                            let _ = guard.get(&key);
                        }
                        2 => {
                            let _ = guard.get_mut(&key);
                        }
                        3 => {
                            let _ = guard.remove(&key);
                        }
                        _ => unreachable!(),
                    }

                    if i == 500 && t == 0 {
                        guard.clear();
                    }
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let mut guard = cache.lock().unwrap();
        assert!(guard.len() <= 100);
        guard.clear(); // Clean up for MIRI
    }

    #[test]
    fn test_lru_size_aware_tracking() {
        // Test that current_size and max_size are tracked correctly
        let mut cache = make_cache(10);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), u64::MAX);

        // Put items with explicit sizes
        cache.put_with_size("a", 1, 100);
        cache.put_with_size("b", 2, 200);
        cache.put_with_size("c", 3, 150);

        assert_eq!(cache.current_size(), 450);
        assert_eq!(cache.len(), 3);

        // Note: Current implementation doesn't track per-entry size on remove
        // The size metric tracks total insertions minus evictions

        // Clear should reset size
        cache.clear();
        assert_eq!(cache.current_size(), 0);
    }

    #[test]
    fn test_lru_init_constructor() {
        // Test the init constructor with size limit
        let config = LruCacheConfig {
            capacity: NonZeroUsize::new(1000).unwrap(),
            max_size: 1024 * 1024,
        };
        let cache: LruCache<String, i32> = LruCache::init(config, None);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_lru_with_limits_constructor() {
        // Test the with_limits constructor
        let config = LruCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            max_size: 1024 * 1024,
        };
        let cache: LruCache<String, String> = LruCache::init(config, None);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
        assert_eq!(cache.cap().get(), 100);
    }
}
