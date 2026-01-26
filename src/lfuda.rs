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
//! | `get(key)` | Increment frequency, update age to global_age | O(1) |
//! | `put(key, value)` | Insert with priority=global_age+1, evict min priority | O(1) |
//! | `remove(key)` | Remove from priority list | O(1) |
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
//! | Get | O(1) |
//! | Put | O(1) |
//! | Remove | O(1) |
//! | Memory per entry | ~110 bytes overhead + key×2 + value |
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
//! use core::num::NonZeroUsize;
//!
//! let mut cache = LfudaCache::new(NonZeroUsize::new(3).unwrap());
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
//! use core::num::NonZeroUsize;
//!
//! let mut cache = LfudaCache::new(NonZeroUsize::new(100).unwrap());
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
use crate::meta::LfudaMeta;
use crate::metrics::{CacheMetrics, LfudaCacheMetrics};
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
    /// Creates a new LFUDA segment with the specified capacity and hash builder.
    pub(crate) fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self::with_hasher_and_size(cap, hash_builder, u64::MAX)
    }

    /// Creates a new LFUDA segment with the specified capacity, hash builder, and max size.
    pub(crate) fn with_hasher_and_size(cap: NonZeroUsize, hash_builder: S, max_size: u64) -> Self {
        let config = LfudaCacheConfig::with_max_size(cap, max_size);
        let map_capacity = config.capacity().get().next_power_of_two();

        LfudaSegment {
            config,
            global_age: 0,
            min_priority: 0,
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            priority_lists: BTreeMap::new(),
            metrics: LfudaCacheMetrics::new(max_size),
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
        self.config.max_size()
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
        let meta = (*node).get_value().metadata.as_ref().unwrap();
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

        // If the old priority list is now empty and it was the minimum priority,
        // update the minimum priority
        if self.priority_lists.get(&old_priority).unwrap().is_empty()
            && old_priority == self.min_priority
        {
            self.min_priority = new_priority;
        }

        // Update frequency in the entry's metadata
        let entry_ptr = Box::into_raw(boxed_entry);
        if let Some(ref mut meta) = (*entry_ptr).get_value_mut().metadata {
            meta.frequency += 1;
        }

        // Ensure the new priority list exists
        let capacity = self.config.capacity();
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
                let meta = entry.metadata.as_ref().unwrap();
                let old_priority = meta.priority();
                self.metrics.core.record_hit(entry.size);

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
                let meta = entry.metadata.as_ref().unwrap();
                let old_priority = meta.priority();
                self.metrics.core.record_hit(entry.size);

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
                let meta = entry.metadata.as_ref().unwrap();
                let priority = meta.priority();
                let old_size = entry.size;

                // Create new CacheEntry with same frequency and age
                let new_entry = CacheEntry::with_metadata(
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
        while self.len() >= self.config.capacity().get()
            || (self.current_size + size > self.config.max_size() && !self.map.is_empty())
        {
            if let Some(min_priority_list) = self.priority_lists.get_mut(&self.min_priority) {
                if let Some(old_entry) = min_priority_list.remove_last() {
                    unsafe {
                        let entry_ptr = Box::into_raw(old_entry);
                        let cache_entry = (*entry_ptr).get_value();
                        let old_key = cache_entry.key.clone();
                        let old_value = cache_entry.value.clone();
                        let evicted_size = cache_entry.size;

                        // Update global age to the evicted item's effective priority
                        if let Some(ref meta) = cache_entry.metadata {
                            self.global_age = meta.priority();
                            self.metrics.record_aging_event(self.global_age);
                        }

                        self.current_size = self.current_size.saturating_sub(evicted_size);
                        self.metrics.core.record_eviction(evicted_size);
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));
                        let _ = Box::from_raw(entry_ptr);

                        // Update min_priority if the list becomes empty
                        if min_priority_list.is_empty() {
                            self.min_priority = self
                                .priority_lists
                                .keys()
                                .find(|&&p| {
                                    p > self.min_priority
                                        && !self
                                            .priority_lists
                                            .get(&p)
                                            .map(|l| l.is_empty())
                                            .unwrap_or(true)
                                })
                                .copied()
                                .unwrap_or(priority);
                        }
                    }
                } else {
                    break; // No more items to evict
                }
            } else {
                break; // No priority list at min_priority
            }
        }

        self.min_priority = if self.is_empty() {
            priority
        } else {
            core::cmp::min(self.min_priority, priority)
        };

        // Ensure priority list exists
        let capacity = self.config.capacity();
        self.priority_lists
            .entry(priority)
            .or_insert_with(|| List::new(capacity));

        // Create CacheEntry with LfudaMeta
        let cache_entry = CacheEntry::with_metadata(
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
        V: Clone,
    {
        let node = self.map.remove(key)?;

        unsafe {
            // SAFETY: node comes from our map and was just removed
            let entry = (*node).get_value();
            let meta = entry.metadata.as_ref().unwrap();
            let priority = meta.priority();
            let removed_size = entry.size;
            let value = entry.value.clone();

            let boxed_entry = self.priority_lists.get_mut(&priority)?.remove(node)?;
            let _ = Box::from_raw(Box::into_raw(boxed_entry));

            self.current_size = self.current_size.saturating_sub(removed_size);

            // Update min_priority if necessary
            if self.priority_lists.get(&priority).unwrap().is_empty()
                && priority == self.min_priority
            {
                self.min_priority = self
                    .priority_lists
                    .keys()
                    .find(|&&p| {
                        p > priority
                            && !self
                                .priority_lists
                                .get(&p)
                                .map(|l| l.is_empty())
                                .unwrap_or(true)
                    })
                    .copied()
                    .unwrap_or(self.global_age);
            }

            Some(value)
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
}

// Implement Debug for LfudaSegment manually since it contains raw pointers
impl<K, V, S> core::fmt::Debug for LfudaSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LfudaSegment")
            .field("capacity", &self.config.capacity())
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
/// use core::num::NonZeroUsize;
///
/// // Create an LFUDA cache with capacity 3
/// let mut cache = LfudaCache::new(NonZeroUsize::new(3).unwrap());
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
    /// Creates a new LFUDA cache with the specified capacity and hash builder.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use core::num::NonZeroUsize;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let cache: LfudaCache<&str, u32, _> = LfudaCache::with_hasher(
    ///     NonZeroUsize::new(10).unwrap(),
    ///     RandomState::new()
    /// );
    /// ```
    pub fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self {
            segment: LfudaSegment::with_hasher(cap, hash_builder),
        }
    }

    /// Creates a new LFUDA cache with the specified capacity, hash builder, and max size.
    pub fn with_hasher_and_size(cap: NonZeroUsize, hash_builder: S, max_size: u64) -> Self {
        Self {
            segment: LfudaSegment::with_hasher_and_size(cap, hash_builder, max_size),
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
    /// Resets the global age to 0.
    #[inline]
    pub fn clear(&mut self) {
        self.segment.clear()
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
    /// Creates a new LFUDA cache with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let cache: LfudaCache<&str, u32> = LfudaCache::new(NonZeroUsize::new(10).unwrap());
    /// ```
    pub fn new(cap: NonZeroUsize) -> LfudaCache<K, V, DefaultHashBuilder> {
        let config = LfudaCacheConfig::new(cap);
        LfudaCache::with_hasher(config.capacity(), DefaultHashBuilder::default())
    }

    /// Creates a size-based LFUDA cache (max size only, effectively unlimited entries).
    ///
    /// Useful for in-memory caches bounded by total memory.
    ///
    /// # Example
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    ///
    /// // 10 MB cache
    /// let mut cache: LfudaCache<String, Vec<u8>> = LfudaCache::with_max_size(10 * 1024 * 1024);
    /// // cache.put_with_size("image.png".into(), bytes.clone(), bytes.len() as u64);
    /// ```
    pub fn with_max_size(max_size: u64) -> LfudaCache<K, V, DefaultHashBuilder> {
        // Use a large but reasonable entry limit to avoid excessive memory pre-allocation
        // 10 million entries * ~100 bytes overhead = ~1GB cache index memory
        let max_entries = NonZeroUsize::new(10_000_000).unwrap();
        LfudaCache::with_hasher_and_size(max_entries, DefaultHashBuilder::default(), max_size)
    }

    /// Creates a dual-limit LFUDA cache.
    ///
    /// Evicts when EITHER limit would be exceeded:
    /// - `max_entries`: bounds cache-rs memory (~150 bytes per entry)
    /// - `max_size`: bounds content storage (sum of `size` params)
    ///
    /// # Example
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use core::num::NonZeroUsize;
    ///
    /// // 5M entries (~750MB RAM for index), 100GB tracked content
    /// let cache: LfudaCache<String, String> = LfudaCache::with_limits(
    ///     NonZeroUsize::new(5_000_000).unwrap(),
    ///     100 * 1024 * 1024 * 1024
    /// );
    /// ```
    pub fn with_limits(
        max_entries: NonZeroUsize,
        max_size: u64,
    ) -> LfudaCache<K, V, DefaultHashBuilder> {
        LfudaCache::with_hasher_and_size(max_entries, DefaultHashBuilder::default(), max_size)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use alloc::string::ToString;

    use super::*;
    use alloc::string::String;

    #[test]
    fn test_lfuda_basic() {
        let mut cache = LfudaCache::new(NonZeroUsize::new(3).unwrap());

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
        let mut cache = LfudaCache::new(NonZeroUsize::new(2).unwrap());

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
        let mut cache = LfudaCache::new(NonZeroUsize::new(3).unwrap());

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
        let mut cache = LfudaCache::new(NonZeroUsize::new(2).unwrap());

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
        let mut cache = LfudaCache::new(NonZeroUsize::new(3).unwrap());

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
        let mut cache = LfudaCache::new(NonZeroUsize::new(3).unwrap());

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
        let mut cache = LfudaCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1);

        // Modify value using get_mut
        if let Some(value) = cache.get_mut(&"a") {
            *value = 10;
        }

        assert_eq!(cache.get(&"a"), Some(&10));
    }

    #[test]
    fn test_lfuda_complex_values() {
        let mut cache = LfudaCache::new(NonZeroUsize::new(2).unwrap());

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
        let mut cache = LfudaCache::new(NonZeroUsize::new(2).unwrap());

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
        let mut cache = LfudaCache::new(NonZeroUsize::new(10).unwrap());

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
        let mut segment: LfudaSegment<&str, i32, DefaultHashBuilder> =
            LfudaSegment::with_hasher(NonZeroUsize::new(3).unwrap(), DefaultHashBuilder::default());

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

        let cache = Arc::new(Mutex::new(LfudaCache::new(NonZeroUsize::new(100).unwrap())));
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
        let mut cache = LfudaCache::new(NonZeroUsize::new(10).unwrap());

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
    fn test_lfuda_with_max_size_constructor() {
        let cache: LfudaCache<String, i32> = LfudaCache::with_max_size(1024 * 1024);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
    }

    #[test]
    fn test_lfuda_with_limits_constructor() {
        let cache: LfudaCache<String, String> =
            LfudaCache::with_limits(NonZeroUsize::new(100).unwrap(), 1024 * 1024);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
        assert_eq!(cache.cap().get(), 100);
    }
}
