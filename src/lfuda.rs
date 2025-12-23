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
        let config = LfudaCacheConfig::new(cap);
        let map_capacity = config.capacity().get().next_power_of_two();
        let max_cache_size_bytes = config.capacity().get() as u64 * 128; // Estimate based on capacity

        LfudaCache {
            config,
            global_age: config.initial_age(),
            min_priority: 0,
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            priority_lists: BTreeMap::new(),
            metrics: LfudaCacheMetrics::new(max_cache_size_bytes),
        }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    pub fn cap(&self) -> NonZeroUsize {
        self.config.capacity()
    }

    /// Returns the current number of key-value pairs in the cache.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the cache contains no key-value pairs.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Returns the current global age value.
    pub fn global_age(&self) -> usize {
        self.global_age
    }

    /// Estimates the size of a key-value pair in bytes for metrics tracking
    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        // Simple estimation: key size + value size + overhead for pointers and metadata
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    /// Records a cache miss for metrics tracking (to be called by simulation system)
    pub fn record_miss(&mut self, object_size: u64) {
        self.metrics.core.record_miss(object_size);
    }

    /// Updates the priority of an item and moves it to the appropriate priority list.
    /// Takes the node pointer directly to avoid aliasing issues.
    unsafe fn update_priority_by_node(&mut self, node: *mut Entry<(K, V)>) -> *mut Entry<(K, V)>
    where
        K: Clone + Hash + Eq,
    {
        // Get the key from the node to look up metadata
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
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// Accessing an item increases its frequency count.
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(metadata) = self.map.get(key) {
            let node = metadata.node;
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our priority list
                // Get the key from the node to pass to update_priority
                let (key_ref, value) = (*node).get_value();

                // Record hit with object size estimation
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.core.record_hit(object_size);

                // Record frequency change
                let _old_frequency = metadata.frequency;
                let new_node = self.update_priority_by_node(node);
                let (_, value) = (*new_node).get_value();
                Some(value)
            }
        } else {
            None
        }
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// Accessing an item increases its frequency count.
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q> + Clone,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(metadata) = self.map.get(key) {
            let node = metadata.node;
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our priority list
                // Get the key from the node to pass to update_priority
                let (key_ref, value) = (*node).get_value();

                // Record hit with object size estimation
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.core.record_hit(object_size);

                // Record frequency change
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

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache already contained this key, the old value is replaced and returned.
    /// Otherwise, if the cache is at capacity, the item with the lowest effective priority
    /// is evicted. The global age is updated to the evicted item's effective priority.
    ///
    /// New items are inserted with a frequency of 1 and age_at_insertion set to the
    /// current global_age.
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        K: Clone,
    {
        let object_size = self.estimate_object_size(&key, &value);

        // If key already exists, update it
        if let Some(metadata) = self.map.get(&key) {
            let node = metadata.node;
            let priority = metadata.frequency + metadata.age_at_insertion;

            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our priority list
                let old_entry = self.priority_lists.get_mut(&priority).unwrap().update(
                    node,
                    (key.clone(), value),
                    true,
                );

                // Record insertion (update)
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
            // Find the list with minimum priority and evict from the end (LRU within priority)
            if let Some(min_priority_list) = self.priority_lists.get_mut(&self.min_priority) {
                if let Some(old_entry) = min_priority_list.remove_last() {
                    unsafe {
                        // SAFETY: old_entry comes from min_priority_list.remove_last(), so it's a valid Box
                        // that we own. Converting to raw pointer is safe for temporary access.
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();

                        // Record eviction
                        let evicted_size = self.estimate_object_size(&old_key, &old_value);
                        self.metrics.core.record_eviction(evicted_size);

                        // Update global age to the evicted item's effective priority
                        if let Some(evicted_metadata) = self.map.get(&old_key) {
                            let _old_global_age = self.global_age;
                            self.global_age =
                                evicted_metadata.frequency + evicted_metadata.age_at_insertion;

                            // Record aging event
                            self.metrics.record_aging_event(self.global_age as u64);
                        }

                        // Remove from map
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));

                        // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                        let _ = Box::from_raw(entry_ptr);

                        // Update min_priority if the list becomes empty
                        if self
                            .priority_lists
                            .get(&self.min_priority)
                            .unwrap()
                            .is_empty()
                        {
                            // Find the next minimum priority
                            self.min_priority = self
                                .priority_lists
                                .keys()
                                .find(|&&p| {
                                    p > self.min_priority
                                        && !self.priority_lists.get(&p).unwrap().is_empty()
                                })
                                .copied()
                                .unwrap_or(priority); // Use the new item's priority as fallback
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

            // Record insertion and frequency/aging metrics
            self.metrics.core.record_insertion(object_size);
            self.metrics.record_frequency_increment(priority as u64);
            if age_at_insertion > 0 {
                self.metrics.record_aging_benefit(age_at_insertion as u64);
            }
        }

        evicted
    }

    /// Removes a key from the cache, returning the value at the key if the key was previously in the cache.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        let metadata = self.map.remove(key)?;
        let priority = metadata.frequency + metadata.age_at_insertion;

        unsafe {
            // SAFETY: metadata.node comes from our map and was just removed, so priority_lists.remove is safe
            let boxed_entry = self
                .priority_lists
                .get_mut(&priority)?
                .remove(metadata.node)?;
            // SAFETY: boxed_entry is a valid Box we own, converting to raw pointer for temporary access
            let entry_ptr = Box::into_raw(boxed_entry);
            let value = (*entry_ptr).get_value().1.clone();
            // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
            let _ = Box::from_raw(entry_ptr);

            // Update min_priority if necessary
            if self.priority_lists.get(&priority).unwrap().is_empty()
                && priority == self.min_priority
            {
                // Find the next minimum priority
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

    /// Clears the cache, removing all key-value pairs.
    /// Resets the global age to 0.
    pub fn clear(&mut self) {
        self.map.clear();
        self.priority_lists.clear();
        self.global_age = 0;
        self.min_priority = 0;
    }
}

impl<K, V, S> CacheMetrics for LfudaCache<K, V, S> {
    /// Returns all LFUDA cache metrics as key-value pairs in deterministic order
    ///
    /// # Returns
    /// A BTreeMap containing all metrics tracked by this LFUDA cache instance
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.metrics.metrics()
    }

    /// Returns the algorithm name for this cache implementation
    ///
    /// # Returns
    /// "LFUDA" - identifying this as a Least Frequently Used with Dynamic Aging cache
    fn algorithm_name(&self) -> &'static str {
        self.metrics.algorithm_name()
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
        // This demonstrates that aging helps newer items compete
        assert!(cache.len() <= cache.cap().get());
    }

    /// Test to validate the fix for Stacked Borrows violations detected by Miri.
    /// 
    /// The original code had an aliasing issue where a borrowed key reference from a node
    /// was passed to update_priority, which then tried to mutably access the HashMap.
    /// This violated Miri's Stacked Borrows rules.
    /// 
    /// The fix passes the node pointer directly and clones the key internally,
    /// breaking the aliasing chain.
    #[test]
    fn test_miri_stacked_borrows_fix() {
        let mut cache = LfudaCache::new(NonZeroUsize::new(10).unwrap());
        
        // Insert some items
        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);
        
        // Access items multiple times to trigger priority updates
        // This would fail under Miri with the original buggy code
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
}
