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
//! | `get(key)` | Increment frequency, move to new frequency list | O(log F)* |
//! | `put(key, value)` | Insert at frequency 1, evict lowest freq if full | O(log F)* |
//! | `remove(key)` | Remove from frequency list, update min_frequency | O(log F)* |
//!
//! *Effectively O(1) since F (distinct frequencies) is bounded to a small constant.
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
//! | Get | O(log F) amortized, effectively O(1) |
//! | Put | O(log F) amortized, effectively O(1) |
//! | Remove | O(log F) amortized, effectively O(1) |
//! | Memory per entry | ~100 bytes overhead + key×2 + value |
//!
//! Where F = number of distinct frequency values. Since frequencies are small integers
//! (1, 2, 3, ...), F is typically bounded to a small constant (< 100 in practice),
//! making operations effectively O(1).
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
//! use cache_rs::config::LfuCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = LfuCacheConfig {
//!     capacity: NonZeroUsize::new(3).unwrap(),
//!     max_size: u64::MAX,
//! };
//! let mut cache = LfuCache::init(config, None);
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
//! use cache_rs::config::LfuCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! // Cache with max 1000 entries and 10MB total size
//! let config = LfuCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     max_size: 10 * 1024 * 1024,
//! };
//! let mut cache: LfuCache<String, Vec<u8>> = LfuCache::init(config, None);
//!
//! let data = vec![0u8; 1024];  // 1KB
//! cache.put_with_size("file.bin".to_string(), data.clone(), 1024);
//! ```

extern crate alloc;

use crate::config::LfuCacheConfig;
use crate::entry::CacheEntry;
use crate::list::{List, ListEntry};
use crate::metrics::{CacheMetrics, LfuCacheMetrics};

/// Metadata for LFU (Least Frequently Used) cache entries.
///
/// LFU tracks access frequency to evict the least frequently accessed items.
/// The frequency counter is incremented on each access.
///
/// # Examples
///
/// ```
/// use cache_rs::lfu::LfuMeta;
///
/// let mut meta = LfuMeta::default();
/// assert_eq!(meta.frequency, 0);
///
/// // Simulate access
/// meta.frequency += 1;
/// assert_eq!(meta.frequency, 1);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LfuMeta {
    /// Access frequency count.
    /// Incremented each time the entry is accessed.
    pub frequency: u64,
}

impl LfuMeta {
    /// Creates a new LFU metadata with the specified initial frequency.
    ///
    /// # Arguments
    ///
    /// * `frequency` - Initial frequency value (usually 0 or 1)
    #[inline]
    pub fn new(frequency: u64) -> Self {
        Self { frequency }
    }

    /// Increments the frequency counter and returns the new value.
    #[inline]
    pub fn increment(&mut self) -> u64 {
        self.frequency += 1;
        self.frequency
    }
}
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
    /// Creates a new LFU segment from a configuration.
    ///
    /// This is the **recommended** way to create an LFU segment. All configuration
    /// is specified through the [`LfuCacheConfig`] struct.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity and optional size limit
    /// * `hasher` - Hash builder for the internal HashMap
    #[allow(dead_code)] // Used by concurrent module when feature is enabled
    pub(crate) fn init(config: LfuCacheConfig, hasher: S) -> Self {
        let map_capacity = config.capacity.get().next_power_of_two();
        LfuSegment {
            config,
            min_frequency: 1,
            map: HashMap::with_capacity_and_hasher(map_capacity, hasher),
            frequency_lists: BTreeMap::new(),
            metrics: LfuCacheMetrics::new(config.max_size),
            current_size: 0,
        }
    }

    /// Returns the maximum number of key-value pairs the segment can hold.
    #[inline]
    pub(crate) fn cap(&self) -> NonZeroUsize {
        self.config.capacity
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
        self.config.max_size
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

        // If the old frequency list is now empty, remove it and update min_frequency
        if self.frequency_lists.get(&old_frequency).unwrap().is_empty() {
            self.frequency_lists.remove(&old_frequency);
            if old_frequency == self.min_frequency {
                self.min_frequency = new_frequency;
            }
        }

        // Update frequency in the entry's metadata
        let entry_ptr = Box::into_raw(boxed_entry);
        (*entry_ptr).get_value_mut().metadata.algorithm.frequency = new_frequency as u64;

        // Ensure the new frequency list exists
        let capacity = self.config.capacity;
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
                let frequency = entry.metadata.algorithm.frequency as usize;
                let object_size = entry.metadata.size;
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
                let frequency = entry.metadata.algorithm.frequency as usize;
                let object_size = entry.metadata.size;
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
                let frequency = entry.metadata.algorithm.frequency as usize;
                let old_size = entry.metadata.size;

                // Create new CacheEntry with same frequency
                let new_entry = CacheEntry::with_algorithm_metadata(
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
        while self.len() >= self.config.capacity.get()
            || (self.current_size + size > self.config.max_size && !self.map.is_empty())
        {
            if let Some(entry) = self.pop() {
                self.metrics.core.evictions += 1;
                evicted = Some(entry);
            } else {
                break;
            }
        }

        // Add new item with frequency 1
        let frequency = 1;
        self.min_frequency = 1;

        // Ensure frequency list exists
        let capacity = self.config.capacity;
        self.frequency_lists
            .entry(frequency)
            .or_insert_with(|| List::new(capacity));

        // Create CacheEntry with LfuMeta
        let cache_entry = CacheEntry::with_algorithm_metadata(
            key.clone(),
            value,
            size,
            LfuMeta::new(frequency as u64),
        );

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
    {
        let node = self.map.remove(key)?;

        unsafe {
            // SAFETY: node comes from our map; take_value moves the value out
            // and Box::from_raw frees memory (MaybeUninit won't double-drop).
            // Read frequency before removal — needed to find the correct frequency list
            let frequency = (*node).get_value().metadata.algorithm.frequency as usize;

            let boxed_entry = self.frequency_lists.get_mut(&frequency)?.remove(node)?;
            let entry_ptr = Box::into_raw(boxed_entry);
            let cache_entry = (*entry_ptr).take_value();
            let removed_size = cache_entry.metadata.size;
            let _ = Box::from_raw(entry_ptr);

            self.current_size = self.current_size.saturating_sub(removed_size);
            self.metrics.core.record_removal(removed_size);

            // Remove empty frequency list and update min_frequency if necessary
            if self.frequency_lists.get(&frequency).unwrap().is_empty() {
                self.frequency_lists.remove(&frequency);
                if frequency == self.min_frequency {
                    self.min_frequency = self.frequency_lists.keys().copied().next().unwrap_or(1);
                }
            }

            Some(cache_entry.value)
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

    /// Check if key exists without updating its frequency.
    ///
    /// Unlike `get()`, this method does NOT update the entry's frequency
    /// or access metadata.
    #[inline]
    pub(crate) fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.contains_key(key)
    }

    /// Returns a reference to the value without updating frequency or access metadata.
    ///
    /// Unlike `get()`, this method does NOT increment the entry's frequency
    /// or change its position in any frequency list.
    pub(crate) fn peek<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let &node = self.map.get(key)?;
        unsafe {
            // SAFETY: node comes from our map, so it's a valid pointer
            let entry = (*node).get_value();
            Some(&entry.value)
        }
    }

    /// Removes and returns the eviction candidate (lowest frequency entry).
    ///
    /// Returns the entry with the lowest frequency. In case of a tie,
    /// returns the least recently used entry among those with the same frequency.
    ///
    /// This method does **not** increment the eviction counter in metrics.
    /// Eviction metrics are only recorded when the cache internally evicts
    /// entries to make room during `put()`/`put_with_size()` operations.
    ///
    /// Returns `None` if the cache is empty.
    pub(crate) fn pop(&mut self) -> Option<(K, V)> {
        if self.is_empty() {
            return None;
        }

        let min_freq_list = self.frequency_lists.get_mut(&self.min_frequency)?;
        let old_entry = min_freq_list.remove_last()?;
        let is_list_empty = min_freq_list.is_empty();

        unsafe {
            // SAFETY: take_value moves the CacheEntry out by value.
            // Box::from_raw frees memory (MaybeUninit won't double-drop).
            let entry_ptr = Box::into_raw(old_entry);
            let cache_entry = (*entry_ptr).take_value();
            let evicted_size = cache_entry.metadata.size;
            self.map.remove(&cache_entry.key);
            self.current_size = self.current_size.saturating_sub(evicted_size);
            self.metrics.core.record_removal(evicted_size);

            // Update min_frequency if the list is now empty
            if is_list_empty {
                self.frequency_lists.remove(&self.min_frequency);
                self.min_frequency = self.frequency_lists.keys().copied().next().unwrap_or(1);
            }

            let _ = Box::from_raw(entry_ptr);
            Some((cache_entry.key, cache_entry.value))
        }
    }

    /// Removes and returns the highest frequency entry (reverse of pop).
    ///
    /// This is the opposite of `pop()` - instead of returning the lowest frequency
    /// item, it returns the highest frequency item.
    ///
    /// This method does **not** increment the eviction counter in metrics.
    ///
    /// Returns `None` if the cache is empty.
    pub(crate) fn pop_r(&mut self) -> Option<(K, V)> {
        if self.is_empty() {
            return None;
        }

        // Get the highest frequency (last key in BTreeMap)
        let max_frequency = *self.frequency_lists.keys().next_back()?;
        let max_freq_list = self.frequency_lists.get_mut(&max_frequency)?;
        let entry = max_freq_list.remove_first()?;
        let is_list_empty = max_freq_list.is_empty();

        unsafe {
            // SAFETY: take_value moves the CacheEntry out by value.
            // Box::from_raw frees memory (MaybeUninit won't double-drop).
            let entry_ptr = Box::into_raw(entry);
            let cache_entry = (*entry_ptr).take_value();
            let evicted_size = cache_entry.metadata.size;
            self.map.remove(&cache_entry.key);
            self.current_size = self.current_size.saturating_sub(evicted_size);
            self.metrics.core.record_removal(evicted_size);

            // Remove empty frequency list
            if is_list_empty {
                self.frequency_lists.remove(&max_frequency);
            }

            let _ = Box::from_raw(entry_ptr);
            Some((cache_entry.key, cache_entry.value))
        }
    }

    /// Returns the minimum frequency in this segment, or `None` if empty.
    ///
    /// Used by the concurrent cache to compare eviction priorities across segments.
    pub(crate) fn peek_min_frequency(&self) -> Option<usize> {
        if self.is_empty() {
            None
        } else {
            Some(self.min_frequency)
        }
    }

    /// Returns the maximum frequency in this segment, or `None` if empty.
    ///
    /// Used by the concurrent cache to compare priorities across segments for `pop_r()`.
    pub(crate) fn peek_max_frequency(&self) -> Option<usize> {
        if self.is_empty() {
            None
        } else {
            self.frequency_lists.keys().next_back().copied()
        }
    }
}

// Implement Debug for LfuSegment manually since it contains raw pointers
impl<K, V, S> core::fmt::Debug for LfuSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LfuSegment")
            .field("capacity", &self.config.capacity)
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
/// use cache_rs::config::LfuCacheConfig;
/// use core::num::NonZeroUsize;
///
/// // Create an LFU cache with capacity 3
/// let config = LfuCacheConfig {
///     capacity: NonZeroUsize::new(3).unwrap(),
///     max_size: u64::MAX,
/// };
/// let mut cache = LfuCache::init(config, None);
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
    ///
    /// # Multi-eviction behavior
    ///
    /// When using size-based caching (`max_size` is not `u64::MAX`), inserting
    /// a large entry may cause **multiple** smaller entries to be evicted to
    /// free enough space. In this case, only the **last** evicted entry is
    /// returned. For count-based caches, at most one entry is evicted.
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
    ///
    /// # Multi-eviction behavior
    ///
    /// When the new entry's size would exceed `max_size`, multiple existing
    /// entries may be evicted to free enough space. Only the **last** evicted
    /// entry is returned. All evicted entries are counted in the `evictions`
    /// metric.
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

    /// Check if key exists without updating its frequency.
    ///
    /// Unlike `get()`, this method does NOT update the entry's frequency
    /// or access metadata. Useful for existence checks without affecting
    /// cache eviction order.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LfuCache;
    /// use cache_rs::config::LfuCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfuCacheConfig {
    ///     capacity: NonZeroUsize::new(2).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LfuCache::init(config, None);
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    ///
    /// // contains() does NOT update frequency
    /// assert!(cache.contains(&"a"));
    /// ```
    #[inline]
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.contains(key)
    }

    /// Returns a reference to the value without updating frequency or access metadata.
    ///
    /// Unlike [`get()`](Self::get), this does NOT increment the entry's frequency
    /// or change its position in any frequency list.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LfuCache;
    /// use cache_rs::config::LfuCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfuCacheConfig {
    ///     capacity: NonZeroUsize::new(3).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LfuCache::init(config, None);
    /// cache.put("a", 1);
    ///
    /// // peek does not change frequency
    /// assert_eq!(cache.peek(&"a"), Some(&1));
    /// assert_eq!(cache.peek(&"missing"), None);
    /// ```
    #[inline]
    pub fn peek<Q>(&self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.peek(key)
    }

    /// Removes and returns the eviction candidate (lowest frequency entry).
    ///
    /// For LFU, this is the entry with the lowest frequency. In case of a tie,
    /// returns the least recently used entry among those with the same frequency.
    ///
    /// Returns `None` if the cache is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LfuCache;
    /// use cache_rs::config::LfuCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfuCacheConfig {
    ///     capacity: NonZeroUsize::new(3).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LfuCache::init(config, None);
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.get(&"b");  // Increase frequency of "b"
    ///
    /// // Pop the eviction candidate (lowest frequency item)
    /// assert_eq!(cache.pop(), Some(("a", 1)));
    /// ```
    #[inline]
    pub fn pop(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        self.segment.pop()
    }

    /// Removes and returns the highest frequency entry (reverse of pop).
    ///
    /// This is the opposite of `pop()` - instead of returning the lowest frequency
    /// item, it returns the highest frequency item.
    ///
    /// Returns `None` if the cache is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LfuCache;
    /// use cache_rs::config::LfuCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfuCacheConfig {
    ///     capacity: NonZeroUsize::new(3).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LfuCache::init(config, None);
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.get(&"b");  // Increase frequency of "b"
    /// cache.get(&"b");  // Increase frequency again
    ///
    /// // Pop the highest frequency item
    /// assert_eq!(cache.pop_r(), Some(("b", 2)));
    /// ```
    #[inline]
    pub fn pop_r(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        self.segment.pop_r()
    }
}

impl<K: Hash + Eq, V> LfuCache<K, V>
where
    V: Clone,
{
    /// Creates a new LFU cache from a configuration.
    ///
    /// This is the **recommended** way to create an LFU cache. All configuration
    /// is specified through the [`LfuCacheConfig`] struct.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity and optional size limit
    /// * `hasher` - Optional custom hash builder (uses default if `None`)
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LfuCache;
    /// use cache_rs::config::LfuCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// // Simple capacity-only cache
    /// let config = LfuCacheConfig {
    ///     capacity: NonZeroUsize::new(100).unwrap(),
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache: LfuCache<&str, i32> = LfuCache::init(config, None);
    /// cache.put("key", 42);
    ///
    /// // Cache with size limit
    /// let config = LfuCacheConfig {
    ///     capacity: NonZeroUsize::new(1000).unwrap(),
    ///     max_size: 10 * 1024 * 1024,  // 10MB
    /// };
    /// let cache: LfuCache<String, Vec<u8>> = LfuCache::init(config, None);
    /// ```
    pub fn init(
        config: LfuCacheConfig,
        hasher: Option<DefaultHashBuilder>,
    ) -> LfuCache<K, V, DefaultHashBuilder> {
        LfuCache {
            segment: LfuSegment::init(config, hasher.unwrap_or_default()),
        }
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
    use alloc::format;
    use std::println;
    use std::string::ToString;

    use super::*;
    use crate::config::LfuCacheConfig;
    use alloc::string::String;

    /// Helper to create an LfuCache with the given capacity
    fn make_cache<K: Hash + Eq + Clone, V: Clone>(cap: usize) -> LfuCache<K, V> {
        let config = LfuCacheConfig {
            capacity: NonZeroUsize::new(cap).unwrap(),
            max_size: u64::MAX,
        };
        LfuCache::init(config, None)
    }

    #[test]
    fn test_lfu_basic() {
        let mut cache = make_cache(3);

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
        let mut cache = make_cache(2);

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
        let mut cache = make_cache(2);

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
        let mut cache = make_cache(3);

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
        let mut cache = make_cache(3);

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
        let mut cache = make_cache(2);

        cache.put("a", 1);

        // Modify value using get_mut
        if let Some(value) = cache.get_mut(&"a") {
            *value = 10;
        }

        assert_eq!(cache.get(&"a"), Some(&10));
    }

    #[test]
    fn test_lfu_complex_values() {
        let mut cache = make_cache(2);

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
        let mut cache = make_cache(10);

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
        let config = LfuCacheConfig {
            capacity: NonZeroUsize::new(3).unwrap(),
            max_size: u64::MAX,
        };
        let mut segment: LfuSegment<&str, i32, DefaultHashBuilder> =
            LfuSegment::init(config, DefaultHashBuilder::default());

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

        let cache = Arc::new(Mutex::new(make_cache::<String, i32>(100)));
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
        let mut cache = make_cache(10);

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
    fn test_lfu_init_constructor() {
        let config = LfuCacheConfig {
            capacity: NonZeroUsize::new(1000).unwrap(),
            max_size: 1024 * 1024,
        };
        let cache: LfuCache<String, i32> = LfuCache::init(config, None);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
    }

    #[test]
    fn test_lfu_with_limits_constructor() {
        let config = LfuCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            max_size: 1024 * 1024,
        };
        let cache: LfuCache<String, String> = LfuCache::init(config, None);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
        assert_eq!(cache.cap().get(), 100);
    }

    #[test]
    fn test_lfu_frequency_list_accumulation() {
        // This test verifies that empty frequency lists don't accumulate in LFU cache.
        // Empty list accumulation was causing 300x slowdown in simulations (896s vs 3s).
        //
        // The test simulates a constrained scenario: small cache with high-cardinality traffic
        // (many unique keys), which historically triggered the empty list accumulation bug.
        use std::time::Instant;

        // Reproduce the constrained scenario: small cache, many unique keys
        let mut cache = make_cache(500);

        // Simulate high-cardinality traffic pattern
        // Use fewer operations under Miri to keep test time reasonable
        let num_ops = if cfg!(miri) { 5_000 } else { 50_000 };
        let start = Instant::now();
        for i in 0..num_ops {
            // Simulate 80-20: 80% of accesses go to 20% of keys
            let key = if i % 5 == 0 {
                format!("popular_{}", i % 100) // 100 popular keys
            } else {
                format!("long_tail_{}", i) // Many unique keys
            };
            cache.put(key.clone(), i);

            // Also do some gets to build frequency
            if i % 10 == 0 {
                for j in 0..10 {
                    cache.get(&format!("popular_{}", j));
                }
            }
        }
        let elapsed = start.elapsed();

        let num_freq_lists = cache.segment.frequency_lists.len();
        let empty_lists = cache
            .segment
            .frequency_lists
            .values()
            .filter(|l| l.is_empty())
            .count();

        println!(
            "{} ops in {:?} ({:.0} ops/sec)",
            num_ops,
            elapsed,
            num_ops as f64 / elapsed.as_secs_f64()
        );
        println!(
            "Frequency lists: {} total, {} empty",
            num_freq_lists, empty_lists
        );

        // Print frequency distribution
        let mut freq_counts: std::collections::BTreeMap<usize, usize> =
            std::collections::BTreeMap::new();
        for (freq, list) in &cache.segment.frequency_lists {
            if !list.is_empty() {
                *freq_counts.entry(*freq).or_insert(0) += list.len();
            }
        }
        println!("Frequency distribution (non-empty): {:?}", freq_counts);

        // The key correctness check: verify empty frequency lists don't accumulate
        // This was the bug that caused the 300x slowdown (896s vs 3s) in simulations
        assert!(
            empty_lists == 0,
            "LFU should not accumulate empty frequency lists. Found {} empty lists out of {} total",
            empty_lists,
            num_freq_lists
        );
    }

    #[test]
    fn test_lfu_contains_non_promoting() {
        let mut cache = make_cache(2);
        cache.put("a", 1);
        cache.put("b", 2);

        // contains() should return true for existing keys
        assert!(cache.contains(&"a"));
        assert!(cache.contains(&"b"));
        assert!(!cache.contains(&"c"));

        // Access "b" to increase its frequency
        cache.get(&"b");

        // contains() should NOT increase frequency of "a"
        // So "a" should still be the eviction candidate
        assert!(cache.contains(&"a"));

        // Adding "c" should evict "a" (lowest frequency), not "b"
        cache.put("c", 3);
        assert!(!cache.contains(&"a")); // "a" was evicted
        assert!(cache.contains(&"b")); // "b" still exists
        assert!(cache.contains(&"c")); // "c" was just added
    }

    #[test]
    fn test_lfu_pop_returns_lowest_frequency() {
        let mut cache = make_cache(3);
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access "b" and "c" to increase their frequencies
        cache.get(&"b");
        cache.get(&"c");
        cache.get(&"c"); // "c" now has frequency 3

        // pop() should return the lowest frequency item ("a" with frequency 1)
        assert_eq!(cache.pop(), Some(("a", 1)));
        assert_eq!(cache.len(), 2);

        // Next lowest is "b" (frequency 2)
        assert_eq!(cache.pop(), Some(("b", 2)));
        assert_eq!(cache.len(), 1);

        // Only "c" remains
        assert_eq!(cache.pop(), Some(("c", 3)));
        assert!(cache.is_empty());

        // Empty cache returns None
        assert_eq!(cache.pop(), None);
    }

    #[test]
    fn test_lfu_pop_r_returns_highest_frequency() {
        let mut cache = make_cache(3);
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access items to build different frequencies
        cache.get(&"b"); // "b" frequency = 2
        cache.get(&"c"); // "c" frequency = 2
        cache.get(&"c"); // "c" frequency = 3

        // pop_r() should return the highest frequency item ("c" with frequency 3)
        assert_eq!(cache.pop_r(), Some(("c", 3)));
        assert_eq!(cache.len(), 2);

        // "a" (freq 1) and "b" (freq 2) remain. Highest is "b"
        assert_eq!(cache.pop_r(), Some(("b", 2)));
        assert_eq!(cache.len(), 1);

        // Last item
        assert_eq!(cache.pop_r(), Some(("a", 1)));
        assert!(cache.is_empty());

        // Empty cache returns None
        assert_eq!(cache.pop_r(), None);
    }

    #[test]
    fn test_lfu_pop_empty_cache() {
        let mut cache: LfuCache<&str, i32> = make_cache(2);
        assert_eq!(cache.pop(), None);
        assert_eq!(cache.pop_r(), None);
    }

    #[test]
    fn test_lfu_pop_single_element() {
        let mut cache = make_cache(2);
        cache.put("a", 1);

        let popped = cache.pop();
        assert_eq!(popped, Some(("a", 1)));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_lfu_pop_r_single_element() {
        let mut cache = make_cache(2);
        cache.put("a", 1);

        let popped = cache.pop_r();
        assert_eq!(popped, Some(("a", 1)));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_lfu_pop_interleaved_with_put() {
        let mut cache = make_cache(3);
        cache.put("a", 1);
        cache.put("b", 2);

        // Pop lowest frequency
        assert_eq!(cache.pop(), Some(("a", 1)));

        // Add new items
        cache.put("c", 3);
        cache.put("d", 4);

        // "b", "c", "d" all have frequency 1 now. Pop order depends on recency
        assert_eq!(cache.len(), 3);

        // Access "d" to increase its frequency
        cache.get(&"d");

        // pop() should return one of the frequency-1 items ("b" or "c")
        let first_pop = cache.pop().unwrap();
        assert!(first_pop == ("b", 2) || first_pop == ("c", 3));

        // pop_r() should return "d" (highest frequency)
        assert_eq!(cache.pop_r(), Some(("d", 4)));
    }
}
