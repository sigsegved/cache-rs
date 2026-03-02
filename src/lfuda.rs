//! Least Frequently Used with Dynamic Aging (LFUDA) Cache Implementation
//!
//! LFUDA is an enhanced LFU algorithm that addresses the "cache pollution" problem
//! by incorporating a global age factor. This allows the cache to gradually forget
//! old access patterns, enabling new popular items to compete fairly against
//! historically popular but now cold items.
//!
//! # How the Algorithm Works
//!
//! LFUDA maintains a **global age** that increases whenever an item is evicted.
//! Each item's priority is the sum of its access frequency and the age at which
//! it was last accessed. This elegant mechanism ensures that:
//!
//! 1. Recently accessed items get a "boost" from the current global age
//! 2. Old items with high frequency but no recent access gradually become eviction candidates
//! 3. New items can compete fairly against established items
//!
//! ## Mathematical Formulation
//!
//! ```text
//! For each cache entry i:
//!   - F_i = access frequency of item i
//!   - A_i = global age when item i was last accessed
//!   - Priority_i = F_i + A_i
//!
//! On eviction:
//!   - Select item j where Priority_j = min{Priority_i for all i}
//!   - Set global_age = Priority_j
//!
//! On access:
//!   - Increment F_i
//!   - Update A_i = global_age (item gets current age)
//! ```
//!
//! ## Data Structure
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                           LFUDA Cache (global_age=100)                       │
//! │                                                                              │
//! │  HashMap<K, *Node>              BTreeMap<Priority, List>                     │
//! │  ┌──────────────┐              ┌─────────────────────────────────────────┐   │
//! │  │ "hot" ──────────────────────│ pri=115: [hot]    (freq=5, age=110)     │   │
//! │  │ "warm" ─────────────────────│ pri=103: [warm]   (freq=3, age=100)     │   │
//! │  │ "stale" ────────────────────│ pri=60:  [stale]  (freq=50, age=10) ←LFU│   │
//! │  └──────────────┘              └─────────────────────────────────────────┘   │
//! │                                        ▲                                     │
//! │                                        │                                     │
//! │                                   min_priority=60                            │
//! │                                                                              │
//! │  Note: "stale" has high frequency (50) but low age (10), making it the      │
//! │        eviction candidate despite being historically popular.                │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Operations
//!
//! | Operation | Action | Time |
//! |-----------|--------|------|
//! | `get(key)` | Increment frequency, update age to global_age | O(log P) |
//! | `put(key, value)` | Insert with priority=global_age+1, evict min priority | O(log P) |
//! | `remove(key)` | Remove from priority list | O(log P) |
//!
//! ## Aging Example
//!
//! ```text
//! global_age = 0
//!
//! put("a", 1)    →  a: freq=1, age=0, priority=1
//! get("a") x10   →  a: freq=11, age=0, priority=11
//! put("b", 2)    →  b: freq=1, age=0, priority=1
//! put("c", 3)    →  c: freq=1, age=0, priority=1
//!
//! [Cache full, add "d"]
//! - Evict min priority (b or c with priority=1)
//! - Set global_age = 1
//!
//! put("d", 4)    →  d: freq=1, age=1, priority=2
//!
//! [Many evictions later, global_age = 100]
//! - "a" still has priority=11 (no recent access)
//! - New item "e" gets priority=101
//! - "a" becomes eviction candidate despite high frequency!
//! ```
//!
//! # Dual-Limit Capacity
//!
//! This implementation supports two independent limits:
//!
//! - **`max_entries`**: Maximum number of items
//! - **`max_size`**: Maximum total size of content
//!
//! Eviction occurs when **either** limit would be exceeded.
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get | O(log P) |
//! | Put | O(log P) amortized |
//! | Remove | O(log P) |
//! | Memory per entry | ~110 bytes overhead + key×2 + value |
//!
//! Where P = number of distinct priority values. Since priority = frequency + age,
//! P can grow with the number of entries. BTreeMap provides O(log P) lookups.
//!
//! Slightly higher overhead than LFU due to age tracking per entry.
//!
//! # When to Use LFUDA
//!
//! **Good for:**
//! - Long-running caches where item popularity changes over time
//! - Web caches, CDN caches with evolving content popularity
//! - Systems that need frequency-based eviction but must adapt to trends
//! - Preventing "cache pollution" from historically popular but now stale items
//!
//! **Not ideal for:**
//! - Short-lived caches (aging doesn't have time to take effect)
//! - Static popularity patterns (pure LFU is simpler and equally effective)
//! - Strictly recency-based workloads (use LRU instead)
//!
//! # LFUDA vs LFU
//!
//! | Aspect | LFU | LFUDA |
//! |--------|-----|-------|
//! | Adapts to popularity changes | No | Yes |
//! | Memory overhead | Lower | Slightly higher |
//! | Complexity | Simpler | More complex |
//! | Best for | Stable patterns | Evolving patterns |
//!
//! # Thread Safety
//!
//! `LfudaCache` is **not thread-safe**. For concurrent access, either:
//! - Wrap with `Mutex` or `RwLock`
//! - Use `ConcurrentLfudaCache` (requires `concurrent` feature)
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```
//! use cache_rs::LfudaCache;
//! use cache_rs::config::LfudaCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = LfudaCacheConfig {
//!     capacity: NonZeroUsize::new(3).unwrap(),
//!     initial_age: 0,
//!     max_size: u64::MAX,
//! };
//! let mut cache = LfudaCache::init(config, None);
//!
//! cache.put("a", 1);
//! cache.put("b", 2);
//! cache.put("c", 3);
//!
//! // Access "a" to increase its priority
//! assert_eq!(cache.get(&"a"), Some(&1));
//! assert_eq!(cache.get(&"a"), Some(&1));
//!
//! // Add new item - lowest priority item evicted
//! cache.put("d", 4);
//!
//! // "a" survives due to higher priority (frequency + age)
//! assert_eq!(cache.get(&"a"), Some(&1));
//! ```
//!
//! ## Long-Running Cache with Aging
//!
//! ```
//! use cache_rs::LfudaCache;
//! use cache_rs::config::LfudaCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = LfudaCacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     initial_age: 0,
//!     max_size: u64::MAX,
//! };
//! let mut cache = LfudaCache::init(config, None);
//!
//! // Populate cache with initial data
//! for i in 0..100 {
//!     cache.put(i, i * 10);
//! }
//!
//! // Access some items frequently
//! for _ in 0..50 {
//!     cache.get(&0);  // Item 0 becomes very popular
//! }
//!
//! // Later: new content arrives, old content ages out
//! for i in 100..200 {
//!     cache.put(i, i * 10);  // Each insert may evict old items
//! }
//!
//! // Item 0 may eventually be evicted if not accessed recently,
//! // despite its historical popularity - this is the power of LFUDA!
//! ```

extern crate alloc;

use crate::config::LfudaCacheConfig;
use crate::entry::CacheEntry;
use crate::list::{List, ListEntry};
use crate::metrics::{CacheMetrics, LfudaCacheMetrics};

/// Metadata for LFUDA (LFU with Dynamic Aging) cache entries.
///
/// LFUDA is similar to LFU but addresses the "aging problem" where old
/// frequently-used items can prevent new items from being cached.
/// The age factor is maintained at the cache level, not per-entry.
///
/// # Algorithm
///
/// Entry priority = frequency + age_at_insertion
/// - When an item is evicted, global_age = evicted_item.priority
/// - New items start with current global_age as their insertion age
///
/// # Examples
///
/// ```
/// use cache_rs::lfuda::LfudaMeta;
///
/// let meta = LfudaMeta::new(1, 10); // frequency=1, age_at_insertion=10
/// assert_eq!(meta.frequency, 1);
/// assert_eq!(meta.age_at_insertion, 10);
/// assert_eq!(meta.priority(), 11);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LfudaMeta {
    /// Access frequency count.
    pub frequency: u64,
    /// Age value when this item was inserted (snapshot of global_age).
    pub age_at_insertion: u64,
}

impl LfudaMeta {
    /// Creates a new LFUDA metadata with the specified initial frequency and age.
    ///
    /// # Arguments
    ///
    /// * `frequency` - Initial frequency value (usually 1 for new items)
    /// * `age_at_insertion` - The global_age at the time of insertion
    #[inline]
    pub fn new(frequency: u64, age_at_insertion: u64) -> Self {
        Self {
            frequency,
            age_at_insertion,
        }
    }

    /// Increments the frequency counter and returns the new value.
    #[inline]
    pub fn increment(&mut self) -> u64 {
        self.frequency += 1;
        self.frequency
    }

    /// Calculates the effective priority (frequency + age_at_insertion).
    #[inline]
    pub fn priority(&self) -> u64 {
        self.frequency + self.age_at_insertion
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

/// Internal LFUDA segment containing the actual cache algorithm.
///
/// This is shared between `LfudaCache` (single-threaded) and
/// `ConcurrentLfudaCache` (multi-threaded). All algorithm logic is
/// implemented here to avoid code duplication.
///
/// Uses `CacheEntry<K, V, LfudaMeta>` for unified entry management with built-in
/// size tracking, timestamps, and LFUDA-specific metadata (frequency + age_at_insertion).
///
/// # Safety
///
/// This struct contains raw pointers in the `map` field.
/// These pointers are always valid as long as:
/// - The pointer was obtained from a `priority_lists` entry's `add()` call
/// - The node has not been removed from the list
/// - The segment has not been dropped
pub(crate) struct LfudaSegment<K, V, S = DefaultHashBuilder> {
    /// Configuration for the LFUDA cache (includes capacity and max_size)
    config: LfudaCacheConfig,

    /// Global age value that increases when items are evicted
    global_age: u64,

    /// Current minimum effective priority in the cache
    min_priority: u64,

    /// Map from keys to their node pointer.
    /// All metadata (frequency, age, size) is stored in CacheEntry.
    map: HashMap<K, *mut ListEntry<CacheEntry<K, V, LfudaMeta>>, S>,

    /// Map from effective priority to list of items with that priority
    /// Items within each priority list are ordered by recency (LRU within priority)
    priority_lists: BTreeMap<u64, List<CacheEntry<K, V, LfudaMeta>>>,

    /// Metrics tracking for this cache instance
    metrics: LfudaCacheMetrics,

    /// Current total size of cached content (sum of entry sizes)
    current_size: u64,
}

// SAFETY: LfudaSegment owns all data and raw pointers point only to nodes owned by
// `priority_lists`. Concurrent access is safe when wrapped in proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for LfudaSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for LfudaSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LfudaSegment<K, V, S> {
    /// Creates a new LFUDA segment from a configuration.
    ///
    /// This is the **recommended** way to create an LFUDA segment. All configuration
    /// is specified through the [`LfudaCacheConfig`] struct.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity, initial age, and optional size limit
    /// * `hasher` - Hash builder for the internal HashMap
    #[allow(dead_code)] // Used by concurrent module when feature is enabled
    pub(crate) fn init(config: LfudaCacheConfig, hasher: S) -> Self {
        let map_capacity = config.capacity.get().next_power_of_two();
        LfudaSegment {
            config,
            global_age: config.initial_age as u64,
            min_priority: 0,
            map: HashMap::with_capacity_and_hasher(map_capacity, hasher),
            priority_lists: BTreeMap::new(),
            metrics: LfudaCacheMetrics::new(config.max_size),
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

    /// Returns the current global age value.
    #[inline]
    pub(crate) fn global_age(&self) -> u64 {
        self.global_age
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

    /// Returns a reference to the metrics for this segment.
    #[inline]
    pub(crate) fn metrics(&self) -> &LfudaCacheMetrics {
        &self.metrics
    }

    /// Estimates the size of a key-value pair in bytes for metrics tracking
    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    /// Records a cache miss for metrics tracking
    #[inline]
    pub(crate) fn record_miss(&mut self, object_size: u64) {
        self.metrics.core.record_miss(object_size);
    }

    /// Updates the priority of an item and moves it to the appropriate priority list.
    ///
    /// # Safety
    ///
    /// The caller must ensure that `node` is a valid pointer to a ListEntry that exists
    /// in this cache's priority_lists and has not been freed.
    unsafe fn update_priority_by_node(
        &mut self,
        node: *mut ListEntry<CacheEntry<K, V, LfudaMeta>>,
        old_priority: u64,
    ) -> *mut ListEntry<CacheEntry<K, V, LfudaMeta>>
    where
        K: Clone + Hash + Eq,
    {
        // SAFETY: node is guaranteed to be valid by the caller's contract
        let entry = (*node).get_value();
        let key_cloned = entry.key.clone();

        // Get current node from map
        let node = *self.map.get(&key_cloned).unwrap();

        // Calculate new priority after incrementing frequency
        let meta = &(*node).get_value().metadata.algorithm;
        let new_priority = (meta.frequency + 1) + meta.age_at_insertion;

        // If priority hasn't changed, just move to front of the same list
        if old_priority == new_priority {
            self.priority_lists
                .get_mut(&old_priority)
                .unwrap()
                .move_to_front(node);
            return node;
        }

        // Remove from old priority list
        let boxed_entry = self
            .priority_lists
            .get_mut(&old_priority)
            .unwrap()
            .remove(node)
            .unwrap();

        // Clean up empty old priority list and update min_priority if necessary
        if self.priority_lists.get(&old_priority).unwrap().is_empty() {
            self.priority_lists.remove(&old_priority);
            if old_priority == self.min_priority {
                self.min_priority = new_priority;
            }
        }

        // Update frequency in the entry's metadata
        let entry_ptr = Box::into_raw(boxed_entry);
        (*entry_ptr).get_value_mut().metadata.algorithm.frequency += 1;

        // Ensure the new priority list exists
        let capacity = self.config.capacity;
        self.priority_lists
            .entry(new_priority)
            .or_insert_with(|| List::new(capacity));

        // Add to the front of the new priority list (most recently used within that priority)
        self.priority_lists
            .get_mut(&new_priority)
            .unwrap()
            .attach_from_other_list(entry_ptr);

        // Update the map with the new node pointer
        *self.map.get_mut(&key_cloned).unwrap() = entry_ptr;

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
                // SAFETY: node comes from our map
                let entry = (*node).get_value();
                let meta = &entry.metadata.algorithm;
                let old_priority = meta.priority();
                self.metrics.core.record_hit(entry.metadata.size);

                let new_node = self.update_priority_by_node(node, old_priority);
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
                // SAFETY: node comes from our map
                let entry = (*node).get_value();
                let meta = &entry.metadata.algorithm;
                let old_priority = meta.priority();
                self.metrics.core.record_hit(entry.metadata.size);

                let new_priority = (meta.frequency + 1) + meta.age_at_insertion;
                self.metrics.record_frequency_increment(new_priority);

                let new_node = self.update_priority_by_node(node, old_priority);
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
                // SAFETY: node comes from our map
                let entry = (*node).get_value();
                let meta = &entry.metadata.algorithm;
                let priority = meta.priority();
                let old_size = entry.metadata.size;

                // Create new CacheEntry with same frequency and age
                let new_entry = CacheEntry::with_algorithm_metadata(
                    key.clone(),
                    value,
                    size,
                    LfudaMeta::new(meta.frequency, meta.age_at_insertion),
                );

                let old_entry = self
                    .priority_lists
                    .get_mut(&priority)
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

        // Add new item with frequency 1 and current global age
        let frequency: u64 = 1;
        let age_at_insertion = self.global_age;
        let priority = frequency + age_at_insertion;

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

        self.min_priority = if self.is_empty() {
            priority
        } else {
            core::cmp::min(self.min_priority, priority)
        };

        // Ensure priority list exists
        let capacity = self.config.capacity;
        self.priority_lists
            .entry(priority)
            .or_insert_with(|| List::new(capacity));

        // Create CacheEntry with LfudaMeta
        let cache_entry = CacheEntry::with_algorithm_metadata(
            key.clone(),
            value,
            size,
            LfudaMeta::new(frequency, age_at_insertion),
        );

        if let Some(node) = self
            .priority_lists
            .get_mut(&priority)
            .unwrap()
            .add(cache_entry)
        {
            self.map.insert(key, node);
            self.current_size += size;

            self.metrics.core.record_insertion(size);
            self.metrics.record_frequency_increment(priority);
            if age_at_insertion > 0 {
                self.metrics.record_aging_benefit(age_at_insertion);
            }
        }

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
            // Read priority before removal — needed to find the correct priority list
            let priority = (*node).get_value().metadata.algorithm.priority();

            let boxed_entry = self.priority_lists.get_mut(&priority)?.remove(node)?;
            let entry_ptr = Box::into_raw(boxed_entry);
            let cache_entry = (*entry_ptr).take_value();
            let removed_size = cache_entry.metadata.size;
            let _ = Box::from_raw(entry_ptr);

            self.current_size = self.current_size.saturating_sub(removed_size);
            self.metrics.core.record_removal(removed_size);

            // Clean up empty priority list and update min_priority if necessary
            if self.priority_lists.get(&priority).unwrap().is_empty() {
                self.priority_lists.remove(&priority);
                if priority == self.min_priority {
                    self.min_priority = self
                        .priority_lists
                        .keys()
                        .copied()
                        .next()
                        .unwrap_or(self.global_age);
                }
            }

            Some(cache_entry.value)
        }
    }

    /// Clears the segment, removing all key-value pairs.
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.priority_lists.clear();
        self.global_age = 0;
        self.min_priority = 0;
        self.current_size = 0;
    }

    /// Check if key exists without updating its priority or access metadata.
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

    /// Returns a reference to the value without updating priority or access metadata.
    ///
    /// Unlike `get()`, this method does NOT increment the entry's frequency,
    /// change its priority, or move it between priority lists.
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

    /// Removes and returns the eviction candidate (lowest priority entry).
    ///
    /// Also updates the global age to the evicted item's priority (LFUDA aging).
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

        // Find the actual minimum non-empty priority list.
        // self.min_priority may be stale if remove() left empty lists behind.
        let min_key = loop {
            if let Some(min_priority_list) = self.priority_lists.get(&self.min_priority) {
                if !min_priority_list.is_empty() {
                    break self.min_priority;
                }
                // Clean up empty list and advance
                let stale = self.min_priority;
                self.priority_lists.remove(&stale);
                self.min_priority = self
                    .priority_lists
                    .keys()
                    .copied()
                    .next()
                    .unwrap_or(self.global_age);
            } else {
                // min_priority doesn't exist in the map, recalculate
                self.min_priority = self
                    .priority_lists
                    .keys()
                    .copied()
                    .next()
                    .unwrap_or(self.global_age);
                // If there are still no lists, the cache is effectively empty
                if self.priority_lists.is_empty() {
                    return None;
                }
            }
        };

        let min_priority_list = self.priority_lists.get_mut(&min_key)?;
        let old_entry = min_priority_list.remove_last()?;
        let is_list_empty = min_priority_list.is_empty();

        unsafe {
            // SAFETY: take_value moves the CacheEntry out by value.
            // Box::from_raw frees memory (MaybeUninit won't double-drop).
            let entry_ptr = Box::into_raw(old_entry);
            let cache_entry = (*entry_ptr).take_value();
            let evicted_size = cache_entry.metadata.size;
            let evicted_priority = cache_entry.metadata.algorithm.priority();

            // Update global age to the evicted item's priority (LFUDA aging)
            self.global_age = evicted_priority;
            self.metrics.record_aging_event(self.global_age);

            self.map.remove(&cache_entry.key);
            self.current_size = self.current_size.saturating_sub(evicted_size);
            self.metrics.core.record_removal(evicted_size);

            // Update min_priority if the list is now empty
            if is_list_empty {
                self.priority_lists.remove(&self.min_priority);
                self.min_priority = self
                    .priority_lists
                    .keys()
                    .copied()
                    .next()
                    .unwrap_or(self.global_age);
            }

            let _ = Box::from_raw(entry_ptr);
            Some((cache_entry.key, cache_entry.value))
        }
    }

    /// Removes and returns the highest priority entry (reverse of pop).
    ///
    /// This is the opposite of `pop()` - instead of returning the lowest priority
    /// item, it returns the highest priority item.
    ///
    /// This method does **not** increment the eviction counter in metrics.
    ///
    /// Returns `None` if the cache is empty.
    pub(crate) fn pop_r(&mut self) -> Option<(K, V)> {
        if self.is_empty() {
            return None;
        }

        // Get the highest priority (last key in BTreeMap)
        let max_priority = *self.priority_lists.keys().next_back()?;
        let max_priority_list = self.priority_lists.get_mut(&max_priority)?;
        let entry = max_priority_list.remove_first()?;
        let is_list_empty = max_priority_list.is_empty();

        unsafe {
            // SAFETY: take_value moves the CacheEntry out by value.
            // Box::from_raw frees memory (MaybeUninit won't double-drop).
            let entry_ptr = Box::into_raw(entry);
            let cache_entry = (*entry_ptr).take_value();
            let evicted_size = cache_entry.metadata.size;
            self.map.remove(&cache_entry.key);
            self.current_size = self.current_size.saturating_sub(evicted_size);
            self.metrics.core.record_removal(evicted_size);

            // Remove empty priority list
            if is_list_empty {
                self.priority_lists.remove(&max_priority);
            }

            let _ = Box::from_raw(entry_ptr);
            Some((cache_entry.key, cache_entry.value))
        }
    }

    /// Returns the minimum priority in this segment, or `None` if empty.
    ///
    /// Used by the concurrent cache to compare eviction priorities across segments.
    #[allow(dead_code)]
    pub(crate) fn peek_min_priority(&self) -> Option<u64> {
        if self.is_empty() {
            None
        } else {
            Some(self.min_priority)
        }
    }

    /// Returns the maximum priority in this segment, or `None` if empty.
    ///
    /// Used by the concurrent cache to compare priorities across segments for `pop_r()`.
    #[allow(dead_code)]
    pub(crate) fn peek_max_priority(&self) -> Option<u64> {
        if self.is_empty() {
            None
        } else {
            self.priority_lists.keys().next_back().copied()
        }
    }
}

// Implement Debug for LfudaSegment manually since it contains raw pointers
impl<K, V, S> core::fmt::Debug for LfudaSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LfudaSegment")
            .field("capacity", &self.config.capacity)
            .field("len", &self.map.len())
            .field("global_age", &self.global_age)
            .field("min_priority", &self.min_priority)
            .finish()
    }
}

/// An implementation of a Least Frequently Used with Dynamic Aging (LFUDA) cache.
///
/// LFUDA improves upon LFU by adding an aging mechanism that prevents old frequently-used
/// items from blocking new items indefinitely. Each item's effective priority is calculated
/// as (access_frequency + age_at_insertion), where age_at_insertion is the global age
/// value when the item was first inserted.
///
/// When an item is evicted, the global age is set to the evicted item's effective priority,
/// ensuring that new items start with a competitive priority.
///
/// # Examples
///
/// ```
/// use cache_rs::lfuda::LfudaCache;
/// use cache_rs::config::LfudaCacheConfig;
/// use core::num::NonZeroUsize;
///
/// // Create an LFUDA cache with capacity 3
/// let config = LfudaCacheConfig {
///     capacity: NonZeroUsize::new(3).unwrap(),
///     initial_age: 0,
///     max_size: u64::MAX,
/// };
/// let mut cache = LfudaCache::init(config, None);
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
/// // Add more items to trigger aging
/// cache.put("d", 4); // This will evict an item and increase global age
/// cache.put("e", 5); // New items benefit from the increased age
/// ```
#[derive(Debug)]
pub struct LfudaCache<K, V, S = DefaultHashBuilder> {
    segment: LfudaSegment<K, V, S>,
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LfudaCache<K, V, S> {
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

    /// Returns the current global age value.
    #[inline]
    pub fn global_age(&self) -> u64 {
        self.segment.global_age()
    }

    /// Records a cache miss for metrics tracking (to be called by simulation system)
    #[inline]
    pub fn record_miss(&mut self, object_size: u64) {
        self.segment.record_miss(object_size);
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
    /// Otherwise, if the cache is at capacity, the item with the lowest effective priority
    /// is evicted. The global age is updated to the evicted item's effective priority.
    ///
    /// New items are inserted with a frequency of 1 and age_at_insertion set to the
    /// current global_age.
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
    /// Resets the global age to 0.
    #[inline]
    pub fn clear(&mut self) {
        self.segment.clear()
    }

    /// Check if key exists without updating its priority or access metadata.
    ///
    /// Unlike `get()`, this method does NOT update the entry's frequency
    /// or access metadata. Useful for existence checks without affecting
    /// cache eviction order.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use cache_rs::config::LfudaCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfudaCacheConfig {
    ///     capacity: NonZeroUsize::new(2).unwrap(),
    ///     initial_age: 0,
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LfudaCache::init(config, None);
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    ///
    /// // contains() does NOT update priority
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

    /// Returns a reference to the value without updating priority or access metadata.
    ///
    /// Unlike [`get()`](Self::get), this does NOT increment the entry's frequency,
    /// change its priority, or move it between priority lists.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use cache_rs::config::LfudaCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfudaCacheConfig {
    ///     capacity: NonZeroUsize::new(3).unwrap(),
    ///     initial_age: 0,
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LfudaCache::init(config, None);
    /// cache.put("a", 1);
    ///
    /// // peek does not change priority
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

    /// Removes and returns the eviction candidate (lowest priority entry).
    ///
    /// For LFUDA, this is the entry with the lowest priority (frequency + age_at_insertion).
    /// Also updates the global age to the evicted item's priority (LFUDA aging).
    ///
    /// Returns `None` if the cache is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use cache_rs::config::LfudaCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfudaCacheConfig {
    ///     capacity: NonZeroUsize::new(3).unwrap(),
    ///     initial_age: 0,
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LfudaCache::init(config, None);
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.get(&"b");  // Increase frequency of "b"
    ///
    /// // Pop the eviction candidate (lowest priority item)
    /// assert_eq!(cache.pop(), Some(("a", 1)));
    /// ```
    #[inline]
    pub fn pop(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        self.segment.pop()
    }

    /// Removes and returns the highest priority entry (reverse of pop).
    ///
    /// This is the opposite of `pop()` - instead of returning the lowest priority
    /// item, it returns the highest priority item.
    ///
    /// Returns `None` if the cache is empty.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use cache_rs::config::LfudaCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfudaCacheConfig {
    ///     capacity: NonZeroUsize::new(3).unwrap(),
    ///     initial_age: 0,
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache = LfudaCache::init(config, None);
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.get(&"b");  // Increase frequency of "b"
    /// cache.get(&"b");  // Increase frequency again
    ///
    /// // Pop the highest priority item
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

impl<K: Hash + Eq, V: Clone, S: BuildHasher> CacheMetrics for LfudaCache<K, V, S> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.segment.metrics().metrics()
    }

    fn algorithm_name(&self) -> &'static str {
        self.segment.metrics().algorithm_name()
    }
}

impl<K: Hash + Eq, V> LfudaCache<K, V>
where
    V: Clone,
{
    /// Creates a new LFUDA cache from a configuration.
    ///
    /// This is the **recommended** way to create an LFUDA cache. All configuration
    /// is specified through the [`LfudaCacheConfig`] struct, which uses a builder
    /// pattern for optional parameters.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity and optional size limit/initial age
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::LfudaCache;
    /// use cache_rs::config::LfudaCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// // Simple capacity-only cache
    /// let config = LfudaCacheConfig {
    ///     capacity: NonZeroUsize::new(100).unwrap(),
    ///     initial_age: 0,
    ///     max_size: u64::MAX,
    /// };
    /// let mut cache: LfudaCache<&str, i32> = LfudaCache::init(config, None);
    /// cache.put("key", 42);
    ///
    /// // Cache with size limit
    /// let config = LfudaCacheConfig {
    ///     capacity: NonZeroUsize::new(1000).unwrap(),
    ///     initial_age: 100,
    ///     max_size: 10 * 1024 * 1024,  // 10MB
    /// };
    /// let cache: LfudaCache<String, Vec<u8>> = LfudaCache::init(config, None);
    /// ```
    pub fn init(
        config: LfudaCacheConfig,
        hasher: Option<DefaultHashBuilder>,
    ) -> LfudaCache<K, V, DefaultHashBuilder> {
        LfudaCache {
            segment: LfudaSegment::init(config, hasher.unwrap_or_default()),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use alloc::string::ToString;

    use super::*;
    use crate::config::LfudaCacheConfig;
    use alloc::string::String;

    /// Helper to create an LfudaCache with the given capacity
    fn make_cache<K: Hash + Eq + Clone, V: Clone>(cap: usize) -> LfudaCache<K, V> {
        let config = LfudaCacheConfig {
            capacity: NonZeroUsize::new(cap).unwrap(),
            initial_age: 0,
            max_size: u64::MAX,
        };
        LfudaCache::init(config, None)
    }

    #[test]
    fn test_lfuda_basic() {
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

        // Add a new item, should evict "c" (lowest effective priority)
        let evicted = cache.put("d", 4);
        assert!(evicted.is_some());
        let (evicted_key, evicted_val) = evicted.unwrap();
        assert_eq!(evicted_key, "c");
        assert_eq!(evicted_val, 3);

        // Check remaining items
        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"d"), Some(&4));
        assert_eq!(cache.get(&"c"), None); // evicted
    }

    #[test]
    fn test_lfuda_aging() {
        let mut cache = make_cache(2);

        // Add items and access them
        cache.put("a", 1);
        cache.put("b", 2);

        // Access "a" many times
        for _ in 0..10 {
            cache.get(&"a");
        }

        // Initially, global age should be 0
        assert_eq!(cache.global_age(), 0);

        // Fill cache and cause eviction
        let evicted = cache.put("c", 3);
        assert!(evicted.is_some());

        // Global age should have increased after eviction
        assert!(cache.global_age() > 0);

        // New items should benefit from the increased global age
        cache.put("d", 4);

        // The new item should start with competitive priority due to aging
        assert!(cache.len() <= cache.cap().get());
    }

    #[test]
    fn test_lfuda_priority_calculation() {
        let mut cache = make_cache(3);

        cache.put("a", 1);
        assert_eq!(cache.global_age(), 0);

        // Access "a" to increase its frequency
        cache.get(&"a");

        // Add more items
        cache.put("b", 2);
        cache.put("c", 3);

        // Force eviction to increase global age
        let evicted = cache.put("d", 4);
        assert!(evicted.is_some());

        // Global age should now be greater than 0
        assert!(cache.global_age() > 0);
    }

    #[test]
    fn test_lfuda_update_existing() {
        let mut cache = make_cache(2);

        cache.put("a", 1);
        cache.get(&"a"); // Increase frequency

        // Update existing key
        let old_value = cache.put("a", 10);
        assert_eq!(old_value.unwrap().1, 1);

        // Add another item
        cache.put("b", 2);
        cache.put("c", 3); // Should evict "b" because "a" has higher effective priority

        assert_eq!(cache.get(&"a"), Some(&10));
        assert_eq!(cache.get(&"c"), Some(&3));
        assert_eq!(cache.get(&"b"), None);
    }

    #[test]
    fn test_lfuda_remove() {
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
    fn test_lfuda_clear() {
        let mut cache = make_cache(3);

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Force some aging
        cache.get(&"a");
        cache.put("d", 4); // This should increase global_age

        let age_before_clear = cache.global_age();
        assert!(age_before_clear > 0);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert_eq!(cache.global_age(), 0); // Should reset to 0

        // Should be able to add items after clear
        cache.put("e", 5);
        assert_eq!(cache.get(&"e"), Some(&5));
    }

    #[test]
    fn test_lfuda_get_mut() {
        let mut cache = make_cache(2);

        cache.put("a", 1);

        // Modify value using get_mut
        if let Some(value) = cache.get_mut(&"a") {
            *value = 10;
        }

        assert_eq!(cache.get(&"a"), Some(&10));
    }

    #[test]
    fn test_lfuda_complex_values() {
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

    #[test]
    fn test_lfuda_aging_advantage() {
        let mut cache = make_cache(2);

        // Add and heavily access an old item
        cache.put("old", 1);
        for _ in 0..100 {
            cache.get(&"old");
        }

        // Fill cache
        cache.put("temp", 2);

        // Force eviction to age the cache
        let _evicted = cache.put("new1", 3);
        let age_after_first_eviction = cache.global_age();

        // Add more items to further age the cache
        let _evicted = cache.put("new2", 4);
        let age_after_second_eviction = cache.global_age();

        // The global age should have increased
        assert!(age_after_second_eviction >= age_after_first_eviction);

        // Now add a brand new item - it should benefit from the aging
        cache.put("newer", 5);

        // The newer item should be able to compete despite the old item's high frequency
        assert!(cache.len() <= cache.cap().get());
    }

    /// Test to validate the fix for Stacked Borrows violations detected by Miri.
    #[test]
    fn test_miri_stacked_borrows_fix() {
        let mut cache = make_cache(10);

        // Insert some items
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access items multiple times to trigger priority updates
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

    // Test that LfudaSegment works correctly (internal tests)
    #[test]
    fn test_lfuda_segment_directly() {
        let config = LfudaCacheConfig {
            capacity: NonZeroUsize::new(3).unwrap(),
            initial_age: 0,
            max_size: u64::MAX,
        };
        let mut segment: LfudaSegment<&str, i32, DefaultHashBuilder> =
            LfudaSegment::init(config, DefaultHashBuilder::default());

        assert_eq!(segment.len(), 0);
        assert!(segment.is_empty());
        assert_eq!(segment.cap().get(), 3);
        assert_eq!(segment.global_age(), 0);

        segment.put("a", 1);
        segment.put("b", 2);
        assert_eq!(segment.len(), 2);

        // Access to increase frequency
        assert_eq!(segment.get(&"a"), Some(&1));
        assert_eq!(segment.get(&"b"), Some(&2));
    }

    #[test]
    fn test_lfuda_concurrent_access() {
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
                    let _ = guard.get(&key);
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
    fn test_lfuda_size_aware_tracking() {
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
    fn test_lfuda_init_constructor() {
        let config = LfudaCacheConfig {
            capacity: NonZeroUsize::new(1000).unwrap(),
            initial_age: 0,
            max_size: 1024 * 1024,
        };
        let cache: LfudaCache<String, i32> = LfudaCache::init(config, None);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
    }

    #[test]
    fn test_lfuda_with_limits_constructor() {
        let config = LfudaCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            initial_age: 0,
            max_size: 1024 * 1024,
        };
        let cache: LfudaCache<String, String> = LfudaCache::init(config, None);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
        assert_eq!(cache.cap().get(), 100);
    }

    #[test]
    fn test_lfuda_contains_non_promoting() {
        let mut cache = make_cache(2);
        cache.put("a", 1);
        cache.put("b", 2);

        // contains() should return true for existing keys
        assert!(cache.contains(&"a"));
        assert!(cache.contains(&"b"));
        assert!(!cache.contains(&"c"));

        // Access "b" to increase its priority
        cache.get(&"b");

        // contains() should NOT increase priority of "a"
        assert!(cache.contains(&"a"));

        // Adding "c" should evict "a" (lowest priority), not "b"
        cache.put("c", 3);
        assert!(!cache.contains(&"a")); // "a" was evicted
        assert!(cache.contains(&"b")); // "b" still exists
        assert!(cache.contains(&"c")); // "c" was just added
    }

    #[test]
    fn test_lfuda_pop_returns_lowest_priority() {
        let mut cache = make_cache(3);
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access "b" and "c" to increase their priorities
        cache.get(&"b");
        cache.get(&"c");
        cache.get(&"c"); // "c" now has higher priority

        // pop() should return the lowest priority item ("a")
        assert_eq!(cache.pop(), Some(("a", 1)));
        assert_eq!(cache.len(), 2);

        // Next lowest is "b"
        assert_eq!(cache.pop(), Some(("b", 2)));
        assert_eq!(cache.len(), 1);

        // Only "c" remains
        assert_eq!(cache.pop(), Some(("c", 3)));
        assert!(cache.is_empty());

        // Empty cache returns None
        assert_eq!(cache.pop(), None);
    }

    #[test]
    fn test_lfuda_pop_r_returns_highest_priority() {
        let mut cache = make_cache(3);
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access items to build different priorities
        cache.get(&"b"); // "b" priority increases
        cache.get(&"c"); // "c" priority increases
        cache.get(&"c"); // "c" priority increases more

        // pop_r() should return the highest priority item ("c")
        assert_eq!(cache.pop_r(), Some(("c", 3)));
        assert_eq!(cache.len(), 2);

        // "a" and "b" remain. Highest priority is "b"
        assert_eq!(cache.pop_r(), Some(("b", 2)));
        assert_eq!(cache.len(), 1);

        // Last item
        assert_eq!(cache.pop_r(), Some(("a", 1)));
        assert!(cache.is_empty());

        // Empty cache returns None
        assert_eq!(cache.pop_r(), None);
    }

    #[test]
    fn test_lfuda_pop_empty_cache() {
        let mut cache: LfudaCache<&str, i32> = make_cache(2);
        assert_eq!(cache.pop(), None);
        assert_eq!(cache.pop_r(), None);
    }

    #[test]
    fn test_lfuda_pop_updates_global_age() {
        let mut cache = make_cache(2);
        cache.put("a", 1);
        cache.put("b", 2);

        let initial_age = cache.global_age();

        // Access "b" to increase its priority
        cache.get(&"b");

        // Pop should evict "a" and update global age to its priority
        let popped = cache.pop();
        assert_eq!(popped, Some(("a", 1)));

        // Global age should have been updated (increased)
        let new_age = cache.global_age();
        assert!(
            new_age >= initial_age,
            "Global age should increase after pop: {} >= {}",
            new_age,
            initial_age
        );
    }

    #[test]
    fn test_lfuda_pop_single_element() {
        let mut cache = make_cache(2);
        cache.put("a", 1);

        let popped = cache.pop();
        assert_eq!(popped, Some(("a", 1)));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_lfuda_pop_r_single_element() {
        let mut cache = make_cache(2);
        cache.put("a", 1);

        let popped = cache.pop_r();
        assert_eq!(popped, Some(("a", 1)));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_lfuda_remove_cleans_up_empty_priority_lists() {
        // Regression test: remove() should clean up empty priority lists
        // from BTreeMap to avoid memory leaks and stale entries.
        let mut cache = make_cache(5);

        // Insert items that will have different priorities
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access "a" to bump its frequency (changes priority)
        cache.get(&"a");
        cache.get(&"a");

        // Remove items and verify cache stays consistent
        cache.remove(&"b");
        cache.remove(&"c");

        // Cache should still work correctly after removals
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.get(&"a"), Some(&1));

        // Insert new items - should not hit stale empty lists
        cache.put("d", 4);
        cache.put("e", 5);
        assert_eq!(cache.len(), 3);

        // pop() should work correctly without needing to skip empty lists
        let popped = cache.pop();
        assert!(popped.is_some());
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_lfuda_remove_non_min_priority_cleans_up() {
        // Verify that removing items at non-minimum priorities also cleans up empty lists.
        let mut cache = make_cache(5);

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        // Access "a" many times - it will have a higher priority
        for _ in 0..5 {
            cache.get(&"a");
        }

        // Remove "a" (high priority) - the high-priority list should be cleaned up
        cache.remove(&"a");
        assert_eq!(cache.len(), 2);

        // Remaining items should still be accessible
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), Some(&3));

        // pop/pop_r should work correctly
        let popped = cache.pop();
        assert!(popped.is_some());
    }

    #[test]
    fn test_lfuda_update_priority_cleans_up_empty_lists() {
        // Regression test: update_priority_by_node() should clean up empty
        // priority lists when moving a node to a new priority.
        let mut cache = make_cache(5);

        // Insert items at the same initial priority
        cache.put("a", 1);
        cache.put("b", 2);

        // Access "a" to move it to a new priority list.
        // If "a" was the only item at that priority, the old list should be removed.
        cache.get(&"a");

        // Now access "b" to also move it
        cache.get(&"b");

        // Both should still be accessible
        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));

        // Insert more items and verify eviction works correctly
        cache.put("c", 3);
        cache.put("d", 4);
        cache.put("e", 5);
        assert_eq!(cache.len(), 5);

        // Force eviction - should work without issues from stale empty lists
        cache.put("f", 6);
        assert_eq!(cache.len(), 5);

        // pop should work correctly
        let popped = cache.pop();
        assert!(popped.is_some());
        assert_eq!(cache.len(), 4);
    }
}
