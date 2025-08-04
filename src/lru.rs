//! Least Recently Used (LRU) Cache Implementation
//!
//! This module provides a memory-efficient LRU cache implementation with O(1) operations
//! for all common cache operations. LRU is one of the most widely used cache eviction
//! algorithms due to its simplicity and good performance for workloads with temporal locality.
//!
//! # Algorithm
//!
//! The LRU cache maintains items in order of recency of use, evicting the least recently
//! used item when capacity is reached. This works on the principle of temporal locality:
//! items that have been accessed recently are likely to be accessed again soon.
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
//!   - Memory overhead is approximately 48 bytes per entry plus the size of keys and values
//!
//! # When to Use
//!
//! LRU caches are ideal for:
//! - General-purpose caching where access patterns exhibit temporal locality
//! - Simple implementation with predictable performance
//! - Caching with a fixed memory budget
//!
//! They are less suitable for:
//! - Workloads where frequency of access is more important than recency
//! - Scanning patterns where a large set of items is accessed once in sequence
//!
//! # Thread Safety
//!
//! This implementation is not thread-safe. For concurrent access, you should
//! wrap the cache with a synchronization primitive such as `Mutex` or `RwLock`.

extern crate alloc;

use crate::config::LruCacheConfig;
use crate::list::{Entry, List};
use crate::metrics::{CacheMetrics, LruCacheMetrics};
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

/// An implementation of a Least Recently Used (LRU) cache.
///
/// The cache has a fixed capacity and supports O(1) operations for
/// inserting, retrieving, and updating entries. When the cache reaches capacity,
/// the least recently used entry will be evicted to make room for new entries.
///
/// # Examples
///
/// ```
/// use cache_rs::LruCache;
/// use core::num::NonZeroUsize;
///
/// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
///
/// // Add items to the cache
/// cache.put("apple", 1);
/// cache.put("banana", 2);
///
/// // Accessing items updates their recency
/// assert_eq!(cache.get(&"apple"), Some(&1));
///
/// // Adding beyond capacity evicts the least recently used item
/// // In this case "banana" is evicted because "apple" was just accessed
/// cache.put("cherry", 3);
/// assert_eq!(cache.get(&"banana"), None);
/// assert_eq!(cache.get(&"apple"), Some(&1));
/// assert_eq!(cache.get(&"cherry"), Some(&3));
/// ```
///
/// # Memory Usage
///
/// The memory usage of this cache is approximately:
/// - 16 bytes for the cache struct itself
/// - (32 + size_of(K) + size_of(V)) bytes per entry
/// - Plus HashMap overhead
///
/// # Examples
///
/// ```
/// use cache_rs::lru::LruCache;
/// use core::num::NonZeroUsize;
///
/// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
///
/// // Insert some items
/// cache.put("apple", 1);
/// cache.put("banana", 2);
///
/// // Most recently used is banana, then apple
/// assert_eq!(cache.get(&"apple"), Some(&1));
/// // Now most recently used is apple, then banana
///
/// // Add a new item, evicting the least recently used (banana)
/// cache.put("cherry", 3);
///
/// // Banana was evicted
/// assert_eq!(cache.get(&"banana"), None);
/// assert_eq!(cache.get(&"apple"), Some(&1));
/// assert_eq!(cache.get(&"cherry"), Some(&3));
/// ```
#[derive(Debug)]
pub struct LruCache<K, V, S = DefaultHashBuilder> {
    /// Configuration for the LRU cache
    config: LruCacheConfig,

    /// The internal list holding the key-value pairs in LRU order.  
    list: List<(K, V)>,

    /// A hash map mapping keys to pointers to the list entries.
    map: HashMap<K, *mut Entry<(K, V)>, S>,

    /// Metrics tracking for this cache instance.
    metrics: LruCacheMetrics,
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LruCache<K, V, S> {
    /// Creates a new LRU cache that holds at most `cap` items using the specified hash builder.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let cache: LruCache<&str, u32, _> = LruCache::with_hasher(
    ///     NonZeroUsize::new(10).unwrap(),
    ///     RandomState::new()
    /// );
    /// ```
    pub fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        // Default to a reasonable estimate: 1KB average object size
        Self::with_hasher_and_size(cap, hash_builder, cap.get() as u64 * 1024)
    }

    /// Creates a new LRU cache with specified capacity, hash builder, and size limit in bytes.
    pub fn with_hasher_and_size(cap: NonZeroUsize, hash_builder: S, max_size_bytes: u64) -> Self {
        let map_capacity = cap.get().next_power_of_two();
        let config = LruCacheConfig::new(cap);
        LruCache {
            config,
            list: List::new(cap),
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            metrics: LruCacheMetrics::new(max_size_bytes),
        }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let cache: LruCache<&str, u32> = LruCache::new(NonZeroUsize::new(10).unwrap());
    /// assert_eq!(cache.cap().get(), 10);
    /// ```
    pub fn cap(&self) -> NonZeroUsize {
        self.config.capacity()
    }

    /// Returns the number of key-value pairs in the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    /// assert_eq!(cache.len(), 0);
    ///
    /// cache.put("apple", 1);
    /// assert_eq!(cache.len(), 1);
    ///
    /// cache.put("banana", 2);
    /// assert_eq!(cache.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Returns `true` if the cache contains no key-value pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    /// assert!(cache.is_empty());
    ///
    /// cache.put("apple", 1);
    /// assert!(!cache.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Estimates the size of a key-value pair in bytes for metrics tracking
    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        // Simple estimation: key size + value size + overhead for pointers and metadata
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    /// Returns a reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// If a value is returned, that key-value pair becomes the most recently used pair in the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("apple", 1);
    /// cache.put("banana", 2);
    ///
    /// assert_eq!(cache.get(&"apple"), Some(&1));
    /// assert_eq!(cache.get(&"banana"), Some(&2));
    /// assert_eq!(cache.get(&"cherry"), None);
    /// ```
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(node) = self.map.get(key).copied() {
            // Cache hit
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our list
                self.list.move_to_front(node);
                let (k, v) = (*node).get_value();

                // Record hit with estimated object size
                let object_size = self.estimate_object_size(k, v);
                self.metrics.core.record_hit(object_size);

                Some(v)
            }
        } else {
            // Cache miss - we can't estimate size without the actual object
            // The simulation system will need to call record_miss separately
            None
        }
    }

    /// Records a cache miss for metrics tracking (to be called by simulation system)
    pub fn record_miss(&mut self, object_size: u64) {
        self.metrics.core.record_miss(object_size);
    }

    /// Returns a mutable reference to the value corresponding to the key.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// If a value is returned, that key-value pair becomes the most recently used pair in the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("apple", 1);
    ///
    /// if let Some(v) = cache.get_mut(&"apple") {
    ///     *v = 4;
    /// }
    ///
    /// assert_eq!(cache.get(&"apple"), Some(&4));
    /// ```
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let node = self.map.get(key).copied()?;

        // Move the node to the front of the list to mark it as most recently used
        // SAFETY: node comes from our map, so it's a valid pointer to an entry in our list
        unsafe {
            self.list.move_to_front(node);
            let (k, v) = (*node).get_value_mut();

            // Record hit with estimated object size
            let object_size = self.estimate_object_size(k, v);
            self.metrics.core.record_hit(object_size);

            Some(v)
        }
    }
}

impl<K: Hash + Eq + Clone, V, S: BuildHasher> LruCache<K, V, S> {
    /// Inserts a key-value pair into the cache.
    ///
    /// If the cache already contained this key, the old value is replaced and returned.
    /// Otherwise, if the cache is at capacity, the least recently used
    /// key-value pair will be evicted and returned.
    ///
    /// The inserted key-value pair becomes the most recently used pair in the cache.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    ///
    /// assert_eq!(cache.put("apple", 1), None); // No previous value
    /// assert_eq!(cache.put("apple", 4).unwrap().1, 1); // Replaced existing value
    ///
    /// // At capacity, adding new item evicts least recently used
    /// cache.put("banana", 2);
    /// assert_eq!(cache.put("cherry", 3).unwrap().1, 4); // Evicted "apple"
    /// assert_eq!(cache.get(&"apple"), None); // "apple" was evicted
    /// ```
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        V: Clone,
    {
        let mut evicted = None;
        let object_size = self.estimate_object_size(&key, &value);

        // If key is already in the cache
        if let Some(&node) = self.map.get(&key) {
            // SAFETY: node comes from our map, so it's a valid pointer to an entry in our list
            unsafe {
                self.list.move_to_front(node);
                let (k, old_value) = self.list.update(node, (key, value), true).0?;
                // This was an update, not a new insertion, so just track the size change
                return Some((k, old_value));
            }
        }

        // If we're at capacity, remove the least recently used item from the cache
        if self.map.len() >= self.cap().get() {
            if let Some(old_entry) = self.list.remove_last() {
                // Extract the key and remove it from the map
                unsafe {
                    // SAFETY: old_entry comes from list.remove_last(), so it's a valid Box
                    // that we own. Converting to raw pointer is safe for temporary access.
                    let entry_ptr = Box::into_raw(old_entry);
                    let key_ref = &(*entry_ptr).get_value().0;
                    self.map.remove(key_ref);
                    let key = (*entry_ptr).get_value().0.clone();
                    let value = (*entry_ptr).get_value().1.clone();

                    // Record eviction
                    let evicted_size = self.estimate_object_size(&key, &value);
                    self.metrics.core.record_eviction(evicted_size);

                    evicted = Some((key, value));
                    // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                    let _ = Box::from_raw(entry_ptr);
                }
            }
        }

        // Insert the new key-value pair at the front of the list
        if let Some(node) = self.list.add((key.clone(), value)) {
            // SAFETY: node comes from our list.add() call, so it's a valid pointer
            self.map.insert(key, node);

            // Record the insertion (increase in stored data)
            self.metrics.core.record_insertion(object_size);
        }

        evicted
    }

    /// Removes a key from the cache, returning the value at the key if the key was previously in the cache.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("apple", 1);
    ///
    /// assert_eq!(cache.remove(&"apple"), Some(1));
    /// assert_eq!(cache.remove(&"apple"), None);
    /// ```
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        let node = self.map.remove(key)?;

        unsafe {
            // SAFETY: node comes from our map and was just removed, so it's a valid pointer to an entry in our list
            // Get the key-value pair before removing
            let (k, v) = (*node).get_value();
            let object_size = self.estimate_object_size(k, v);
            let value = v.clone();

            // Detach the node from the list
            self.list.remove(node);

            // Record the removal (decrease in stored data)
            self.metrics.core.record_eviction(object_size);

            // Return the value (second element) from the tuple
            Some(value)
        }
    }

    /// Clears the cache, removing all key-value pairs.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("apple", 1);
    /// cache.put("banana", 2);
    ///
    /// assert!(!cache.is_empty());
    /// cache.clear();
    /// assert!(cache.is_empty());
    /// ```
    pub fn clear(&mut self) {
        // Reset cache size to zero
        self.metrics.core.cache_size_bytes = 0;

        self.map.clear();
        self.list.clear();
    }

    /// Returns an iterator over the cache's key-value pairs in least-recently-used to most-recently-used order.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    /// use std::prelude::v1::*;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(3).unwrap());
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.put("c", 3);
    ///
    /// // Access "a" to move it to the front
    /// assert_eq!(cache.get(&"a"), Some(&1));
    ///
    /// let items: Vec<_> = cache.iter().collect();
    /// assert_eq!(items, [(&"b", &2), (&"c", &3), (&"a", &1)]);
    /// ```
    pub fn iter(&self) -> Iter<'_, K, V> {
        // Not implemented here - this requires a more complex implementation to traverse
        // the linked list in reverse order
        unimplemented!("Iteration not yet implemented")
    }

    /// Returns a mutable iterator over the cache's key-value pairs in least-recently-used to most-recently-used order.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    /// use std::prelude::v1::*;
    ///
    /// let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    ///
    /// for (_, v) in cache.iter_mut() {
    ///     *v += 10;
    /// }
    ///
    /// assert_eq!(cache.get(&"a"), Some(&11));
    /// assert_eq!(cache.get(&"b"), Some(&12));
    /// ```
    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        // Not implemented here - this requires a more complex implementation to traverse
        // the linked list in reverse order
        unimplemented!("Mutable iteration not yet implemented")
    }
}

impl<K: Hash + Eq, V> LruCache<K, V>
where
    V: Clone,
{
    /// Creates a new LRU cache that holds at most `cap` items.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lru::LruCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let cache: LruCache<&str, u32> = LruCache::new(NonZeroUsize::new(10).unwrap());
    /// ```
    pub fn new(cap: NonZeroUsize) -> LruCache<K, V, DefaultHashBuilder> {
        LruCache::with_hasher(cap, DefaultHashBuilder::default())
    }
}

impl<K: Hash + Eq, V, S: BuildHasher> CacheMetrics for LruCache<K, V, S> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.metrics.metrics()
    }

    fn algorithm_name(&self) -> &'static str {
        self.metrics.algorithm_name()
    }
}

/// An iterator over the entries of a `LruCache` in least-recently-used to most-recently-used order.
///
/// This struct is created by the [`LruCache::iter`] method.
pub struct Iter<'a, K, V> {
    _marker: core::marker::PhantomData<&'a (K, V)>,
}

/// A mutable iterator over the entries of a `LruCache` in least-recently-used to most-recently-used order.
///
/// This struct is created by the [`LruCache::iter_mut`] method.
pub struct IterMut<'a, K, V> {
    _marker: core::marker::PhantomData<&'a mut (K, V)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn test_lru_get_put() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        // Insert items
        assert_eq!(cache.put("apple", 1), None);
        assert_eq!(cache.put("banana", 2), None);

        // Get existing items
        assert_eq!(cache.get(&"apple"), Some(&1));
        assert_eq!(cache.get(&"banana"), Some(&2));

        // Get non-existent item
        assert_eq!(cache.get(&"cherry"), None);

        // Replace existing item
        assert_eq!(cache.put("apple", 3).unwrap().1, 1);
        assert_eq!(cache.get(&"apple"), Some(&3));

        // Add a third item, should evict the least recently used (banana)
        assert_eq!(cache.put("cherry", 4).unwrap().1, 2);

        // Banana should be gone, apple and cherry should exist
        assert_eq!(cache.get(&"banana"), None);
        assert_eq!(cache.get(&"apple"), Some(&3));
        assert_eq!(cache.get(&"cherry"), Some(&4));
    }

    #[test]
    fn test_lru_get_mut() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("apple", 1);
        cache.put("banana", 2);

        // Modify value via get_mut
        if let Some(v) = cache.get_mut(&"apple") {
            *v = 3;
        }

        assert_eq!(cache.get(&"apple"), Some(&3));

        // Add a third item, should evict the least recently used (banana)
        // because apple was accessed most recently via get_mut
        cache.put("cherry", 4);

        assert_eq!(cache.get(&"banana"), None);
        assert_eq!(cache.get(&"apple"), Some(&3));
        assert_eq!(cache.get(&"cherry"), Some(&4));
    }

    #[test]
    fn test_lru_remove() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("apple", 1);
        cache.put("banana", 2);

        // Get existing items
        assert_eq!(cache.get(&"apple"), Some(&1));
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), None);

        // Remove item
        assert_eq!(cache.remove(&"apple"), Some(1));
        assert_eq!(cache.get(&"apple"), None);
        assert_eq!(cache.len(), 1);

        // Remove non-existent item
        assert_eq!(cache.remove(&"cherry"), None);

        // We should be able to add a new item now
        let evicted = cache.put("cherry", 3);
        assert_eq!(evicted, None);
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), Some(&3));
    }

    #[test]
    fn test_lru_clear() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("apple", 1);
        cache.put("banana", 2);

        assert_eq!(cache.len(), 2);
        cache.clear();
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());

        // Should be able to add new items after clearing
        cache.put("cherry", 3);
        assert_eq!(cache.get(&"cherry"), Some(&3));
    }

    #[test]
    fn test_lru_capacity_limits() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("apple", 1);
        cache.put("banana", 2);
        cache.put("cherry", 3);

        // Should only have the 2 most recently used items
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&"apple"), None); // evicted
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), Some(&3));
    }

    #[test]
    fn test_lru_string_keys() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        let key1 = String::from("apple");
        let key2 = String::from("banana");

        cache.put(key1.clone(), 1);
        cache.put(key2.clone(), 2);

        assert_eq!(cache.get(&key1), Some(&1));
        assert_eq!(cache.get(&key2), Some(&2));

        // String slice lookup should work too
        assert_eq!(cache.get("apple"), Some(&1));
        assert_eq!(cache.get("banana"), Some(&2));
    }

    #[derive(Debug, Clone, Eq, PartialEq)]
    struct ComplexValue {
        val: i32,
        description: String,
    }

    #[test]
    fn test_lru_complex_values() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

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

        // Lets add another item
        let evicted = cache.put(String::from("cherry"), fruit3.clone());
        let evicted_fruit = evicted.unwrap();
        assert_eq!(evicted_fruit.1, fruit1);

        // lets remove the first item
        let removed = cache.remove(&key1);
        assert_eq!(removed, None);
    }

    #[test]
    fn test_lru_metrics() {
        use crate::metrics::CacheMetrics;

        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());

        // Test initial metrics
        let metrics = cache.metrics();
        assert_eq!(metrics.get("requests").unwrap(), &0.0);
        assert_eq!(metrics.get("cache_hits").unwrap(), &0.0);
        assert_eq!(metrics.get("cache_misses").unwrap(), &0.0);

        // Add some items
        cache.put("apple", 1);
        cache.put("banana", 2);

        // Test cache hits
        cache.get(&"apple");
        cache.get(&"banana");

        let metrics = cache.metrics();
        assert_eq!(metrics.get("cache_hits").unwrap(), &2.0);

        // Test cache miss
        cache.record_miss(64); // Simulate a miss
        let metrics = cache.metrics();
        assert_eq!(metrics.get("cache_misses").unwrap(), &1.0);
        assert_eq!(metrics.get("requests").unwrap(), &3.0);

        // Test eviction by adding a third item
        cache.put("cherry", 3);
        let metrics = cache.metrics();
        assert_eq!(metrics.get("evictions").unwrap(), &1.0);

        // Test bytes_written_to_cache tracking
        assert!(metrics.get("bytes_written_to_cache").unwrap() > &0.0);

        // Test algorithm name
        assert_eq!(cache.algorithm_name(), "LRU");
    }
}
