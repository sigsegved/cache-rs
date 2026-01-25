//! Least Frequently Used (LFU) Cache Implementation
//!
//! An LFU cache evicts the least frequently accessed item when capacity is reached.
//! This implementation tracks access frequency for each item and maintains items
//! organized by their frequency count using a combination of hash map and frequency-indexed lists.
//!
//! # How the Algorithm Works
//!
//! LFU is based on the principle that items accessed more frequently in the past
//! are more likely to be accessed again in the future. Unlike LRU which only considers
//! recency, LFU considers the total number of accesses.
//!
//! ## Data Structure
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                              LFU Cache                                       │
//! │                                                                              │
//! │  HashMap<K, *Node>              BTreeMap<Frequency, List>                    │
//! │  ┌──────────────┐              ┌─────────────────────────────────────────┐   │
//! │  │ "hot" ──────────────────────│ freq=10: [hot] ◀──▶ [warm]              │   │
//! │  │ "warm" ─────────────────────│ freq=5:  [item_a] ◀──▶ [item_b]         │   │
//! │  │ "cold" ─────────────────────│ freq=1:  [cold] ◀──▶ [new_item]  ← LFU  │   │
//! │  └──────────────┘              └─────────────────────────────────────────┘   │
//! │                                        ▲                                     │
//! │                                        │                                     │
//! │                                   min_frequency=1                            │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! - **HashMap**: Provides O(1) key lookup, storing pointers to list nodes
//! - **BTreeMap**: Maps frequency counts to lists of items with that frequency
//! - **min_frequency**: Tracks the lowest frequency for O(1) eviction
//!
//! ## Operations
//!
//! | Operation | Action | Time |
//! |-----------|--------|------|
//! | `get(key)` | Increment frequency, move to new frequency list | O(1) |
//! | `put(key, value)` | Insert at frequency 1, evict lowest freq if full | O(1) |
//! | `remove(key)` | Remove from frequency list, update min_frequency | O(1) |
//!
//! ## Access Pattern Example
//!
//! ```text
//! Cache capacity: 3
//!
//! put("a", 1)  →  freq_1: [a]
//! put("b", 2)  →  freq_1: [b, a]
//! put("c", 3)  →  freq_1: [c, b, a]
//! get("a")     →  freq_1: [c, b], freq_2: [a]
//! get("a")     →  freq_1: [c, b], freq_3: [a]
//! put("d", 4)  →  freq_1: [d, c], freq_3: [a]   // "b" evicted (LFU at freq_1)
//! ```
//!
//! # Dual-Limit Capacity
//!
//! This implementation supports two independent limits:
//!
//! - **`max_entries`**: Maximum number of items (bounds entry count)
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
//! | Memory per entry | ~100 bytes overhead + key×2 + value |
//!
//! Memory overhead includes: list node pointers (16B), `CacheEntry` metadata (32B),
//! frequency metadata (8B), HashMap bucket (~24B), BTreeMap overhead (~16B).
//!
//! # When to Use LFU
//!
//! **Good for:**
//! - Workloads with stable popularity patterns (some items are consistently popular)
//! - Database query caches where certain queries are repeated frequently
//! - CDN edge caches with predictable content popularity
//! - Scenarios requiring excellent scan resistance
//!
//! **Not ideal for:**
//! - Recency-based access patterns (use LRU instead)
//! - Workloads where popularity changes over time (use LFUDA instead)
//! - Short-lived caches where frequency counts don't accumulate meaningfully
//! - "Cache pollution" scenarios where old popular items block new ones
//!
//! # The Cache Pollution Problem
//!
//! A limitation of pure LFU: items that were popular in the past but are no longer
//! accessed can persist in the cache indefinitely due to high frequency counts.
//! For long-running caches with changing popularity, consider [`LfudaCache`](crate::LfudaCache)
//! which addresses this with dynamic aging.
//!
//! # Thread Safety
//!
//! `LfuCache` is **not thread-safe**. For concurrent access, either:
//! - Wrap with `Mutex` or `RwLock`
//! - Use `ConcurrentLfuCache` (requires `concurrent` feature)
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```
//! use cache_rs::LfuCache;
//! use core::num::NonZeroUsize;
//!
//! let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());
//!
//! cache.put("a", 1);
//! cache.put("b", 2);
//! cache.put("c", 3);
//!
//! // Access "a" multiple times - increases its frequency
//! assert_eq!(cache.get(&"a"), Some(&1));
//! assert_eq!(cache.get(&"a"), Some(&1));
//!
//! // Add new item - "b" or "c" evicted (both at frequency 1)
//! cache.put("d", 4);
//!
//! // "a" survives due to higher frequency
//! assert_eq!(cache.get(&"a"), Some(&1));
//! ```
//!
//! ## Size-Aware Caching
//!
//! ```
//! use cache_rs::LfuCache;
//! use core::num::NonZeroUsize;
//!
//! // Cache with max 1000 entries and 10MB total size
//! let mut cache: LfuCache<String, Vec<u8>> = LfuCache::with_limits(
//!     NonZeroUsize::new(1000).unwrap(),
//!     10 * 1024 * 1024,
//! );
//!
//! let data = vec![0u8; 1024];  // 1KB
//! cache.put_with_size("file.bin".to_string(), data.clone(), 1024);
//! ```

extern crate alloc;

use crate::config::LfuCacheConfig;
use crate::entry::CacheEntry;
use crate::list::{List, ListEntry};
use crate::meta::LfuMeta;
use crate::metrics::{CacheMetrics, LfuCacheMetrics};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash};
use core::mem;
use core::num::NonZeroUsize;

#[cfg(feature = "hashbrown")]
use hashbrown::hash_map::DefaultHashBuilder;
#[cfg(feature = "hashbrown")]
use hashbrown::HashMap;

#[cfg(not(feature = "hashbrown"))]
use std::collections::hash_map::RandomState as DefaultHashBuilder;
#[cfg(not(feature = "hashbrown"))]
use std::collections::HashMap;

/// Internal LFU segment containing the actual cache algorithm.
///
/// This is shared between `LfuCache` (single-threaded) and
/// `ConcurrentLfuCache` (multi-threaded). All algorithm logic is
/// implemented here to avoid code duplication.
///
/// Uses `CacheEntry<K, V, LfuMeta>` for unified entry management with built-in
/// size tracking, timestamps, and frequency metadata. The frequency is stored
/// only in `LfuMeta` (inside CacheEntry), eliminating duplication.
///
/// # Safety
///
/// This struct contains raw pointers in the `map` field. These pointers
/// are always valid as long as:
/// - The pointer was obtained from a `frequency_lists` entry's `add()` call
/// - The node has not been removed from the list
/// - The segment has not been dropped
pub(crate) struct LfuSegment<K, V, S = DefaultHashBuilder> {
    /// Configuration for the LFU cache (includes capacity and max_size)
    config: LfuCacheConfig,

    /// Current minimum frequency in the cache
    min_frequency: usize,

    /// Map from keys to their list node pointer.
    /// Frequency is stored in CacheEntry.metadata (LfuMeta), not duplicated here.
    map: HashMap<K, *mut ListEntry<CacheEntry<K, V, LfuMeta>>, S>,

    /// Map from frequency to list of items with that frequency
    /// Items within each frequency list are ordered by recency (LRU within frequency)
    frequency_lists: BTreeMap<usize, List<CacheEntry<K, V, LfuMeta>>>,

    /// Metrics for tracking cache performance and frequency distribution
    metrics: LfuCacheMetrics,

    /// Current total size of cached content (sum of entry sizes)
    current_size: u64,
}

// SAFETY: LfuSegment owns all data and raw pointers point only to nodes owned by
// `frequency_lists`. Concurrent access is safe when wrapped in proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for LfuSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for LfuSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LfuSegment<K, V, S> {
    /// Creates a new LFU segment with the specified capacity and hash builder.
    pub(crate) fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self::with_hasher_and_size(cap, hash_builder, u64::MAX)
    }

    /// Creates a new LFU segment with the specified capacity, hash builder, and max size.
    pub(crate) fn with_hasher_and_size(cap: NonZeroUsize, hash_builder: S, max_size: u64) -> Self {
        let config = LfuCacheConfig::with_max_size(cap, max_size);
        let map_capacity = config.capacity().get().next_power_of_two();
        LfuSegment {
            config,
            min_frequency: 1,
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            frequency_lists: BTreeMap::new(),
            metrics: LfuCacheMetrics::new(max_size),
            current_size: 0,
        }
    }

    /// Returns the maximum number of key-value pairs the segment can hold.
    #[inline]
    pub(crate) fn cap(&self) -> NonZeroUsize {
        self.config.capacity()
    }

    /// Returns the current number of key-value pairs in the segment.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the segment contains no key-value pairs.
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
        self.config.max_size()
    }

    /// Estimates the size of a key-value pair in bytes for metrics tracking
    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    /// Returns a reference to the metrics for this segment.
    #[inline]
    pub(crate) fn metrics(&self) -> &LfuCacheMetrics {
        &self.metrics
    }

    /// Updates the frequency of an item and moves it to the appropriate frequency list.
    /// Takes the node pointer directly to avoid aliasing issues.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `node` is a valid pointer to a ListEntry that exists
    /// in this cache's frequency lists and has not been freed.
    unsafe fn update_frequency_by_node(
        &mut self,
        node: *mut ListEntry<CacheEntry<K, V, LfuMeta>>,
        old_frequency: usize,
    ) -> *mut ListEntry<CacheEntry<K, V, LfuMeta>>
    where
        K: Clone + Hash + Eq,
    {
        let new_frequency = old_frequency + 1;

        // Record frequency increment
        self.metrics
            .record_frequency_increment(old_frequency, new_frequency);

        // SAFETY: node is guaranteed to be valid by the caller's contract
        let entry = (*node).get_value();
        let key_cloned = entry.key.clone();

        // Get the current node from the map
        let node = *self.map.get(&key_cloned).unwrap();

        // Remove from old frequency list
        let boxed_entry = self
            .frequency_lists
            .get_mut(&old_frequency)
            .unwrap()
            .remove(node)
            .unwrap();

        // If the old frequency list is now empty and it was the minimum frequency,
        // update the minimum frequency
        if self.frequency_lists.get(&old_frequency).unwrap().is_empty()
            && old_frequency == self.min_frequency
        {
            self.min_frequency = new_frequency;
        }

        // Update frequency in the entry's metadata
        let entry_ptr = Box::into_raw(boxed_entry);
        if let Some(ref mut meta) = (*entry_ptr).get_value_mut().metadata {
            meta.frequency = new_frequency as u64;
        }

        // Ensure the new frequency list exists
        let capacity = self.config.capacity();
        self.frequency_lists
            .entry(new_frequency)
            .or_insert_with(|| List::new(capacity));

        // Add to the front of the new frequency list (most recently used within that frequency)
        self.frequency_lists
            .get_mut(&new_frequency)
            .unwrap()
            .attach_from_other_list(entry_ptr);

        // Update the map with the new node pointer
        *self.map.get_mut(&key_cloned).unwrap() = entry_ptr;

        // Update metrics with new frequency levels
        self.metrics.update_frequency_levels(&self.frequency_lists);

        entry_ptr
    }

    /// Returns a reference to the value corresponding to the key.
    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(&node) = self.map.get(key) {
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our frequency list
                let entry = (*node).get_value();
                let frequency = entry
                    .metadata
                    .as_ref()
                    .map(|m| m.frequency as usize)
                    .unwrap_or(1);
                let object_size = entry.size;
                self.metrics.record_frequency_hit(object_size, frequency);

                let new_node = self.update_frequency_by_node(node, frequency);
                let new_entry = (*new_node).get_value();
                Some(&new_entry.value)
            }
        } else {
            None
        }
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(&node) = self.map.get(key) {
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our frequency list
                let entry = (*node).get_value();
                let frequency = entry
                    .metadata
                    .as_ref()
                    .map(|m| m.frequency as usize)
                    .unwrap_or(1);
                let object_size = entry.size;
                self.metrics.record_frequency_hit(object_size, frequency);

                let new_node = self.update_frequency_by_node(node, frequency);
                let new_entry = (*new_node).get_value_mut();
                Some(&mut new_entry.value)
            }
        } else {
            None
        }
    }

    /// Inserts a key-value pair into the segment.
    pub(crate) fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        K: Clone,
    {
        let object_size = self.estimate_object_size(&key, &value);
        self.put_with_size(key, value, object_size)
    }

    /// Insert a key-value pair with explicit size tracking.
    pub(crate) fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>
    where
        K: Clone,
    {
        // If key already exists, update it
        if let Some(&node) = self.map.get(&key) {
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our frequency list
                let entry = (*node).get_value();
                let frequency = entry
                    .metadata
                    .as_ref()
                    .map(|m| m.frequency as usize)
                    .unwrap_or(1);
                let old_size = entry.size;

                // Create new CacheEntry with same frequency
                let new_entry = CacheEntry::with_metadata(
                    key.clone(),
                    value,
                    size,
                    LfuMeta::new(frequency as u64),
                );

                let old_entry = self
                    .frequency_lists
                    .get_mut(&frequency)
                    .unwrap()
                    .update(node, new_entry, true);

                // Update size tracking
                self.current_size = self.current_size.saturating_sub(old_size);
                self.current_size += size;
                self.metrics.core.record_insertion(size);

                // Return old key-value pair
                return old_entry.0.map(|e| (e.key, e.value));
            }
        }

        let mut evicted = None;

        // Evict while entry count limit OR size limit would be exceeded
        while self.len() >= self.config.capacity().get()
            || (self.current_size + size > self.config.max_size() && !self.map.is_empty())
        {
            if let Some(min_freq_list) = self.frequency_lists.get_mut(&self.min_frequency) {
                if let Some(old_entry) = min_freq_list.remove_last() {
                    unsafe {
                        let entry_ptr = Box::into_raw(old_entry);
                        let cache_entry = (*entry_ptr).get_value();
                        let old_key = cache_entry.key.clone();
                        let old_value = cache_entry.value.clone();
                        let evicted_size = cache_entry.size;
                        self.current_size = self.current_size.saturating_sub(evicted_size);
                        self.metrics.core.record_eviction(evicted_size);
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));
                        let _ = Box::from_raw(entry_ptr);
                    }

                    // Update min_frequency if the list is now empty
                    if min_freq_list.is_empty() {
                        let old_min = self.min_frequency;
                        self.min_frequency = self
                            .frequency_lists
                            .keys()
                            .find(|&&f| {
                                f > old_min
                                    && !self
                                        .frequency_lists
                                        .get(&f)
                                        .map(|l| l.is_empty())
                                        .unwrap_or(true)
                            })
                            .copied()
                            .unwrap_or(1);
                    }
                } else {
                    break; // No more items in this frequency list
                }
            } else {
                break; // No frequency list at min_frequency
            }
        }

        // Add new item with frequency 1
        let frequency = 1;
        self.min_frequency = 1;

        // Ensure frequency list exists
        let capacity = self.config.capacity();
        self.frequency_lists
            .entry(frequency)
            .or_insert_with(|| List::new(capacity));

        // Create CacheEntry with LfuMeta
        let cache_entry =
            CacheEntry::with_metadata(key.clone(), value, size, LfuMeta::new(frequency as u64));

        if let Some(node) = self
            .frequency_lists
            .get_mut(&frequency)
            .unwrap()
            .add(cache_entry)
        {
            self.map.insert(key, node);
            self.current_size += size;
        }

        self.metrics.core.record_insertion(size);
        self.metrics.update_frequency_levels(&self.frequency_lists);

        evicted
    }

    /// Removes a key from the segment, returning the value if the key was present.
    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        let node = self.map.remove(key)?;

        unsafe {
            // SAFETY: node comes from our map and was just removed
            let entry = (*node).get_value();
            let frequency = entry
                .metadata
                .as_ref()
                .map(|m| m.frequency as usize)
                .unwrap_or(1);
            let removed_size = entry.size;
            let value = entry.value.clone();

            let boxed_entry = self.frequency_lists.get_mut(&frequency)?.remove(node)?;
            let _ = Box::from_raw(Box::into_raw(boxed_entry));

            self.current_size = self.current_size.saturating_sub(removed_size);

            // Update min_frequency if necessary
            if self.frequency_lists.get(&frequency).unwrap().is_empty()
                && frequency == self.min_frequency
            {
                self.min_frequency = self
                    .frequency_lists
                    .keys()
                    .find(|&&f| f > frequency && !self.frequency_lists.get(&f).unwrap().is_empty())
                    .copied()
                    .unwrap_or(1);
            }

            Some(value)
        }
    }

    /// Clears the segment, removing all key-value pairs.
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.frequency_lists.clear();
        self.min_frequency = 1;
        self.current_size = 0;
    }

    /// Records a cache miss for metrics tracking
    #[inline]
    pub(crate) fn record_miss(&mut self, object_size: u64) {
        self.metrics.record_miss(object_size);
    }
}

// Implement Debug for LfuSegment manually since it contains raw pointers
impl<K, V, S> core::fmt::Debug for LfuSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LfuSegment")
            .field("capacity", &self.config.capacity())
            .field("len", &self.map.len())
            .field("min_frequency", &self.min_frequency)
            .finish()
    }
}

/// An implementation of a Least Frequently Used (LFU) cache.
///
/// The cache tracks the frequency of access for each item and evicts the least
/// frequently used items when the cache reaches capacity. In case of a tie in
/// frequency, the least recently used item among those with the same frequency
/// is evicted.
///
/// # Examples
///
/// ```
/// use cache_rs::lfu::LfuCache;
/// use core::num::NonZeroUsize;
///
/// // Create an LFU cache with capacity 3
/// let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());
///
/// // Add some items
/// cache.put("a", 1);
/// cache.put("b", 2);
/// cache.put("c", 3);
///
/// // Access "a" multiple times to increase its frequency
/// assert_eq!(cache.get(&"a"), Some(&1));
/// assert_eq!(cache.get(&"a"), Some(&1));
///
/// // Add a new item, which will evict the least frequently used item
/// cache.put("d", 4);
/// assert_eq!(cache.get(&"b"), None); // "b" was evicted as it had frequency 0
/// ```
#[derive(Debug)]
pub struct LfuCache<K, V, S = DefaultHashBuilder> {
    segment: LfuSegment<K, V, S>,
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LfuCache<K, V, S> {
    /// Creates a new LFU cache with the specified capacity and hash builder.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfu::LfuCache;
    /// use core::num::NonZeroUsize;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let cache: LfuCache<&str, u32, _> = LfuCache::with_hasher(
    ///     NonZeroUsize::new(10).unwrap(),
    ///     RandomState::new()
    /// );
    /// ```
    pub fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self {
            segment: LfuSegment::with_hasher(cap, hash_builder),
        }
    }

    /// Creates a new LFU cache with the specified capacity, hash builder, and max size.
    pub fn with_hasher_and_size(cap: NonZeroUsize, hash_builder: S, max_size: u64) -> Self {
        Self {
            segment: LfuSegment::with_hasher_and_size(cap, hash_builder, max_size),
        }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    #[inline]
    pub fn cap(&self) -> NonZeroUsize {
        self.segment.cap()
    }

    /// Returns the current number of key-value pairs in the cache.
    #[inline]
    pub fn len(&self) -> usize {
        self.segment.len()
    }

    /// Returns `true` if the cache contains no key-value pairs.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.segment.is_empty()
    }

    /// Returns the current total size of cached content.
    #[inline]
    pub fn current_size(&self) -> u64 {
        self.segment.current_size()
    }

    /// Returns the maximum content size the cache can hold.
    #[inline]
    pub fn max_size(&self) -> u64 {
        self.segment.max_size()
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// Accessing an item increases its frequency count.
    #[inline]
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get(key)
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// Accessing an item increases its frequency count.
    #[inline]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get_mut(key)
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache already contained this key, the old value is replaced and returned.
    /// Otherwise, if the cache is at capacity, the least frequently used item is evicted.
    /// In case of a tie in frequency, the least recently used item among those with
    /// the same frequency is evicted.
    ///
    /// New items are inserted with a frequency of 1.
    #[inline]
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        K: Clone,
    {
        self.segment.put(key, value)
    }

    /// Insert a key-value pair with explicit size tracking.
    ///
    /// The `size` parameter specifies how much of `max_size` this entry consumes.
    /// Use `size=1` for count-based caches.
    #[inline]
    pub fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>
    where
        K: Clone,
    {
        self.segment.put_with_size(key, value, size)
    }

    /// Removes a key from the cache, returning the value at the key if the key was previously in the cache.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    #[inline]
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        self.segment.remove(key)
    }

    /// Clears the cache, removing all key-value pairs.
    #[inline]
    pub fn clear(&mut self) {
        self.segment.clear()
    }

    /// Records a cache miss for metrics tracking (to be called by simulation system)
    #[inline]
    pub fn record_miss(&mut self, object_size: u64) {
        self.segment.record_miss(object_size);
    }
}

impl<K: Hash + Eq, V> LfuCache<K, V>
where
    V: Clone,
{
    /// Creates a new LFU cache with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfu::LfuCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let cache: LfuCache<&str, u32> = LfuCache::new(NonZeroUsize::new(10).unwrap());
    /// ```
    pub fn new(cap: NonZeroUsize) -> LfuCache<K, V, DefaultHashBuilder> {
        let config = LfuCacheConfig::new(cap);
        LfuCache::with_hasher(config.capacity(), DefaultHashBuilder::default())
    }

    /// Creates a size-based LFU cache (max size only, effectively unlimited entries).
    ///
    /// Useful for in-memory caches bounded by total memory.
    ///
    /// # Example
    /// ```
    /// use cache_rs::lfu::LfuCache;
    ///
    /// // 10 MB cache
    /// let mut cache: LfuCache<String, Vec<u8>> = LfuCache::with_max_size(10 * 1024 * 1024);
    /// // cache.put_with_size("image.png".into(), bytes.clone(), bytes.len() as u64);
    /// ```
    pub fn with_max_size(max_size: u64) -> LfuCache<K, V, DefaultHashBuilder> {
        // Use a large but reasonable entry limit to avoid excessive memory pre-allocation
        // 10 million entries * ~100 bytes overhead = ~1GB cache index memory
        let max_entries = NonZeroUsize::new(10_000_000).unwrap();
        LfuCache::with_hasher_and_size(max_entries, DefaultHashBuilder::default(), max_size)
    }

    /// Creates a dual-limit LFU cache.
    ///
    /// Evicts when EITHER limit would be exceeded:
    /// - `max_entries`: bounds cache-rs memory (~150 bytes per entry)
    /// - `max_size`: bounds content storage (sum of `size` params)
    ///
    /// # Example
    /// ```
    /// use cache_rs::lfu::LfuCache;
    /// use core::num::NonZeroUsize;
    ///
    /// // 5M entries (~750MB RAM for index), 100GB tracked content
    /// let cache: LfuCache<String, String> = LfuCache::with_limits(
    ///     NonZeroUsize::new(5_000_000).unwrap(),
    ///     100 * 1024 * 1024 * 1024
    /// );
    /// ```
    pub fn with_limits(
        max_entries: NonZeroUsize,
        max_size: u64,
    ) -> LfuCache<K, V, DefaultHashBuilder> {
        LfuCache::with_hasher_and_size(max_entries, DefaultHashBuilder::default(), max_size)
    }
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> CacheMetrics for LfuCache<K, V, S> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.segment.metrics().metrics()
    }

    fn algorithm_name(&self) -> &'static str {
        self.segment.metrics().algorithm_name()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::string::ToString;

    use super::*;
    use alloc::string::String;

    #[test]
    fn test_lfu_basic() {
        let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());

        // Add items
        assert_eq!(cache.put("a", 1), None);
        assert_eq!(cache.put("b", 2), None);
        assert_eq!(cache.put("c", 3), None);

        // Access "a" multiple times to increase its frequency
        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"a"), Some(&1));

        // Access "b" once
        assert_eq!(cache.get(&"b"), Some(&2));

        // Add a new item, should evict "c" (frequency 0, least recently used among frequency 0)
        let evicted = cache.put("d", 4);
        assert!(evicted.is_some());
        let (evicted_key, evicted_val) = evicted.unwrap();
        assert_eq!(evicted_key, "c");
        assert_eq!(evicted_val, 3);

        // Check remaining items
        assert_eq!(cache.get(&"a"), Some(&1)); // frequency 3 now
        assert_eq!(cache.get(&"b"), Some(&2)); // frequency 2 now
        assert_eq!(cache.get(&"d"), Some(&4)); // frequency 1 now
        assert_eq!(cache.get(&"c"), None); // evicted
    }

    #[test]
    fn test_lfu_frequency_ordering() {
        let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());

        // Add items
        cache.put("a", 1);
        cache.put("b", 2);

        // Access "a" multiple times
        cache.get(&"a");
        cache.get(&"a");
        cache.get(&"a");

        // Access "b" once
        cache.get(&"b");

        // Add new item, should evict "b" (lower frequency)
        let evicted = cache.put("c", 3);
        assert_eq!(evicted.unwrap().0, "b");

        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.get(&"b"), None);
    }

    #[test]
    fn test_lfu_update_existing() {
        let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1);
        cache.get(&"a"); // frequency becomes 2

        // Update existing key
        let old_value = cache.put("a", 10);
        assert_eq!(old_value.unwrap().1, 1);

        // The frequency should be preserved
        cache.put("b", 2);
        cache.put("c", 3); // Should evict "b" because "a" has higher frequency

        assert_eq!(cache.get(&"a"), Some(&10));
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.get(&"b"), None);
    }

    #[test]
    fn test_lfu_remove() {
        let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Remove item
        assert_eq!(cache.remove(&"b"), Some(2));
        assert_eq!(cache.remove(&"b"), None);

        // Check remaining items
        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_lfu_clear() {
        let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        assert_eq!(cache.len(), 3);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        // Should be able to add items after clear
        cache.put("d", 4);
        assert_eq!(cache.get(&"d"), Some(&4));
    }

    #[test]
    fn test_lfu_get_mut() {
        let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1);

        // Modify value using get_mut
        if let Some(value) = cache.get_mut(&"a") {
            *value = 10;
        }

        assert_eq!(cache.get(&"a"), Some(&10));
    }

    #[test]
    fn test_lfu_complex_values() {
        let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());

        #[derive(Debug, Clone, PartialEq)]
        struct ComplexValue {
            id: usize,
            data: String,
        }

        // Add complex values
        cache.put(
            "a",
            ComplexValue {
                id: 1,
                data: "a-data".to_string(),
            },
        );

        cache.put(
            "b",
            ComplexValue {
                id: 2,
                data: "b-data".to_string(),
            },
        );

        // Modify a value using get_mut
        if let Some(value) = cache.get_mut(&"a") {
            value.id = 100;
            value.data = "a-modified".to_string();
        }

        // Check the modified value
        let a = cache.get(&"a").unwrap();
        assert_eq!(a.id, 100);
        assert_eq!(a.data, "a-modified");
    }

    /// Test to validate the fix for Stacked Borrows violations detected by Miri.
    #[test]
    fn test_miri_stacked_borrows_fix() {
        let mut cache = LfuCache::new(NonZeroUsize::new(10).unwrap());

        // Insert some items
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access items multiple times to trigger frequency updates
        for _ in 0..3 {
            assert_eq!(cache.get(&"a"), Some(&1));
            assert_eq!(cache.get(&"b"), Some(&2));
            assert_eq!(cache.get(&"c"), Some(&3));
        }

        assert_eq!(cache.len(), 3);

        // Test with get_mut as well
        if let Some(val) = cache.get_mut(&"a") {
            *val += 10;
        }
        assert_eq!(cache.get(&"a"), Some(&11));
    }

    // Test that LfuSegment works correctly (internal tests)
    #[test]
    fn test_lfu_segment_directly() {
        let mut segment: LfuSegment<&str, i32, DefaultHashBuilder> =
            LfuSegment::with_hasher(NonZeroUsize::new(3).unwrap(), DefaultHashBuilder::default());

        assert_eq!(segment.len(), 0);
        assert!(segment.is_empty());
        assert_eq!(segment.cap().get(), 3);

        segment.put("a", 1);
        segment.put("b", 2);
        assert_eq!(segment.len(), 2);

        // Access to increase frequency
        assert_eq!(segment.get(&"a"), Some(&1));
        assert_eq!(segment.get(&"a"), Some(&1));
        assert_eq!(segment.get(&"b"), Some(&2));
    }

    #[test]
    fn test_lfu_concurrent_access() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(LfuCache::new(NonZeroUsize::new(100).unwrap())));
        let num_threads = 4;
        let ops_per_thread = 100;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}_{}", t, i);
                    let mut guard = cache.lock().unwrap();
                    guard.put(key.clone(), i);
                    // Access some keys multiple times to test frequency tracking
                    if i % 3 == 0 {
                        let _ = guard.get(&key);
                        let _ = guard.get(&key);
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
    fn test_lfu_size_aware_tracking() {
        let mut cache = LfuCache::new(NonZeroUsize::new(10).unwrap());

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), u64::MAX);

        cache.put_with_size("a", 1, 100);
        cache.put_with_size("b", 2, 200);
        cache.put_with_size("c", 3, 150);

        assert_eq!(cache.current_size(), 450);
        assert_eq!(cache.len(), 3);

        // Clear should reset size
        cache.clear();
        assert_eq!(cache.current_size(), 0);
    }

    #[test]
    fn test_lfu_with_max_size_constructor() {
        let cache: LfuCache<String, i32> = LfuCache::with_max_size(1024 * 1024);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
    }

    #[test]
    fn test_lfu_with_limits_constructor() {
        let cache: LfuCache<String, String> =
            LfuCache::with_limits(NonZeroUsize::new(100).unwrap(), 1024 * 1024);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
        assert_eq!(cache.cap().get(), 100);
    }
}
