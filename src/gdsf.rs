//! Greedy Dual-Size Frequency (GDSF) Cache Implementation
//!
//! GDSF is a sophisticated cache replacement algorithm designed for **variable-sized objects**.
//! It combines object size, access frequency, and aging into a unified priority formula,
//! making it ideal for CDN caches, image caches, and any scenario where cached objects
//! have different sizes.
//!
//! # How the Algorithm Works
//!
//! GDSF calculates priority for each cached item using the formula:
//!
//! ```text
//! Priority = (Frequency / Size) + GlobalAge
//! ```
//!
//! This formula cleverly balances multiple factors:
//! - **Frequency**: More frequently accessed items get higher priority
//! - **Size**: Smaller items are favored (more items fit in cache)
//! - **Aging**: Prevents old popular items from staying forever
//!
//! ## Mathematical Formulation
//!
//! ```text
//! For each cache entry i:
//!   - F_i = access frequency of item i
//!   - S_i = size of item i (in bytes)
//!   - GlobalAge = increases on eviction (set to evicted item's priority)
//!   - Priority_i = (F_i / S_i) + GlobalAge
//!
//! On eviction: select item j where Priority_j = min{Priority_i for all i}
//! ```
//!
//! ## Why Size Matters
//!
//! Consider a cache with 10KB capacity:
//! - Option A: Cache one 10KB file accessed 10 times → Priority = 10/10000 = 0.001
//! - Option B: Cache ten 1KB files accessed once each → Each Priority = 1/1000 = 0.001
//!
//! GDSF recognizes that caching many small frequently-accessed items often yields
//! better hit rates than caching fewer large items.
//!
//! ## Data Structure
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                    GDSF Cache (global_age=1.5, max_size=10MB)               │
//! │                                                                              │
//! │  HashMap<K, *Node>              BTreeMap<Priority, List>                     │
//! │  ┌──────────────┐              ┌─────────────────────────────────────────┐   │
//! │  │ "icon.png" ─────────────────│ pri=3.5: [icon.png]   (f=2, s=1KB)      │   │
//! │  │ "thumb.jpg" ────────────────│ pri=2.5: [thumb.jpg]  (f=1, s=1KB)      │   │
//! │  │ "video.mp4" ────────────────│ pri=1.5: [video.mp4]  (f=10, s=100MB)←E │   │
//! │  └──────────────┘              └─────────────────────────────────────────┘   │
//! │                                        ▲                                     │
//! │                                        │                                     │
//! │                                   min_priority=1.5                           │
//! │                                                                              │
//! │  Note: video.mp4 has high frequency (10) but large size (100MB),            │
//! │        so its priority = 10/100000000 + 1.5 ≈ 1.5 (eviction candidate)      │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Operations
//!
//! | Operation | Action | Time |
//! |-----------|--------|------|
//! | `get(key)` | Increment frequency, recalculate priority | O(1) |
//! | `put(key, value, size)` | Insert with priority=(1/size)+age | O(1) |
//! | `remove(key)` | Remove from priority list, update min_priority | O(1) |
//!
//! ## Size-Aware Example
//!
//! ```text
//! Cache: max_size=5KB, global_age=0
//!
//! put("small.txt", data, 1KB)   →  pri=1.0, total_size=1KB
//! put("medium.txt", data, 2KB)  →  pri=0.5, total_size=3KB
//! put("large.txt", data, 3KB)   →  pri=0.33, total_size=6KB  OVERFLOW!
//!
//! Eviction needed: evict "large.txt" (lowest priority=0.33)
//! global_age = 0.33
//!
//! put("large.txt", data, 3KB)   →  pri=0.33+0.33=0.66, total_size=6KB  OVERFLOW!
//!
//! Evict again: now "medium.txt" has lowest priority (0.5 < 0.66)
//! Result: small.txt + large.txt fit in 4KB
//! ```
//!
//! # Dual-Limit Capacity
//!
//! GDSF naturally works with size-based limits:
//!
//! - **`max_entries`**: Maximum number of items (prevents too many tiny items)
//! - **`max_size`**: Maximum total size (primary constraint for GDSF)
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get | O(1) |
//! | Put | O(1) amortized |
//! | Remove | O(1) |
//! | Memory per entry | ~120 bytes overhead + key×2 + value |
//!
//! Higher overhead than simpler algorithms due to priority calculation and
//! BTreeMap-based priority lists.
//!
//! # When to Use GDSF
//!
//! **Good for:**
//! - CDN and proxy caches with variable object sizes
//! - Image thumbnail caches
//! - API response caches with varying payload sizes
//! - File system caches
//! - Any size-constrained cache with heterogeneous objects
//!
//! **Not ideal for:**
//! - Uniform-size objects (simpler algorithms work equally well)
//! - Entry-count-constrained caches (LRU/LFU are simpler)
//! - Very small caches (overhead not justified)
//!
//! # GDSF vs Other Algorithms
//!
//! | Aspect | LRU | LFU | GDSF |
//! |--------|-----|-----|------|
//! | Size-aware | No | No | **Yes** |
//! | Frequency-aware | No | Yes | Yes |
//! | Aging | Implicit | No | Yes |
//! | Best for | Recency | Frequency | Variable-size objects |
//!
//! # Thread Safety
//!
//! `GdsfCache` is **not thread-safe**. For concurrent access, either:
//! - Wrap with `Mutex` or `RwLock`
//! - Use `ConcurrentGdsfCache` (requires `concurrent` feature)
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```
//! use cache_rs::GdsfCache;
//! use core::num::NonZeroUsize;
//!
//! // Create cache with max 1000 entries and 10MB size limit
//! let mut cache: GdsfCache<String, Vec<u8>> = GdsfCache::with_limits(
//!     NonZeroUsize::new(1000).unwrap(),
//!     10 * 1024 * 1024,  // 10MB
//! );
//!
//! // Insert with explicit size tracking
//! let small_data = vec![0u8; 1024];  // 1KB
//! cache.put("small.txt".to_string(), small_data, 1024);
//!
//! let large_data = vec![0u8; 1024 * 1024];  // 1MB
//! cache.put("large.bin".to_string(), large_data, 1024 * 1024);
//!
//! // Small items get higher priority per byte
//! assert!(cache.get(&"small.txt".to_string()).is_some());
//! ```
//!
//! ## CDN-Style Caching
//!
//! ```
//! use cache_rs::GdsfCache;
//! use core::num::NonZeroUsize;
//!
//! // 100MB cache for web assets
//! let mut cache: GdsfCache<String, Vec<u8>> = GdsfCache::with_limits(
//!     NonZeroUsize::new(10000).unwrap(),
//!     100 * 1024 * 1024,
//! );
//!
//! // Cache various asset types with their sizes
//! fn cache_asset(cache: &mut GdsfCache<String, Vec<u8>>, url: &str, data: Vec<u8>) {
//!     let size = data.len() as u64;
//!     cache.put(url.to_string(), data, size);
//! }
//!
//! // Small, frequently-accessed assets get priority over large, rarely-used ones
//! ```

extern crate alloc;

use crate::config::GdsfCacheConfig;
use crate::entry::CacheEntry;
use crate::list::{List, ListEntry};
use crate::meta::GdsfMeta;
use crate::metrics::{CacheMetrics, GdsfCacheMetrics};
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

/// Internal GDSF segment containing the actual cache algorithm.
///
/// Uses `CacheEntry<K, V, GdsfMeta>` as the unified entry type. The map stores
/// raw pointers to list nodes, and all entry data (key, value, size, metadata)
/// is stored in the `CacheEntry`.
pub(crate) struct GdsfSegment<K, V, S = DefaultHashBuilder> {
    config: GdsfCacheConfig,
    global_age: f64,
    min_priority: f64,
    /// Maps keys to node pointers. The node contains CacheEntry with all data.
    map: HashMap<K, *mut ListEntry<CacheEntry<K, V, GdsfMeta>>, S>,
    /// Priority lists: key is (priority * 1000) as u64 for BTreeMap ordering
    priority_lists: BTreeMap<u64, List<CacheEntry<K, V, GdsfMeta>>>,
    metrics: GdsfCacheMetrics,
    /// Current total size of cached content (sum of entry sizes)
    current_size: u64,
}

// SAFETY: GdsfSegment owns all data and raw pointers point only to nodes owned by
// `priority_lists`. Concurrent access is safe when wrapped in proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for GdsfSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for GdsfSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> GdsfSegment<K, V, S> {
    pub(crate) fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self::with_hasher_and_size(cap, hash_builder, u64::MAX)
    }

    pub(crate) fn with_hasher_and_size(cap: NonZeroUsize, hash_builder: S, max_size: u64) -> Self {
        let config = GdsfCacheConfig::new(cap).with_max_size(max_size);
        let map_capacity = config.capacity().get().next_power_of_two();

        GdsfSegment {
            global_age: config.initial_age(),
            min_priority: 0.0,
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            priority_lists: BTreeMap::new(),
            metrics: GdsfCacheMetrics::new(max_size),
            current_size: 0,
            config,
        }
    }

    #[inline]
    pub(crate) fn cap(&self) -> NonZeroUsize {
        self.config.capacity()
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }

    #[inline]
    pub(crate) fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    #[inline]
    pub(crate) fn global_age(&self) -> f64 {
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

    #[inline]
    pub(crate) fn metrics(&self) -> &GdsfCacheMetrics {
        &self.metrics
    }

    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    #[inline]
    pub(crate) fn record_miss(&mut self, object_size: u64) {
        self.metrics.core.record_miss(object_size);
    }

    fn calculate_priority(&self, frequency: u64, size: u64) -> f64 {
        if size == 0 {
            return f64::INFINITY;
        }
        (frequency as f64 / size as f64) + self.global_age
    }

    unsafe fn update_priority_by_node(
        &mut self,
        node: *mut ListEntry<CacheEntry<K, V, GdsfMeta>>,
    ) -> *mut ListEntry<CacheEntry<K, V, GdsfMeta>>
    where
        K: Clone + Hash + Eq,
    {
        // SAFETY: node is guaranteed valid by caller's contract
        let entry = (*node).get_value_mut();
        let key_cloned = entry.key.clone();
        let size = entry.size;
        let meta = entry.metadata_mut().unwrap();
        let old_priority = meta.priority;

        meta.increment();

        let global_age = self.global_age;
        let new_priority = meta.calculate_priority(size, global_age);

        let old_priority_key = (old_priority * 1000.0) as u64;
        let new_priority_key = (new_priority * 1000.0) as u64;

        if old_priority_key == new_priority_key {
            self.priority_lists
                .get_mut(&new_priority_key)
                .unwrap()
                .move_to_front(node);
            return node;
        }

        let boxed_entry = self
            .priority_lists
            .get_mut(&old_priority_key)
            .unwrap()
            .remove(node)
            .unwrap();

        if self
            .priority_lists
            .get(&old_priority_key)
            .unwrap()
            .is_empty()
        {
            self.priority_lists.remove(&old_priority_key);
        }

        let entry_ptr = Box::into_raw(boxed_entry);

        let capacity = self.config.capacity();
        self.priority_lists
            .entry(new_priority_key)
            .or_insert_with(|| List::new(capacity));

        self.priority_lists
            .get_mut(&new_priority_key)
            .unwrap()
            .attach_from_other_list(entry_ptr);

        // Update map with new node pointer
        self.map.insert(key_cloned, entry_ptr);
        entry_ptr
    }

    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(&node) = self.map.get(key) {
            unsafe {
                // SAFETY: node comes from our map
                let entry = (*node).get_value();
                let object_size = self.estimate_object_size(&entry.key, &entry.value);
                let meta = entry.metadata.as_ref().unwrap();
                self.metrics.core.record_hit(object_size);
                self.metrics
                    .record_item_access(meta.frequency, entry.size, meta.priority);

                let new_node = self.update_priority_by_node(node);
                let value = (*new_node).get_value().value.clone();
                Some(value)
            }
        } else {
            None
        }
    }

    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(&node) = self.map.get(key) {
            unsafe {
                // SAFETY: node comes from our map
                let entry = (*node).get_value();
                let object_size = self.estimate_object_size(&entry.key, &entry.value);
                let meta = entry.metadata.as_ref().unwrap();
                self.metrics.core.record_hit(object_size);
                self.metrics
                    .record_item_access(meta.frequency, entry.size, meta.priority);

                let new_node = self.update_priority_by_node(node);
                let entry_mut = (*new_node).get_value_mut();
                Some(&mut entry_mut.value)
            }
        } else {
            None
        }
    }

    pub(crate) fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.contains_key(key)
    }

    pub(crate) fn put(&mut self, key: K, val: V, size: u64) -> Option<V>
    where
        K: Clone,
    {
        if size == 0 {
            return None;
        }

        let object_size = self.estimate_object_size(&key, &val);

        // Check if key exists - update existing entry
        if let Some(&node) = self.map.get(&key) {
            unsafe {
                // SAFETY: node comes from our map
                let entry = (*node).get_value_mut();
                let old_size = entry.size;
                let meta = entry.metadata_mut().unwrap();
                let old_priority_key = (meta.priority * 1000.0) as u64;
                let frequency = meta.frequency;

                // Remove from old priority list
                let list = self.priority_lists.get_mut(&old_priority_key).unwrap();
                let boxed_entry = list.remove(node).unwrap();

                if list.is_empty() {
                    self.priority_lists.remove(&old_priority_key);
                }

                let entry_ptr = Box::into_raw(boxed_entry);
                let old_value = (*entry_ptr).get_value().value.clone();
                let _ = Box::from_raw(entry_ptr);

                // Update size tracking
                self.current_size = self.current_size.saturating_sub(old_size);
                self.current_size += size;

                // Create new entry with updated values but preserved frequency
                let new_priority = self.calculate_priority(frequency, size);
                let new_priority_key = (new_priority * 1000.0) as u64;

                let new_entry = CacheEntry::with_metadata(
                    key.clone(),
                    val,
                    size,
                    GdsfMeta::new(frequency, new_priority),
                );

                let capacity = self.cap();
                let list = self
                    .priority_lists
                    .entry(new_priority_key)
                    .or_insert_with(|| List::new(capacity));

                if let Some(new_node) = list.add(new_entry) {
                    self.map.insert(key, new_node);
                    self.metrics.core.record_insertion(object_size);
                    return Some(old_value);
                } else {
                    self.map.remove(&key);
                    return None;
                }
            }
        }

        // New entry - check capacity and size limits
        let capacity = self.config.capacity().get();
        let max_size = self.config.max_size();

        while self.len() >= capacity
            || (self.current_size + size > max_size && !self.map.is_empty())
        {
            self.evict_one();
        }

        let priority = self.calculate_priority(1, size);
        let priority_key = (priority * 1000.0) as u64;

        let cap = self.config.capacity();
        let list = self
            .priority_lists
            .entry(priority_key)
            .or_insert_with(|| List::new(cap));

        let cache_entry =
            CacheEntry::with_metadata(key.clone(), val, size, GdsfMeta::new(1, priority));

        if let Some(node) = list.add(cache_entry) {
            self.map.insert(key, node);
            self.current_size += size;

            if self.len() == 1 || priority < self.min_priority {
                self.min_priority = priority;
            }

            self.metrics.core.record_insertion(size);
            self.metrics
                .record_item_cached(size, self.metrics.average_item_size());
            self.metrics.record_item_access(1, size, priority);

            None
        } else {
            None
        }
    }

    fn evict_one(&mut self) {
        if self.is_empty() {
            return;
        }

        let min_priority_key = *self.priority_lists.keys().next().unwrap();
        let list = self.priority_lists.get_mut(&min_priority_key).unwrap();

        if let Some(boxed_entry) = list.remove_last() {
            unsafe {
                // SAFETY: entry comes from list.remove_last()
                let entry_ptr = Box::into_raw(boxed_entry);
                let entry = (*entry_ptr).get_value();
                let evicted_size = entry.size;
                let priority_to_update = entry
                    .metadata
                    .as_ref()
                    .map(|m| m.priority)
                    .unwrap_or(self.global_age);

                self.current_size = self.current_size.saturating_sub(evicted_size);
                self.metrics.core.record_eviction(evicted_size);
                self.metrics.record_size_based_eviction();
                self.metrics.record_aging_event(priority_to_update);

                self.global_age = priority_to_update;
                self.map.remove(&entry.key);

                let _ = Box::from_raw(entry_ptr);
            }
        }

        if list.is_empty() {
            self.priority_lists.remove(&min_priority_key);
        }
    }

    pub(crate) fn pop<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(node) = self.map.remove(key) {
            unsafe {
                // SAFETY: node comes from our map
                let entry = (*node).get_value();
                let removed_size = entry.size;
                let priority = entry.metadata.as_ref().map(|m| m.priority).unwrap_or(0.0);
                let priority_key = (priority * 1000.0) as u64;

                let list = self.priority_lists.get_mut(&priority_key).unwrap();
                let boxed_entry = list.remove(node).unwrap();

                if list.is_empty() {
                    self.priority_lists.remove(&priority_key);
                }

                let entry_ptr = Box::into_raw(boxed_entry);
                let result = (*entry_ptr).get_value().value.clone();
                self.current_size = self.current_size.saturating_sub(removed_size);
                let _ = Box::from_raw(entry_ptr);

                Some(result)
            }
        } else {
            None
        }
    }

    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.priority_lists.clear();
        self.global_age = 0.0;
        self.min_priority = 0.0;
        self.current_size = 0;
    }
}

impl<K, V, S> core::fmt::Debug for GdsfSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GdsfSegment")
            .field("capacity", &self.config.capacity())
            .field("len", &self.map.len())
            .field("global_age", &self.global_age)
            .finish()
    }
}

/// An implementation of a Greedy Dual-Size Frequency (GDSF) cache.
#[derive(Debug)]
pub struct GdsfCache<K, V, S = DefaultHashBuilder> {
    segment: GdsfSegment<K, V, S>,
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> GdsfCache<K, V, S> {
    pub fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self {
            segment: GdsfSegment::with_hasher(cap, hash_builder),
        }
    }

    /// Creates a new GDSF cache with the specified capacity, hash builder, and max size.
    pub fn with_hasher_and_size(cap: NonZeroUsize, hash_builder: S, max_size: u64) -> Self {
        Self {
            segment: GdsfSegment::with_hasher_and_size(cap, hash_builder, max_size),
        }
    }

    #[inline]
    pub fn cap(&self) -> NonZeroUsize {
        self.segment.cap()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.segment.len()
    }

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

    #[inline]
    pub fn global_age(&self) -> f64 {
        self.segment.global_age()
    }

    #[inline]
    pub fn record_miss(&mut self, object_size: u64) {
        self.segment.record_miss(object_size);
    }

    #[inline]
    pub fn get<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get(key)
    }

    #[inline]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get_mut(key)
    }

    #[inline]
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.contains_key(key)
    }

    #[inline]
    pub fn put(&mut self, key: K, val: V, size: u64) -> Option<V>
    where
        K: Clone,
    {
        self.segment.put(key, val, size)
    }

    #[inline]
    pub fn pop<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.pop(key)
    }

    #[inline]
    pub fn clear(&mut self) {
        self.segment.clear()
    }
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> CacheMetrics for GdsfCache<K, V, S> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.segment.metrics().metrics()
    }

    fn algorithm_name(&self) -> &'static str {
        self.segment.metrics().algorithm_name()
    }
}

impl<K: Hash + Eq, V: Clone> GdsfCache<K, V, DefaultHashBuilder> {
    /// Creates a new GDSF cache from a configuration.
    ///
    /// This is the **recommended** way to create a GDSF cache. All configuration
    /// is specified through the [`GdsfCacheConfig`] struct, which uses a builder
    /// pattern for optional parameters.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity and optional size limit/initial age
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::GdsfCache;
    /// use cache_rs::config::GdsfCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// // Simple capacity-only cache
    /// let config = GdsfCacheConfig::new(NonZeroUsize::new(100).unwrap());
    /// let mut cache: GdsfCache<&str, i32> = GdsfCache::from_config(config);
    /// cache.put("key", 42, 1);
    ///
    /// // Cache with size limit (recommended for GDSF)
    /// let config = GdsfCacheConfig::new(NonZeroUsize::new(1000).unwrap())
    ///     .with_max_size(10 * 1024 * 1024);  // 10MB
    /// let cache: GdsfCache<String, Vec<u8>> = GdsfCache::from_config(config);
    /// ```
    pub fn from_config(config: GdsfCacheConfig) -> Self {
        Self::with_hasher_and_size(
            config.capacity(),
            DefaultHashBuilder::default(),
            config.max_size(),
        )
    }

    /// Creates a new GDSF cache with the specified capacity.
    ///
    /// This is a convenience constructor. For more control, use [`GdsfCache::from_config`].
    ///
    /// # Arguments
    ///
    /// * `cap` - The maximum number of entries the cache can hold.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::GdsfCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = GdsfCache::new(NonZeroUsize::new(100).unwrap());
    /// cache.put("key", 42, 1);
    /// ```
    pub fn new(cap: NonZeroUsize) -> Self {
        Self::from_config(GdsfCacheConfig::new(cap))
    }

    /// Creates a new GDSF cache with both entry count and size limits.
    ///
    /// This is a convenience constructor. For more control, use [`GdsfCache::from_config`].
    ///
    /// # Arguments
    ///
    /// * `cap` - The maximum number of entries the cache can hold.
    /// * `max_size` - The maximum total size in bytes (0 means unlimited).
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::GdsfCache;
    /// use core::num::NonZeroUsize;
    ///
    /// // Cache with 1000 entries and 10MB size limit
    /// let mut cache: GdsfCache<String, Vec<u8>> = GdsfCache::with_limits(
    ///     NonZeroUsize::new(1000).unwrap(),
    ///     10 * 1024 * 1024,
    /// );
    /// ```
    pub fn with_limits(cap: NonZeroUsize, max_size: u64) -> Self {
        Self::from_config(GdsfCacheConfig::new(cap).with_max_size(max_size))
    }

    /// Creates a new GDSF cache with only a size limit.
    ///
    /// This is a convenience constructor that creates a cache limited by total size
    /// with a very high entry count limit.
    ///
    /// # Arguments
    ///
    /// * `max_size` - The maximum total size in bytes.
    pub fn with_max_size(max_size: u64) -> Self {
        // Use a large but reasonable capacity that won't overflow hash tables
        const MAX_REASONABLE_CAPACITY: usize = 1 << 30; // ~1 billion entries
        Self::from_config(
            GdsfCacheConfig::new(NonZeroUsize::new(MAX_REASONABLE_CAPACITY).unwrap())
                .with_max_size(max_size),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroUsize;

    #[test]
    fn test_gdsf_basic_operations() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(3).unwrap());

        assert_eq!(cache.put("a", 1, 1), None);
        assert_eq!(cache.put("b", 2, 2), None);
        assert_eq!(cache.put("c", 3, 1), None);
        assert_eq!(cache.len(), 3);

        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));

        assert!(cache.contains_key(&"a"));
        assert!(!cache.contains_key(&"d"));
    }

    #[test]
    fn test_gdsf_frequency_priority() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1, 1);
        cache.put("b", 2, 1);

        cache.get(&"a");
        cache.get(&"a");

        cache.put("c", 3, 1);

        assert!(cache.contains_key(&"a"));
        assert!(!cache.contains_key(&"b"));
        assert!(cache.contains_key(&"c"));
    }

    #[test]
    fn test_gdsf_size_consideration() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("small", 1, 1);
        cache.put("large", 2, 10);

        cache.put("medium", 3, 5);

        assert!(cache.contains_key(&"small"));
        assert!(!cache.contains_key(&"large"));
        assert!(cache.contains_key(&"medium"));
    }

    #[test]
    fn test_gdsf_update_existing() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("key", 1, 1);
        assert_eq!(cache.get(&"key"), Some(1));

        assert_eq!(cache.put("key", 2, 2), Some(1));
        assert_eq!(cache.get(&"key"), Some(2));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_gdsf_zero_size_rejection() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        assert_eq!(cache.put("key", 1, 0), None);
        assert_eq!(cache.len(), 0);
        assert!(!cache.contains_key(&"key"));
    }

    #[test]
    fn test_gdsf_pop() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1, 1);
        cache.put("b", 2, 1);

        assert_eq!(cache.pop(&"a"), Some(1));
        assert_eq!(cache.len(), 1);
        assert!(!cache.contains_key(&"a"));
        assert!(cache.contains_key(&"b"));

        assert_eq!(cache.pop(&"nonexistent"), None);
    }

    #[test]
    fn test_gdsf_clear() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1, 1);
        cache.put("b", 2, 1);
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
        assert!(!cache.contains_key(&"a"));
        assert!(!cache.contains_key(&"b"));
    }

    #[test]
    fn test_gdsf_global_aging() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1, 1);
        cache.put("b", 2, 1);

        let initial_age = cache.global_age();

        cache.put("c", 3, 1);

        assert!(cache.global_age() > initial_age);
    }

    #[test]
    fn test_miri_stacked_borrows_fix() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(10).unwrap());

        cache.put("a", 1, 10);
        cache.put("b", 2, 20);
        cache.put("c", 3, 15);

        for _ in 0..3 {
            assert_eq!(cache.get(&"a"), Some(1));
            assert_eq!(cache.get(&"b"), Some(2));
            assert_eq!(cache.get(&"c"), Some(3));
        }

        assert_eq!(cache.len(), 3);

        if let Some(val) = cache.get_mut(&"a") {
            *val += 10;
        }
        assert_eq!(cache.get(&"a"), Some(11));
    }

    #[test]
    fn test_gdsf_segment_directly() {
        let mut segment: GdsfSegment<&str, i32, DefaultHashBuilder> =
            GdsfSegment::with_hasher(NonZeroUsize::new(2).unwrap(), DefaultHashBuilder::default());
        assert_eq!(segment.len(), 0);
        assert!(segment.is_empty());
        assert_eq!(segment.cap().get(), 2);
        segment.put("a", 1, 1);
        segment.put("b", 2, 2);
        assert_eq!(segment.len(), 2);
        assert_eq!(segment.get(&"a"), Some(1));
        assert_eq!(segment.get(&"b"), Some(2));
    }

    #[test]
    fn test_gdsf_concurrent_access() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(GdsfCache::new(NonZeroUsize::new(100).unwrap())));
        let num_threads = 4;
        let ops_per_thread = 100;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}_{}", t, i);
                    let size = ((i % 10) + 1) as u64; // Varying sizes 1-10
                    let mut guard = cache.lock().unwrap();
                    guard.put(key.clone(), i, size);
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
    fn test_gdsf_size_aware_tracking() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(10).unwrap());

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), u64::MAX);

        // GDSF already requires size in put()
        cache.put("a", 1, 100);
        cache.put("b", 2, 200);
        cache.put("c", 3, 150);

        assert_eq!(cache.current_size(), 450);
        assert_eq!(cache.len(), 3);

        // GDSF doesn't have remove method, test clear instead
        cache.clear();
        assert_eq!(cache.current_size(), 0);
    }

    #[test]
    fn test_gdsf_from_config_constructor() {
        let config =
            GdsfCacheConfig::new(NonZeroUsize::new(1000).unwrap()).with_max_size(1024 * 1024);
        let cache: GdsfCache<String, i32> = GdsfCache::from_config(config);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
    }

    #[test]
    fn test_gdsf_with_limits_constructor() {
        let cache: GdsfCache<String, String> =
            GdsfCache::with_limits(NonZeroUsize::new(100).unwrap(), 1024 * 1024);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
        assert_eq!(cache.cap().get(), 100);
    }
}
