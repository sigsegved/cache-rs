//! Least Frequently Used with Dynamic Aging (LFUDA) Cache Implementation.
//!
//! The LFUDA cache is an enhancement of the basic LFU algorithm that addresses the
//! "aging problem" where old frequently-used items can prevent new items from being
//! cached even if they're no longer actively accessed.
//!
//! # Algorithm
//!
//! LFUDA works by maintaining a global age value that increases when items are evicted.
//! Each item's priority is calculated using the formula:
//!
//! ```text
//! priority = access_frequency + age_at_insertion
//! ```
//!
//! When an item is accessed, its frequency is incremented. When an item is evicted,
//! the global age value is set to the priority of the evicted item. New items are
//! inserted with the current age value as their initial age, giving them a boost to
//! compete with older items.
//!
//! ## Mathematical Formulation
//!
//! For each cache entry i:
//! - Let F_i be the access frequency of item i
//! - Let A_i be the age factor when item i was inserted
//! - Let P_i = F_i + A_i be the priority value
//! - On eviction, select item j where P_j = min{P_i for all i}
//! - After eviction, set global_age = P_j
//!
//! # Performance Characteristics
//!
//! - **Time Complexity**:
//!   - Get: O(1)
//!   - Put: O(1)
//!   - Remove: O(1)
//!
//! - **Space Complexity**:
//!   - O(n) where n is the capacity of the cache
//!   - Slightly more overhead than LRU due to frequency and age tracking
//!
//! # When to Use
//!
//! LFUDA caches are ideal for:
//! - Long-running caches where popularity of items changes over time
//! - Workloads where frequency of access is more important than recency
//! - Environments where protecting against cache pollution from one-time scans is important
//!
//! # Thread Safety
//!
//! This implementation is not thread-safe. For concurrent access, wrap the cache
//! with a synchronization primitive such as `Mutex` or `RwLock`.

extern crate alloc;

use crate::config::LfudaCacheConfig;
use crate::list::{Entry, List};
use crate::metrics::{CacheMetrics, LfudaCacheMetrics};
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

/// Metadata for each cache entry in LFUDA
#[derive(Debug, Clone, Copy)]
struct EntryMetadata<K, V> {
    /// Frequency of access for this item
    frequency: usize,
    /// Age value when this item was inserted
    age_at_insertion: usize,
    /// Pointer to the entry in the frequency list
    node: *mut Entry<(K, V)>,
}

/// Internal LFUDA segment containing the actual cache algorithm.
///
/// This is shared between `LfudaCache` (single-threaded) and
/// `ConcurrentLfudaCache` (multi-threaded). All algorithm logic is
/// implemented here to avoid code duplication.
///
/// # Safety
///
/// This struct contains raw pointers in the `map` field (via EntryMetadata).
/// These pointers are always valid as long as:
/// - The pointer was obtained from a `priority_lists` entry's `add()` call
/// - The node has not been removed from the list
/// - The segment has not been dropped
pub(crate) struct LfudaSegment<K, V, S = DefaultHashBuilder> {
    /// Configuration for the LFUDA cache
    config: LfudaCacheConfig,

    /// Global age value that increases when items are evicted
    global_age: usize,

    /// Current minimum effective priority in the cache
    min_priority: usize,

    /// Map from keys to their metadata
    map: HashMap<K, EntryMetadata<K, V>, S>,

    /// Map from effective priority to list of items with that priority
    /// Items within each priority list are ordered by recency (LRU within priority)
    priority_lists: BTreeMap<usize, List<(K, V)>>,

    /// Metrics tracking for this cache instance
    metrics: LfudaCacheMetrics,
}

// SAFETY: LfudaSegment owns all data and raw pointers point only to nodes owned by
// `priority_lists`. Concurrent access is safe when wrapped in proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for LfudaSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for LfudaSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LfudaSegment<K, V, S> {
    /// Creates a new LFUDA segment with the specified capacity and hash builder.
    pub(crate) fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        let config = LfudaCacheConfig::new(cap);
        let map_capacity = config.capacity().get().next_power_of_two();
        let max_cache_size_bytes = config.capacity().get() as u64 * 128;

        LfudaSegment {
            config,
            global_age: config.initial_age(),
            min_priority: 0,
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            priority_lists: BTreeMap::new(),
            metrics: LfudaCacheMetrics::new(max_cache_size_bytes),
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
    pub(crate) fn global_age(&self) -> usize {
        self.global_age
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
    /// The caller must ensure that `node` is a valid pointer to an Entry that exists
    /// in this cache's priority lists and has not been freed.
    unsafe fn update_priority_by_node(&mut self, node: *mut Entry<(K, V)>) -> *mut Entry<(K, V)>
    where
        K: Clone + Hash + Eq,
    {
        // SAFETY: node is guaranteed to be valid by the caller's contract
        let (key_ref, _) = (*node).get_value();
        let key_cloned = key_ref.clone();

        let metadata = self.map.get_mut(&key_cloned).unwrap();
        let old_priority = metadata.frequency + metadata.age_at_insertion;

        // Increment frequency
        metadata.frequency += 1;
        let new_priority = metadata.frequency + metadata.age_at_insertion;

        // If priority hasn't changed, just move to front of the same list
        if old_priority == new_priority {
            let node = metadata.node;
            self.priority_lists
                .get_mut(&old_priority)
                .unwrap()
                .move_to_front(node);
            return node;
        }

        // Remove from old priority list
        let node = metadata.node;
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

        // Add to new priority list
        let entry_ptr = Box::into_raw(boxed_entry);

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

        // Update the metadata
        metadata.node = entry_ptr;

        entry_ptr
    }

    /// Returns a reference to the value corresponding to the key.
    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(metadata) = self.map.get(key) {
            let node = metadata.node;
            unsafe {
                // SAFETY: node comes from our map
                let (key_ref, value) = (*node).get_value();
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.core.record_hit(object_size);

                let new_node = self.update_priority_by_node(node);
                let (_, value) = (*new_node).get_value();
                Some(value)
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
        if let Some(metadata) = self.map.get(key) {
            let node = metadata.node;
            unsafe {
                // SAFETY: node comes from our map
                let (key_ref, value) = (*node).get_value();
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.core.record_hit(object_size);

                let old_frequency = metadata.frequency;
                let new_priority = (old_frequency + 1) + metadata.age_at_insertion;
                self.metrics.record_frequency_increment(new_priority as u64);

                let new_node = self.update_priority_by_node(node);
                let (_, value) = (*new_node).get_value_mut();
                Some(value)
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

        // If key already exists, update it
        if let Some(metadata) = self.map.get(&key) {
            let node = metadata.node;
            let priority = metadata.frequency + metadata.age_at_insertion;

            unsafe {
                // SAFETY: node comes from our map
                let old_entry = self.priority_lists.get_mut(&priority).unwrap().update(
                    node,
                    (key.clone(), value),
                    true,
                );

                self.metrics.core.record_insertion(object_size);
                return old_entry.0;
            }
        }

        let mut evicted = None;

        // Add new item with frequency 1 and current global age
        let frequency = 1;
        let age_at_insertion = self.global_age;
        let priority = frequency + age_at_insertion;

        // If at capacity, evict the item with lowest effective priority
        if self.len() >= self.config.capacity().get() {
            if let Some(min_priority_list) = self.priority_lists.get_mut(&self.min_priority) {
                if let Some(old_entry) = min_priority_list.remove_last() {
                    unsafe {
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();

                        let evicted_size = self.estimate_object_size(&old_key, &old_value);
                        self.metrics.core.record_eviction(evicted_size);

                        // Update global age to the evicted item's effective priority
                        if let Some(evicted_metadata) = self.map.get(&old_key) {
                            self.global_age =
                                evicted_metadata.frequency + evicted_metadata.age_at_insertion;
                            self.metrics.record_aging_event(self.global_age as u64);
                        }

                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));
                        let _ = Box::from_raw(entry_ptr);

                        // Update min_priority if the list becomes empty
                        if self
                            .priority_lists
                            .get(&self.min_priority)
                            .unwrap()
                            .is_empty()
                        {
                            self.min_priority = self
                                .priority_lists
                                .keys()
                                .find(|&&p| {
                                    p > self.min_priority
                                        && !self.priority_lists.get(&p).unwrap().is_empty()
                                })
                                .copied()
                                .unwrap_or(priority);
                        }
                    }
                }
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

        if let Some(node) = self
            .priority_lists
            .get_mut(&priority)
            .unwrap()
            .add((key.clone(), value))
        {
            let metadata = EntryMetadata {
                frequency,
                age_at_insertion,
                node,
            };

            self.map.insert(key, metadata);

            self.metrics.core.record_insertion(object_size);
            self.metrics.record_frequency_increment(priority as u64);
            if age_at_insertion > 0 {
                self.metrics.record_aging_benefit(age_at_insertion as u64);
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
        let metadata = self.map.remove(key)?;
        let priority = metadata.frequency + metadata.age_at_insertion;

        unsafe {
            // SAFETY: metadata.node comes from our map and was just removed
            let boxed_entry = self
                .priority_lists
                .get_mut(&priority)?
                .remove(metadata.node)?;
            let entry_ptr = Box::into_raw(boxed_entry);
            let value = (*entry_ptr).get_value().1.clone();
            let _ = Box::from_raw(entry_ptr);

            // Update min_priority if necessary
            if self.priority_lists.get(&priority).unwrap().is_empty()
                && priority == self.min_priority
            {
                self.min_priority = self
                    .priority_lists
                    .keys()
                    .find(|&&p| p > priority && !self.priority_lists.get(&p).unwrap().is_empty())
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
    }

    /// Removes and returns the eviction candidate (lowest priority item).
    ///
    /// For LFUDA, this is the item with the lowest effective priority
    /// (frequency + age_at_insertion). This also updates the global age
    /// to the evicted item's priority.
    ///
    /// Returns `None` if the cache is empty.
    pub(crate) fn pop(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        if self.is_empty() {
            return None;
        }

        let min_priority = *self.priority_lists.keys().next()?;
        let min_priority_list = self.priority_lists.get_mut(&min_priority)?;
        let old_entry = min_priority_list.remove_last()?;
        let is_list_empty = min_priority_list.is_empty();

        unsafe {
            // SAFETY: entry comes from priority_lists.remove_last()
            let entry_ptr = Box::into_raw(old_entry);
            let (key, value) = (*entry_ptr).get_value().clone();
            let object_size = mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64;
            self.map.remove(&key);
            self.metrics.core.record_eviction(object_size);

            // Update global age to the evicted item's priority (LFUDA aging)
            self.global_age = min_priority;

            // Remove empty priority list and update min_priority
            if is_list_empty {
                self.priority_lists.remove(&min_priority);
                self.min_priority = self
                    .priority_lists
                    .keys()
                    .copied()
                    .next()
                    .unwrap_or(self.global_age);
            }

            let _ = Box::from_raw(entry_ptr);
            Some((key, value))
        }
    }

    /// Removes and returns the item with the highest priority (reverse of pop).
    ///
    /// This is the opposite of `pop()` - instead of returning the lowest priority item,
    /// it returns the highest priority item. If there are multiple items with the same
    /// highest priority, the most recently used among them is returned.
    ///
    /// Returns `None` if the cache is empty.
    pub(crate) fn popr(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        if self.is_empty() {
            return None;
        }

        // Get the highest priority (last key in BTreeMap)
        let max_priority = *self.priority_lists.keys().next_back()?;
        let max_priority_list = self.priority_lists.get_mut(&max_priority)?;
        let entry = max_priority_list.remove_first()?;
        let is_list_empty = max_priority_list.is_empty();

        unsafe {
            // SAFETY: entry comes from priority_lists.remove_first()
            let entry_ptr = Box::into_raw(entry);
            let (key, value) = (*entry_ptr).get_value().clone();
            let object_size = mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64;
            self.map.remove(&key);
            self.metrics.core.record_eviction(object_size);

            // Remove empty priority list
            if is_list_empty {
                self.priority_lists.remove(&max_priority);
            }

            let _ = Box::from_raw(entry_ptr);
            Some((key, value))
        }
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

    /// Returns the current global age value.
    #[inline]
    pub fn global_age(&self) -> usize {
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

    /// Removes and returns the eviction candidate (lowest priority item).
    ///
    /// For LFUDA, this is the item with the lowest effective priority
    /// (frequency + age_at_insertion). This also updates the global age.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LfudaCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.get(&"b"); // Increase frequency of "b"
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

    /// Removes and returns the item with the highest priority (reverse of pop).
    ///
    /// This is the opposite of `pop()` - instead of returning the lowest priority item,
    /// it returns the highest priority item.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfuda::LfudaCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LfudaCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.get(&"b"); // Increase frequency of "b"
    /// cache.get(&"b"); // Increase frequency again
    ///
    /// // Pop the highest priority item
    /// assert_eq!(cache.popr(), Some(("b", 2)));
    /// ```
    #[inline]
    pub fn popr(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        self.segment.popr()
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
}
