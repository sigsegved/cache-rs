//! Segmented Least Recently Used Cache Implementation.
//!
//! The SLRU (Segmented LRU) cache divides the cache into two segments:
//! - Probationary segment: Where new entries are initially placed
//! - Protected segment: Where frequently accessed entries are promoted to
//!
//! This implementation provides better performance for scan-resistant workloads
//! compared to standard LRU, as it protects frequently accessed items from
//! being evicted by one-time scans through the data.

extern crate alloc;

use crate::config::SlruCacheConfig;
use crate::list::{Entry, List};
use crate::metrics::{CacheMetrics, SlruCacheMetrics};
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

/// Entry location within the SLRU cache
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Location {
    /// Entry is in the probationary segment
    Probationary,
    /// Entry is in the protected segment
    Protected,
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
    /// Configuration for the SLRU cache
    config: SlruCacheConfig,

    /// The probationary list holding newer or less frequently accessed items
    probationary: List<(K, V)>,

    /// The protected list holding frequently accessed items
    protected: List<(K, V)>,

    /// A hash map mapping keys to entries in either the probationary or protected list
    #[allow(clippy::type_complexity)]
    map: HashMap<K, (*mut Entry<(K, V)>, Location), S>,

    /// Metrics for tracking cache performance and segment behavior
    metrics: SlruCacheMetrics,
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
        let config = SlruCacheConfig::new(total_cap, protected_cap);

        let probationary_max_size =
            NonZeroUsize::new(config.capacity().get() - config.protected_capacity().get()).unwrap();

        let max_cache_size_bytes = config.capacity().get() as u64 * 128; // Estimate based on capacity

        SlruCache {
            config,
            probationary: List::new(probationary_max_size),
            protected: List::new(config.protected_capacity()),
            map: HashMap::with_capacity_and_hasher(
                config.capacity().get().next_power_of_two(),
                hash_builder,
            ),
            metrics: SlruCacheMetrics::new(
                max_cache_size_bytes,
                config.protected_capacity().get() as u64,
            ),
        }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    pub fn cap(&self) -> NonZeroUsize {
        self.config.capacity()
    }

    /// Returns the maximum size of the protected segment.
    pub fn protected_max_size(&self) -> NonZeroUsize {
        self.config.protected_capacity()
    }

    /// Returns the current number of key-value pairs in the cache.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the cache contains no key-value pairs.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Estimates the size of a key-value pair in bytes for metrics tracking
    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        // Simple estimation: key size + value size + overhead for pointers and metadata
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    /// Moves an entry from the probationary segment to the protected segment.
    /// If the protected segment is full, the LRU item from protected is demoted to probationary.
    ///
    /// Returns a raw pointer to the entry in its new location.
    unsafe fn promote_to_protected(&mut self, node: *mut Entry<(K, V)>) -> *mut Entry<(K, V)> {
        // Remove from probationary list - this removes the node and returns it as a Box
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
                    self.map.remove(old_key);
                    let _ = Box::from_raw(old_ptr);
                }
            }
            self.demote_lru_protected();
        }

        // Get the raw pointer from the box - this should be the same as the original node pointer
        let entry_ptr = Box::into_raw(boxed_entry);

        // Get the key from the entry for updating the map
        let (key_ref, _) = (*entry_ptr).get_value();

        // Update the map with new location and pointer
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

            // Update the location and pointer in the map
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
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// If a value is returned from the probationary segment, it is promoted
    /// to the protected segment.
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (node, location) = self.map.get(key).copied()?;

        match location {
            Location::Probationary => unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our probationary list
                let (key_ref, value) = (*node).get_value();
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.record_probationary_hit(object_size);

                // Promote from probationary to protected
                let entry_ptr = self.promote_to_protected(node);

                // Record promotion
                self.metrics.record_promotion();

                // Update segment sizes
                self.metrics.update_segment_sizes(
                    self.probationary.len() as u64,
                    self.protected.len() as u64,
                );

                // SAFETY: entry_ptr is the return value from promote_to_protected, which ensures
                // it points to a valid entry in the protected list
                let (_, v) = (*entry_ptr).get_value();
                Some(v)
            },
            Location::Protected => unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our protected list
                let (key_ref, value) = (*node).get_value();
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.record_protected_hit(object_size);

                // Already protected, just move to MRU position
                self.protected.move_to_front(node);
                let (_, v) = (*node).get_value();
                Some(v)
            },
        }
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// If a value is returned from the probationary segment, it is promoted
    /// to the protected segment.
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let (node, location) = self.map.get(key).copied()?;

        match location {
            Location::Probationary => unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our probationary list
                let (key_ref, value) = (*node).get_value();
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.record_probationary_hit(object_size);

                // Promote from probationary to protected
                let entry_ptr = self.promote_to_protected(node);

                // Record promotion
                self.metrics.record_promotion();

                // Update segment sizes
                self.metrics.update_segment_sizes(
                    self.probationary.len() as u64,
                    self.protected.len() as u64,
                );

                // SAFETY: entry_ptr is the return value from promote_to_protected, which ensures
                // it points to a valid entry in the protected list
                let (_, v) = (*entry_ptr).get_value_mut();
                Some(v)
            },
            Location::Protected => unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our protected list
                let (key_ref, value) = (*node).get_value();
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.record_protected_hit(object_size);

                // Already protected, just move to MRU position
                self.protected.move_to_front(node);
                // SAFETY: node is still valid after move_to_front operation
                let (_, v) = (*node).get_value_mut();
                Some(v)
            },
        }
    }

    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache already contained this key, the old value is replaced and returned.
    /// Otherwise, if the cache is at capacity, the least recently used item from the
    /// probationary segment will be evicted. If the probationary segment is empty,
    /// the least recently used item from the protected segment will be demoted to
    /// the probationary segment.
    ///
    /// The inserted key-value pair is always placed in the probationary segment.
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        K: Clone,
    {
        let object_size = self.estimate_object_size(&key, &value);

        // If key is already in the cache, update it in place
        if let Some(&(node, location)) = self.map.get(&key) {
            match location {
                Location::Probationary => unsafe {
                    // SAFETY: node comes from our map, so it's a valid pointer to an entry in our probationary list
                    self.probationary.move_to_front(node);
                    let old_entry = self.probationary.update(node, (key.clone(), value), true);

                    // Record insertion (update)
                    self.metrics.core.record_insertion(object_size);

                    return old_entry.0;
                },
                Location::Protected => unsafe {
                    // SAFETY: node comes from our map, so it's a valid pointer to an entry in our protected list
                    self.protected.move_to_front(node);
                    let old_entry = self.protected.update(node, (key.clone(), value), true);

                    // Record insertion (update)
                    self.metrics.core.record_insertion(object_size);

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
                        // SAFETY: old_entry comes from probationary.remove_last(), so it's a valid Box
                        // that we own. Converting to raw pointer is safe for temporary access.
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();

                        // Record probationary eviction
                        let evicted_size = self.estimate_object_size(&old_key, &old_value);
                        self.metrics.record_probationary_eviction(evicted_size);

                        // Remove from map
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));

                        // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                        let _ = Box::from_raw(entry_ptr);
                    }
                }
            } else if !self.protected.is_empty() {
                // If probationary is empty, evict from protected
                if let Some(old_entry) = self.protected.remove_last() {
                    unsafe {
                        // SAFETY: old_entry comes from protected.remove_last(), so it's a valid Box
                        // that we own. Converting to raw pointer is safe for temporary access.
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();

                        // Record protected eviction
                        let evicted_size = self.estimate_object_size(&old_key, &old_value);
                        self.metrics.record_protected_eviction(evicted_size);

                        // Remove from map
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));

                        // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                        let _ = Box::from_raw(entry_ptr);
                    }
                }
            }
        }

        // Add the new key-value pair to the probationary segment
        if self.len() < self.cap().get() {
            // Total cache has space, allow probationary to exceed its capacity
            let node = self.probationary.add_unchecked((key.clone(), value));
            self.map.insert(key, (node, Location::Probationary));
        } else {
            // Total cache is full, try to add normally (may fail due to probationary capacity)
            if let Some(node) = self.probationary.add((key.clone(), value.clone())) {
                self.map.insert(key, (node, Location::Probationary));
            } else {
                // Probationary is at capacity, need to make space
                if let Some(old_entry) = self.probationary.remove_last() {
                    unsafe {
                        // SAFETY: old_entry comes from probationary.remove_last(), so it's a valid Box
                        // that we own. Converting to raw pointer is safe for temporary access.
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();

                        // Record probationary eviction
                        let evicted_size = self.estimate_object_size(&old_key, &old_value);
                        self.metrics.record_probationary_eviction(evicted_size);

                        // Remove from map
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));

                        // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                        let _ = Box::from_raw(entry_ptr);
                    }
                }

                // Try again after making space
                if let Some(node) = self.probationary.add((key.clone(), value)) {
                    self.map.insert(key, (node, Location::Probationary));
                }
            }
        }

        // Record insertion and update segment sizes
        self.metrics.core.record_insertion(object_size);
        self.metrics
            .update_segment_sizes(self.probationary.len() as u64, self.protected.len() as u64);

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
        let (node, location) = self.map.remove(key)?;

        match location {
            Location::Probationary => unsafe {
                // SAFETY: node comes from our map and was just removed, so probationary.remove is safe
                let boxed_entry = self.probationary.remove(node)?;
                // SAFETY: boxed_entry is a valid Box we own, converting to raw pointer for temporary access
                let entry_ptr = Box::into_raw(boxed_entry);
                let value = (*entry_ptr).get_value().1.clone();
                // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                let _ = Box::from_raw(entry_ptr);
                Some(value)
            },
            Location::Protected => unsafe {
                // SAFETY: node comes from our map and was just removed, so protected.remove is safe
                let boxed_entry = self.protected.remove(node)?;
                // SAFETY: boxed_entry is a valid Box we own, converting to raw pointer for temporary access
                let entry_ptr = Box::into_raw(boxed_entry);
                let value = (*entry_ptr).get_value().1.clone();
                // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                let _ = Box::from_raw(entry_ptr);
                Some(value)
            },
        }
    }

    /// Clears the cache, removing all key-value pairs.
    pub fn clear(&mut self) {
        self.map.clear();
        self.probationary.clear();
        self.protected.clear();
    }

    /// Records a cache miss for metrics tracking (to be called by simulation system)
    pub fn record_miss(&mut self, object_size: u64) {
        self.metrics.core.record_miss(object_size);
    }
}

impl<K: Hash + Eq, V> SlruCache<K, V>
where
    V: Clone,
{
    /// Creates a new SLRU cache with the specified capacity and protected capacity.
    ///
    /// The total capacity must be greater than the protected capacity.
    ///
    /// # Panics
    ///
    /// Panics if `protected_capacity` is greater than `capacity`.
    pub fn new(
        capacity: NonZeroUsize,
        protected_capacity: NonZeroUsize,
    ) -> SlruCache<K, V, DefaultHashBuilder> {
        let config = SlruCacheConfig::new(capacity, protected_capacity);
        SlruCache::with_hasher(
            config.capacity(),
            config.protected_capacity(),
            DefaultHashBuilder::default(),
        )
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> CacheMetrics for SlruCache<K, V, S> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.metrics.metrics()
    }

    fn algorithm_name(&self) -> &'static str {
        self.metrics.algorithm_name()
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
}
