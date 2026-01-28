//! Segmented Least Recently Used (SLRU) Cache Implementation
//!
//! SLRU is a scan-resistant cache algorithm that divides the cache into two segments:
//! a **probationary segment** for new entries and a **protected segment** for frequently
//! accessed entries. This design prevents one-time access patterns (scans) from evicting
//! valuable cached items.
//!
//! # How the Algorithm Works
//!
//! SLRU uses a two-tier approach to distinguish between items that are accessed once
//! (scans, sequential reads) versus items accessed repeatedly (working set).
//!
//! ## Segment Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                              SLRU Cache                                      │
//! │                                                                              │
//! │  ┌─────────────────────────────────────────────────────────────────────┐    │
//! │  │                    PROTECTED SEGMENT (20%)                          │    │
//! │  │          Frequently accessed items - harder to evict                │    │
//! │  │  ┌─────────────────────────────────────────────────────────────┐   │    │
//! │  │  │ MRU ◀──▶ [hot_1] ◀──▶ [hot_2] ◀──▶ ... ◀──▶ [demote] LRU  │   │    │
//! │  │  └─────────────────────────────────────────────────────────────┘   │    │
//! │  └─────────────────────────────────────────────────────────────────────┘    │
//! │                              │ demote                   ▲ promote           │
//! │                              ▼                          │                   │
//! │  ┌─────────────────────────────────────────────────────────────────────┐    │
//! │  │                   PROBATIONARY SEGMENT (80%)                        │    │
//! │  │          New items and demoted items - easier to evict              │    │
//! │  │  ┌─────────────────────────────────────────────────────────────┐   │    │
//! │  │  │ MRU ◀──▶ [new_1] ◀──▶ [new_2] ◀──▶ ... ◀──▶ [evict] LRU   │   │    │
//! │  │  └─────────────────────────────────────────────────────────────┘   │    │
//! │  └─────────────────────────────────────────────────────────────────────┘    │
//! │                              ▲                                              │
//! │                              │ insert                                       │
//! │                         new items                                           │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Entry Lifecycle
//!
//! 1. **Insert**: New items enter the probationary segment
//! 2. **First access in probationary**: Item promoted to protected segment
//! 3. **Protected segment full**: LRU item demoted back to probationary
//! 4. **Eviction**: Always from the LRU end of the probationary segment
//!
//! ## Scan Resistance Example
//!
//! ```text
//! Initial state: Protected=[A, B, C], Probationary=[D, E, F]
//! (A, B, C are hot items; D, E, F are warm items)
//!
//! Sequential scan of X, Y, Z (one-time access):
//!   put(X) → Protected=[A, B, C], Probationary=[X, D, E]  (F evicted)
//!   put(Y) → Protected=[A, B, C], Probationary=[Y, X, D]  (E evicted)
//!   put(Z) → Protected=[A, B, C], Probationary=[Z, Y, X]  (D evicted)
//!
//! Hot items A, B, C remain in protected segment!
//! The scan only displaced probationary items.
//! ```
//!
//! ## Operations
//!
//! | Operation | Action | Time |
//! |-----------|--------|------|
//! | `get(key)` | Promote to protected if in probationary | O(1) |
//! | `put(key, value)` | Insert to probationary, may evict | O(1) |
//! | `remove(key)` | Remove from whichever segment contains it | O(1) |
//!
//! # Dual-Limit Capacity
//!
//! This implementation supports two independent limits:
//!
//! - **`max_entries`**: Maximum total items across both segments
//! - **`max_size`**: Maximum total size of content
//!
//! The protected segment size is configured separately (default: 20% of total).
//!
//! # Performance Characteristics
//!
//! | Metric | Value |
//! |--------|-------|
//! | Get | O(1) |
//! | Put | O(1) |
//! | Remove | O(1) |
//! | Memory per entry | ~90 bytes overhead + key×2 + value |
//!
//! Memory overhead includes: two list pointers, location tag, size tracking,
//! HashMap bucket, and allocator overhead.
//!
//! # When to Use SLRU
//!
//! **Good for:**
//! - Mixed workloads with both hot data and sequential scans
//! - Database buffer pools
//! - File system caches
//! - Any scenario where scans shouldn't evict the working set
//!
//! **Not ideal for:**
//! - Pure recency-based access patterns (LRU is simpler)
//! - Frequency-dominant patterns (LFU/LFUDA is better)
//! - Very small caches where the two-segment overhead isn't justified
//!
//! # Tuning the Protected Ratio
//!
//! The protected segment size controls the trade-off:
//! - **Larger protected**: More scan resistance, but new items evicted faster
//! - **Smaller protected**: Less scan resistance, but more room for new items
//!
//! Default is 20% protected, which works well for most workloads.
//!
//! # Thread Safety
//!
//! `SlruCache` is **not thread-safe**. For concurrent access, either:
//! - Wrap with `Mutex` or `RwLock`
//! - Use `ConcurrentSlruCache` (requires `concurrent` feature)
//!
//! # Examples
//!
//! ## Basic Usage
//!
//! ```
//! use cache_rs::SlruCache;
//! use core::num::NonZeroUsize;
//!
//! // Total capacity 100, protected segment 20
//! let mut cache = SlruCache::new(
//!     NonZeroUsize::new(100).unwrap(),
//!     NonZeroUsize::new(20).unwrap(),
//! );
//!
//! cache.put("a", 1);  // Enters probationary
//! cache.get(&"a");    // Promoted to protected!
//! cache.put("b", 2);  // Enters probationary
//!
//! assert_eq!(cache.get(&"a"), Some(&1));  // Still in protected
//! ```
//!
//! ## Scan Resistance Demo
//!
//! ```
//! use cache_rs::SlruCache;
//! use core::num::NonZeroUsize;
//!
//! let mut cache: SlruCache<i32, i32> = SlruCache::new(
//!     NonZeroUsize::new(10).unwrap(),
//!     NonZeroUsize::new(3).unwrap(),
//! );
//!
//! // Establish hot items in protected segment
//! for key in [1, 2, 3] {
//!     cache.put(key, 100);
//!     cache.get(&key);  // Promote to protected
//! }
//!
//! // Simulate a scan - these items only enter probationary
//! for i in 100..120 {
//!     cache.put(i, i);  // One-time insertions
//! }
//!
//! // Hot items survive the scan!
//! assert!(cache.get(&1).is_some());
//! assert!(cache.get(&2).is_some());
//! assert!(cache.get(&3).is_some());
//! ```

extern crate alloc;

use crate::config::SlruCacheConfig;
use crate::list::{List, ListEntry};
use crate::metrics::{CacheMetrics, SlruCacheMetrics};
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

/// Entry location within the SLRU cache
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Location {
    /// Entry is in the probationary segment
    Probationary,
    /// Entry is in the protected segment
    Protected,
}

/// Internal SLRU segment containing the actual cache algorithm.
///
/// This is shared between `SlruCache` (single-threaded) and
/// `ConcurrentSlruCache` (multi-threaded). All algorithm logic is
/// implemented here to avoid code duplication.
///
/// # Safety
///
/// This struct contains raw pointers in the `map` field. These pointers
/// are always valid as long as:
/// - The pointer was obtained from `probationary.add()` or `protected.add()`
/// - The node has not been removed from the list
/// - The segment has not been dropped
pub(crate) struct SlruSegment<K, V, S = DefaultHashBuilder> {
    /// Configuration for the SLRU cache
    config: SlruCacheConfig,

    /// The probationary list holding newer or less frequently accessed items
    probationary: List<(K, V)>,

    /// The protected list holding frequently accessed items
    protected: List<(K, V)>,

    /// A hash map mapping keys to entries in either the probationary or protected list
    /// The tuple stores (node pointer, segment location, entry size in bytes)
    #[allow(clippy::type_complexity)]
    map: HashMap<K, (*mut ListEntry<(K, V)>, Location, u64), S>,

    /// Metrics for tracking cache performance and segment behavior
    metrics: SlruCacheMetrics,

    /// Current total size of cached content (sum of entry sizes)
    current_size: u64,

    /// Maximum content size the cache can hold
    max_size: u64,
}

// SAFETY: SlruSegment owns all data and raw pointers point only to nodes owned by
// `probationary` or `protected` lists. Concurrent access is safe when wrapped in
// proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for SlruSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for SlruSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> SlruSegment<K, V, S> {
    /// Creates a new SLRU segment with the specified capacity and hash builder.
    pub(crate) fn with_hasher(
        total_cap: NonZeroUsize,
        protected_cap: NonZeroUsize,
        hash_builder: S,
    ) -> Self {
        Self::with_hasher_and_size(total_cap, protected_cap, hash_builder, u64::MAX)
    }

    /// Creates a new SLRU segment with the specified capacity, hash builder, and max size.
    pub(crate) fn with_hasher_and_size(
        total_cap: NonZeroUsize,
        protected_cap: NonZeroUsize,
        hash_builder: S,
        max_size: u64,
    ) -> Self {
        let config = SlruCacheConfig::new(total_cap, protected_cap);

        let probationary_max_size =
            NonZeroUsize::new(config.capacity().get() - config.protected_capacity().get()).unwrap();

        SlruSegment {
            config,
            probationary: List::new(probationary_max_size),
            protected: List::new(config.protected_capacity()),
            map: HashMap::with_capacity_and_hasher(
                config.capacity().get().next_power_of_two(),
                hash_builder,
            ),
            metrics: SlruCacheMetrics::new(max_size, config.protected_capacity().get() as u64),
            current_size: 0,
            max_size,
        }
    }

    /// Returns the maximum number of key-value pairs the segment can hold.
    #[inline]
    pub(crate) fn cap(&self) -> NonZeroUsize {
        self.config.capacity()
    }

    /// Returns the maximum size of the protected segment.
    #[inline]
    pub(crate) fn protected_max_size(&self) -> NonZeroUsize {
        self.config.protected_capacity()
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
        self.max_size
    }

    /// Estimates the size of a key-value pair in bytes for metrics tracking
    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    /// Returns a reference to the metrics for this segment.
    #[inline]
    pub(crate) fn metrics(&self) -> &SlruCacheMetrics {
        &self.metrics
    }

    /// Moves an entry from the probationary segment to the protected segment.
    /// If the protected segment is full, the LRU item from protected is demoted to probationary.
    ///
    /// Returns a raw pointer to the entry in its new location.
    unsafe fn promote_to_protected(
        &mut self,
        node: *mut ListEntry<(K, V)>,
    ) -> *mut ListEntry<(K, V)> {
        // Remove from probationary list
        let boxed_entry = self
            .probationary
            .remove(node)
            .expect("Node should exist in probationary");

        // If protected segment is full, demote LRU protected item to probationary
        if self.protected.len() >= self.protected_max_size().get() {
            // We need to make room in probationary first if it's full
            if self.probationary.len() >= self.probationary.cap().get() {
                // Evict LRU from probationary
                if let Some(old_entry) = self.probationary.remove_last() {
                    let old_ptr = Box::into_raw(old_entry);
                    let (old_key, _) = (*old_ptr).get_value();
                    // Get stored size and update current_size
                    if let Some((_, _, evicted_size)) = self.map.remove(old_key) {
                        self.current_size = self.current_size.saturating_sub(evicted_size);
                        self.metrics.record_probationary_eviction(evicted_size);
                    }
                    let _ = Box::from_raw(old_ptr);
                }
            }
            self.demote_lru_protected();
        }

        // Get the raw pointer from the box
        let entry_ptr = Box::into_raw(boxed_entry);

        // Get the key from the entry for updating the map
        let (key_ref, _) = (*entry_ptr).get_value();

        // Update the map with new location and pointer (preserve size)
        if let Some(map_entry) = self.map.get_mut(key_ref) {
            map_entry.0 = entry_ptr;
            map_entry.1 = Location::Protected;
        }

        // Add to protected list using the pointer from the Box
        unsafe {
            self.protected.attach_from_other_list(entry_ptr);
        }

        entry_ptr
    }

    /// Demotes the least recently used item from the protected segment to the probationary segment.
    unsafe fn demote_lru_protected(&mut self) {
        if let Some(lru_protected) = self.protected.remove_last() {
            let lru_ptr = Box::into_raw(lru_protected);
            let (lru_key, _) = (*lru_ptr).get_value();

            // Update the location and pointer in the map (preserve size)
            if let Some(entry) = self.map.get_mut(lru_key) {
                entry.0 = lru_ptr;
                entry.1 = Location::Probationary;
            }

            // Add to probationary list
            self.probationary.attach_from_other_list(lru_ptr);

            // Record demotion
            self.metrics.record_demotion();
        }
    }

    /// Returns a reference to the value corresponding to the key.
    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (node, location, size) = self.map.get(key).copied()?;

        match location {
            Location::Probationary => unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our probationary list
                self.metrics.record_probationary_hit(size);

                // Promote from probationary to protected
                let entry_ptr = self.promote_to_protected(node);

                // Record promotion
                self.metrics.record_promotion();

                // Update segment sizes
                self.metrics.update_segment_sizes(
                    self.probationary.len() as u64,
                    self.protected.len() as u64,
                );

                // SAFETY: entry_ptr is the return value from promote_to_protected
                let (_, v) = (*entry_ptr).get_value();
                Some(v)
            },
            Location::Protected => unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our protected list
                self.metrics.record_protected_hit(size);

                // Already protected, just move to MRU position
                self.protected.move_to_front(node);
                let (_, v) = (*node).get_value();
                Some(v)
            },
        }
    }

    /// Returns a mutable reference to the value corresponding to the key.
    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (node, location, size) = self.map.get(key).copied()?;

        match location {
            Location::Probationary => unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our probationary list
                self.metrics.record_probationary_hit(size);

                // Promote from probationary to protected
                let entry_ptr = self.promote_to_protected(node);

                // Record promotion
                self.metrics.record_promotion();

                // Update segment sizes
                self.metrics.update_segment_sizes(
                    self.probationary.len() as u64,
                    self.protected.len() as u64,
                );

                // SAFETY: entry_ptr is the return value from promote_to_protected
                let (_, v) = (*entry_ptr).get_value_mut();
                Some(v)
            },
            Location::Protected => unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our protected list
                self.metrics.record_protected_hit(size);

                // Already protected, just move to MRU position
                self.protected.move_to_front(node);
                // SAFETY: node is still valid after move_to_front operation
                let (_, v) = (*node).get_value_mut();
                Some(v)
            },
        }
    }

    /// Records a cache miss for metrics tracking
    #[inline]
    pub(crate) fn record_miss(&mut self, object_size: u64) {
        self.metrics.core.record_miss(object_size);
    }
}

impl<K: Hash + Eq + Clone, V, S: BuildHasher> SlruSegment<K, V, S> {
    /// Inserts a key-value pair into the segment.
    pub(crate) fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        V: Clone,
    {
        let object_size = self.estimate_object_size(&key, &value);
        self.put_with_size(key, value, object_size)
    }

    /// Insert a key-value pair with explicit size tracking.
    pub(crate) fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>
    where
        V: Clone,
    {
        // If key is already in the cache, update it in place
        if let Some(&(node, location, old_size)) = self.map.get(&key) {
            match location {
                Location::Probationary => unsafe {
                    // SAFETY: node comes from our map
                    self.probationary.move_to_front(node);
                    let old_entry = self.probationary.update(node, (key.clone(), value), true);
                    // Update size tracking: subtract old size, add new size
                    self.current_size = self.current_size.saturating_sub(old_size);
                    self.current_size += size;
                    // Update the size in the map
                    if let Some(map_entry) = self.map.get_mut(&key) {
                        map_entry.2 = size;
                    }
                    self.metrics.core.record_insertion(size);
                    return old_entry.0;
                },
                Location::Protected => unsafe {
                    // SAFETY: node comes from our map
                    self.protected.move_to_front(node);
                    let old_entry = self.protected.update(node, (key.clone(), value), true);
                    // Update size tracking: subtract old size, add new size
                    self.current_size = self.current_size.saturating_sub(old_size);
                    self.current_size += size;
                    // Update the size in the map
                    if let Some(map_entry) = self.map.get_mut(&key) {
                        map_entry.2 = size;
                    }
                    self.metrics.core.record_insertion(size);
                    return old_entry.0;
                },
            }
        }

        let mut evicted = None;

        // Check if total cache is at capacity
        if self.len() >= self.cap().get() {
            // If probationary segment has items, evict from there first
            if !self.probationary.is_empty() {
                if let Some(old_entry) = self.probationary.remove_last() {
                    unsafe {
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();
                        // Get the stored size from the map before removing
                        let evicted_size = self
                            .map
                            .get(&old_key)
                            .map(|(_, _, sz)| *sz)
                            .unwrap_or_else(|| self.estimate_object_size(&old_key, &old_value));
                        self.current_size = self.current_size.saturating_sub(evicted_size);
                        self.metrics.record_probationary_eviction(evicted_size);
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));
                        let _ = Box::from_raw(entry_ptr);
                    }
                }
            } else if !self.protected.is_empty() {
                // If probationary is empty, evict from protected
                if let Some(old_entry) = self.protected.remove_last() {
                    unsafe {
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();
                        // Get the stored size from the map before removing
                        let evicted_size = self
                            .map
                            .get(&old_key)
                            .map(|(_, _, sz)| *sz)
                            .unwrap_or_else(|| self.estimate_object_size(&old_key, &old_value));
                        self.current_size = self.current_size.saturating_sub(evicted_size);
                        self.metrics.record_protected_eviction(evicted_size);
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));
                        let _ = Box::from_raw(entry_ptr);
                    }
                }
            }
        }

        // Add the new key-value pair to the probationary segment
        if self.len() < self.cap().get() {
            // Total cache has space, allow probationary to exceed its capacity
            let node = self.probationary.add_unchecked((key.clone(), value));
            self.map.insert(key, (node, Location::Probationary, size));
            self.current_size += size;
        } else {
            // Total cache is full, try to add normally (may fail due to probationary capacity)
            if let Some(node) = self.probationary.add((key.clone(), value.clone())) {
                self.map.insert(key, (node, Location::Probationary, size));
                self.current_size += size;
            } else {
                // Probationary is at capacity, need to make space
                if let Some(old_entry) = self.probationary.remove_last() {
                    unsafe {
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();
                        // Get the stored size from the map before removing
                        let evicted_size = self
                            .map
                            .get(&old_key)
                            .map(|(_, _, sz)| *sz)
                            .unwrap_or_else(|| self.estimate_object_size(&old_key, &old_value));
                        self.current_size = self.current_size.saturating_sub(evicted_size);
                        self.metrics.record_probationary_eviction(evicted_size);
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));
                        let _ = Box::from_raw(entry_ptr);
                    }
                }

                // Try again after making space
                if let Some(node) = self.probationary.add((key.clone(), value)) {
                    self.map.insert(key, (node, Location::Probationary, size));
                    self.current_size += size;
                }
            }
        }

        // Record insertion and update segment sizes
        self.metrics.core.record_insertion(size);
        self.metrics
            .update_segment_sizes(self.probationary.len() as u64, self.protected.len() as u64);

        evicted
    }

    /// Removes a key from the segment, returning the value if the key was present.
    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        let (node, location, removed_size) = self.map.remove(key)?;

        match location {
            Location::Probationary => unsafe {
                // SAFETY: node comes from our map and was just removed
                let boxed_entry = self.probationary.remove(node)?;
                let entry_ptr = Box::into_raw(boxed_entry);
                let (_, v_ref) = (*entry_ptr).get_value();
                let value = v_ref.clone();
                self.current_size = self.current_size.saturating_sub(removed_size);
                let _ = Box::from_raw(entry_ptr);
                Some(value)
            },
            Location::Protected => unsafe {
                // SAFETY: node comes from our map and was just removed
                let boxed_entry = self.protected.remove(node)?;
                let entry_ptr = Box::into_raw(boxed_entry);
                let (_, v_ref) = (*entry_ptr).get_value();
                let value = v_ref.clone();
                self.current_size = self.current_size.saturating_sub(removed_size);
                let _ = Box::from_raw(entry_ptr);
                Some(value)
            },
        }
    }

    /// Clears the segment, removing all key-value pairs.
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.probationary.clear();
        self.protected.clear();
        self.current_size = 0;
    }
}

// Implement Debug for SlruSegment manually since it contains raw pointers
impl<K, V, S> core::fmt::Debug for SlruSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SlruSegment")
            .field("capacity", &self.config.capacity())
            .field("protected_capacity", &self.config.protected_capacity())
            .field("len", &self.map.len())
            .finish()
    }
}

/// An implementation of a Segmented Least Recently Used (SLRU) cache.
///
/// The cache is divided into two segments:
/// - Probationary segment: Where new entries are initially placed
/// - Protected segment: Where frequently accessed entries are promoted to
///
/// When the cache reaches capacity, the least recently used entry from the
/// probationary segment is evicted. If the probationary segment is empty,
/// entries from the protected segment may be demoted back to probationary.
///
/// # Examples
///
/// ```
/// use cache_rs::slru::SlruCache;
/// use core::num::NonZeroUsize;
///
/// // Create an SLRU cache with a total capacity of 4,
/// // with a protected capacity of 2 (half protected, half probationary)
/// let mut cache = SlruCache::new(
///     NonZeroUsize::new(4).unwrap(),
///     NonZeroUsize::new(2).unwrap()
/// );
///
/// // Add some items
/// cache.put("a", 1);
/// cache.put("b", 2);
/// cache.put("c", 3);
/// cache.put("d", 4);
///
/// // Access "a" to promote it to the protected segment
/// assert_eq!(cache.get(&"a"), Some(&1));
///
/// // Add a new item, which will evict the least recently used item
/// // from the probationary segment (likely "b")
/// cache.put("e", 5);
/// assert_eq!(cache.get(&"b"), None);
/// ```
#[derive(Debug)]
pub struct SlruCache<K, V, S = DefaultHashBuilder> {
    segment: SlruSegment<K, V, S>,
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> SlruCache<K, V, S> {
    /// Creates a new SLRU cache with the specified capacity and hash builder.
    ///
    /// # Parameters
    ///
    /// - `total_cap`: The total capacity of the cache. Must be a non-zero value.
    /// - `protected_cap`: The maximum capacity of the protected segment.
    ///   Must be a non-zero value and less than or equal to `total_cap`.
    /// - `hash_builder`: The hash builder to use for the underlying hash map.
    ///
    /// # Panics
    ///
    /// Panics if `protected_cap` is greater than `total_cap`.
    pub fn with_hasher(
        total_cap: NonZeroUsize,
        protected_cap: NonZeroUsize,
        hash_builder: S,
    ) -> Self {
        Self {
            segment: SlruSegment::with_hasher(total_cap, protected_cap, hash_builder),
        }
    }

    /// Creates a new SLRU cache with the specified capacity, hash builder, and max size.
    pub fn with_hasher_and_size(
        total_cap: NonZeroUsize,
        protected_cap: NonZeroUsize,
        hash_builder: S,
        max_size: u64,
    ) -> Self {
        Self {
            segment: SlruSegment::with_hasher_and_size(
                total_cap,
                protected_cap,
                hash_builder,
                max_size,
            ),
        }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    #[inline]
    pub fn cap(&self) -> NonZeroUsize {
        self.segment.cap()
    }

    /// Returns the maximum size of the protected segment.
    #[inline]
    pub fn protected_max_size(&self) -> NonZeroUsize {
        self.segment.protected_max_size()
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
    /// If a value is returned from the probationary segment, it is promoted
    /// to the protected segment.
    #[inline]
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
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
    /// If a value is returned from the probationary segment, it is promoted
    /// to the protected segment.
    #[inline]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get_mut(key)
    }

    /// Records a cache miss for metrics tracking (to be called by simulation system)
    #[inline]
    pub fn record_miss(&mut self, object_size: u64) {
        self.segment.record_miss(object_size);
    }
}

impl<K: Hash + Eq + Clone, V, S: BuildHasher> SlruCache<K, V, S> {
    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache already contained this key, the old value is replaced and returned.
    /// Otherwise, if the cache is at capacity, the least recently used item from the
    /// probationary segment will be evicted. If the probationary segment is empty,
    /// the least recently used item from the protected segment will be demoted to
    /// the probationary segment.
    ///
    /// The inserted key-value pair is always placed in the probationary segment.
    #[inline]
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        V: Clone,
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
        V: Clone,
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
}

impl<K: Hash + Eq, V> SlruCache<K, V>
where
    V: Clone,
{
    /// Creates a new SLRU cache from a configuration.
    ///
    /// This is the **recommended** way to create an SLRU cache. All configuration
    /// is specified through the [`SlruCacheConfig`] struct, which uses a builder
    /// pattern for optional parameters.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration specifying capacity, protected capacity, and optional size limit
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::SlruCache;
    /// use cache_rs::config::SlruCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// // Simple capacity-only cache with 20% protected segment
    /// let config = SlruCacheConfig::new(
    ///     NonZeroUsize::new(100).unwrap(),
    ///     NonZeroUsize::new(20).unwrap(),
    /// );
    /// let mut cache: SlruCache<&str, i32> = SlruCache::from_config(config);
    /// cache.put("key", 42);
    ///
    /// // Cache with size limit
    /// let config = SlruCacheConfig::new(
    ///     NonZeroUsize::new(1000).unwrap(),
    ///     NonZeroUsize::new(200).unwrap(),
    /// ).with_max_size(10 * 1024 * 1024);  // 10MB
    /// let cache: SlruCache<String, Vec<u8>> = SlruCache::from_config(config);
    /// ```
    pub fn from_config(config: SlruCacheConfig) -> SlruCache<K, V, DefaultHashBuilder> {
        SlruCache::with_hasher_and_size(
            config.capacity(),
            config.protected_capacity(),
            DefaultHashBuilder::default(),
            config.max_size(),
        )
    }

    /// Creates a new SLRU cache with the specified total and protected capacities.
    ///
    /// This is a convenience constructor. For more control, use [`SlruCache::from_config`].
    ///
    /// # Arguments
    ///
    /// * `total_cap` - The maximum total number of entries (probationary + protected).
    /// * `protected_cap` - The maximum number of entries in the protected segment.
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::SlruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// // Cache with total 100 entries, 20 in protected segment
    /// let mut cache = SlruCache::new(
    ///     NonZeroUsize::new(100).unwrap(),
    ///     NonZeroUsize::new(20).unwrap(),
    /// );
    /// cache.put("key", 42);
    /// ```
    pub fn new(
        total_cap: NonZeroUsize,
        protected_cap: NonZeroUsize,
    ) -> SlruCache<K, V, DefaultHashBuilder> {
        SlruCache::from_config(SlruCacheConfig::new(total_cap, protected_cap))
    }

    /// Creates a new SLRU cache with entry count and size limits.
    ///
    /// This is a convenience constructor. For more control, use [`SlruCache::from_config`].
    ///
    /// # Arguments
    ///
    /// * `total_cap` - The maximum total number of entries (probationary + protected).
    /// * `protected_cap` - The maximum number of entries in the protected segment.
    /// * `max_size` - The maximum total size in bytes (0 means unlimited).
    ///
    /// # Example
    ///
    /// ```
    /// use cache_rs::SlruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// // Cache with 1000 entries, 200 protected, and 10MB size limit
    /// let mut cache: SlruCache<String, Vec<u8>> = SlruCache::with_limits(
    ///     NonZeroUsize::new(1000).unwrap(),
    ///     NonZeroUsize::new(200).unwrap(),
    ///     10 * 1024 * 1024,
    /// );
    /// ```
    pub fn with_limits(
        total_cap: NonZeroUsize,
        protected_cap: NonZeroUsize,
        max_size: u64,
    ) -> SlruCache<K, V, DefaultHashBuilder> {
        SlruCache::from_config(
            SlruCacheConfig::new(total_cap, protected_cap).with_max_size(max_size),
        )
    }

    /// Creates a new SLRU cache with only a size limit.
    ///
    /// This is a convenience constructor that creates a cache limited by total size
    /// with very high entry count limits.
    ///
    /// # Arguments
    ///
    /// * `max_size` - The maximum total size in bytes.
    pub fn with_max_size(max_size: u64) -> SlruCache<K, V, DefaultHashBuilder> {
        // Use a large but reasonable capacity that won't overflow hash tables
        const MAX_REASONABLE_CAPACITY: usize = 1 << 30; // ~1 billion entries
        let default_cap = NonZeroUsize::new(MAX_REASONABLE_CAPACITY).unwrap();
        let default_protected = NonZeroUsize::new(MAX_REASONABLE_CAPACITY / 5).unwrap();
        SlruCache::from_config(
            SlruCacheConfig::new(default_cap, default_protected).with_max_size(max_size),
        )
    }
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> CacheMetrics for SlruCache<K, V, S> {
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
    fn test_slru_basic() {
        // Create a cache with capacity 4, with protected capacity 2
        // (2 probationary, 2 protected)
        let mut cache =
            SlruCache::new(NonZeroUsize::new(4).unwrap(), NonZeroUsize::new(2).unwrap());

        // Add items to fill probationary segment
        assert_eq!(cache.put("a", 1), None);
        assert_eq!(cache.put("b", 2), None);
        assert_eq!(cache.put("c", 3), None);
        assert_eq!(cache.put("d", 4), None);

        // Cache should be at capacity
        assert_eq!(cache.len(), 4);

        // Access "a" and "b" to promote them to protected segment
        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));

        // Add a new item "e", should evict "c" from probationary
        let evicted = cache.put("e", 5);
        assert!(evicted.is_some());
        let (evicted_key, evicted_val) = evicted.unwrap();
        assert_eq!(evicted_key, "c");
        assert_eq!(evicted_val, 3);

        // Add another item "f", should evict "d" from probationary
        let evicted = cache.put("f", 6);
        assert!(evicted.is_some());
        let (evicted_key, evicted_val) = evicted.unwrap();
        assert_eq!(evicted_key, "d");
        assert_eq!(evicted_val, 4);

        // Check presence
        assert_eq!(cache.get(&"a"), Some(&1)); // Protected
        assert_eq!(cache.get(&"b"), Some(&2)); // Protected
        assert_eq!(cache.get(&"c"), None); // Evicted
        assert_eq!(cache.get(&"d"), None); // Evicted
        assert_eq!(cache.get(&"e"), Some(&5)); // Probationary
        assert_eq!(cache.get(&"f"), Some(&6)); // Probationary
    }

    #[test]
    fn test_slru_update() {
        // Create a cache with capacity 4, with protected capacity 2
        let mut cache =
            SlruCache::new(NonZeroUsize::new(4).unwrap(), NonZeroUsize::new(2).unwrap());

        // Add items
        cache.put("a", 1);
        cache.put("b", 2);

        // Access "a" to promote it to protected
        assert_eq!(cache.get(&"a"), Some(&1));

        // Update values
        assert_eq!(cache.put("a", 10).unwrap().1, 1);
        assert_eq!(cache.put("b", 20).unwrap().1, 2);

        // Check updated values
        assert_eq!(cache.get(&"a"), Some(&10));
        assert_eq!(cache.get(&"b"), Some(&20));
    }

    #[test]
    fn test_slru_remove() {
        // Create a cache with capacity 4, with protected capacity 2
        let mut cache =
            SlruCache::new(NonZeroUsize::new(4).unwrap(), NonZeroUsize::new(2).unwrap());

        // Add items
        cache.put("a", 1);
        cache.put("b", 2);

        // Access "a" to promote it to protected
        assert_eq!(cache.get(&"a"), Some(&1));

        // Remove items
        assert_eq!(cache.remove(&"a"), Some(1)); // From protected
        assert_eq!(cache.remove(&"b"), Some(2)); // From probationary

        // Check that items are gone
        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), None);

        // Check that removing non-existent item returns None
        assert_eq!(cache.remove(&"c"), None);
    }

    #[test]
    fn test_slru_clear() {
        // Create a cache with capacity 4, with protected capacity 2
        let mut cache =
            SlruCache::new(NonZeroUsize::new(4).unwrap(), NonZeroUsize::new(2).unwrap());

        // Add items
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);
        cache.put("d", 4);

        // Clear the cache
        cache.clear();

        // Check that cache is empty
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        // Check that items are gone
        assert_eq!(cache.get(&"a"), None);
        assert_eq!(cache.get(&"b"), None);
        assert_eq!(cache.get(&"c"), None);
        assert_eq!(cache.get(&"d"), None);
    }

    #[test]
    fn test_slru_complex_values() {
        // Create a cache with capacity 4, with protected capacity 2
        let mut cache =
            SlruCache::new(NonZeroUsize::new(4).unwrap(), NonZeroUsize::new(2).unwrap());

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
    fn test_slru_with_ratio() {
        // Test the with_ratio constructor
        let mut cache =
            SlruCache::new(NonZeroUsize::new(4).unwrap(), NonZeroUsize::new(2).unwrap());

        assert_eq!(cache.cap().get(), 4);
        assert_eq!(cache.protected_max_size().get(), 2);

        // Test basic functionality
        assert_eq!(cache.put("a", 1), None);
        assert_eq!(cache.put("b", 2), None);

        // Access "a" to promote it to protected
        assert_eq!(cache.get(&"a"), Some(&1));

        // Fill the cache
        assert_eq!(cache.put("c", 3), None);
        assert_eq!(cache.put("d", 4), None);

        // Add another item, should evict "b" from probationary
        let result = cache.put("e", 5);
        assert_eq!(result.unwrap().0, "b");

        // Check that protected items remain
        assert_eq!(cache.get(&"a"), Some(&1));
    }

    // Test that SlruSegment works correctly (internal tests)
    #[test]
    fn test_slru_segment_directly() {
        let mut segment: SlruSegment<&str, i32, DefaultHashBuilder> = SlruSegment::with_hasher(
            NonZeroUsize::new(4).unwrap(),
            NonZeroUsize::new(2).unwrap(),
            DefaultHashBuilder::default(),
        );

        assert_eq!(segment.len(), 0);
        assert!(segment.is_empty());
        assert_eq!(segment.cap().get(), 4);
        assert_eq!(segment.protected_max_size().get(), 2);

        segment.put("a", 1);
        segment.put("b", 2);
        assert_eq!(segment.len(), 2);

        // Access to promote
        assert_eq!(segment.get(&"a"), Some(&1));
        assert_eq!(segment.get(&"b"), Some(&2));
    }

    #[test]
    fn test_slru_concurrent_access() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(SlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(50).unwrap(),
        )));
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
    fn test_slru_size_aware_tracking() {
        let mut cache = SlruCache::new(
            NonZeroUsize::new(10).unwrap(),
            NonZeroUsize::new(3).unwrap(),
        );

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
    fn test_slru_from_config_constructor() {
        let config = SlruCacheConfig::new(
            NonZeroUsize::new(1000).unwrap(),
            NonZeroUsize::new(300).unwrap(),
        )
        .with_max_size(1024 * 1024);
        let cache: SlruCache<String, i32> = SlruCache::from_config(config);

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
    }

    #[test]
    fn test_slru_with_limits_constructor() {
        let cache: SlruCache<String, String> = SlruCache::with_limits(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(30).unwrap(),
            1024 * 1024,
        );

        assert_eq!(cache.current_size(), 0);
        assert_eq!(cache.max_size(), 1024 * 1024);
        assert_eq!(cache.cap().get(), 100);
        assert_eq!(cache.protected_max_size().get(), 30);
    }

    #[test]
    fn test_slru_record_miss() {
        use crate::metrics::CacheMetrics;

        let mut cache: SlruCache<String, i32> = SlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(30).unwrap(),
        );

        cache.record_miss(100);
        cache.record_miss(200);

        let metrics = cache.metrics();
        assert_eq!(metrics.get("cache_misses").unwrap(), &2.0);
    }

    #[test]
    fn test_slru_get_mut() {
        let mut cache: SlruCache<String, i32> = SlruCache::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(30).unwrap(),
        );

        cache.put("key".to_string(), 10);
        assert_eq!(cache.get(&"key".to_string()), Some(&10));

        // Modify via get_mut
        if let Some(val) = cache.get_mut(&"key".to_string()) {
            *val = 42;
        }
        assert_eq!(cache.get(&"key".to_string()), Some(&42));

        // get_mut on missing key returns None
        assert!(cache.get_mut(&"missing".to_string()).is_none());
    }
}
