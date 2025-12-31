//! Greedy Dual-Size Frequency (GDSF) cache implementation.
//!
//! GDSF is a sophisticated cache replacement algorithm that combines frequency, size,
//! and aging to optimize cache performance for variable-sized objects. It's particularly
//! well-suited for CDNs and web caches where objects have significantly different sizes.
//!
//! # Algorithm
//!
//! GDSF assigns a priority value to each cached item using the formula:
//!
//! ```text
//! Priority = (Frequency * Cost_Factor / Size) + Global_Age
//! ```
//!
//! Where:
//! - `Frequency`: Number of times the item has been accessed
//! - `Cost_Factor`: Optional weighting factor (defaults to 1.0)
//! - `Size`: Size of the item in bytes (or other cost units)
//! - `Global_Age`: A global aging factor that increases when items are evicted
//!
//! When the cache is full, the item with the lowest priority is evicted.
//! After eviction, the global age is set to the priority of the evicted item.
//!
//! # Performance Characteristics
//!
//! - **Time Complexity**:
//!   - Get: O(1)
//!   - Put: O(1)
//!   - Remove: O(1)
//!
//! - **Space Complexity**:
//!   - O(n) where n is the number of items
//!   - Higher overhead than LRU due to priority calculations
//!
//! # Size Considerations
//!
//! Unlike basic LRU or LFU caches, GDSF is size-aware and aims to maximize the hit ratio
//! per byte of cache used. This makes it particularly effective when:
//!
//! - Items vary significantly in size
//! - Storage space is at a premium
//! - You want to optimize for hit rate relative to storage cost
//!
//! # CDN Use Cases
//!
//! GDSF is ideal for Content Delivery Networks because it:
//! - Favors keeping small, frequently accessed items (e.g., thumbnails, icons)
//! - Can intelligently evict large, infrequently accessed items (e.g., large videos)
//! - Adapts to changing access patterns through the aging mechanism
//!
//! # Thread Safety
//!
//! This implementation is not thread-safe. For concurrent access, wrap the cache
//! with a synchronization primitive such as `Mutex` or `RwLock`.

extern crate alloc;

use crate::config::GdsfCacheConfig;
use crate::list::{Entry, List};
use crate::metrics::{CacheMetrics, GdsfCacheMetrics};
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

/// Metadata for each cache entry in GDSF
#[derive(Debug, Clone, Copy)]
struct EntryMetadata<K, V> {
    /// Frequency of access for this item
    frequency: u64,
    /// Size of this item
    size: u64,
    /// Calculated priority (frequency/size + global_age)
    priority: f64,
    /// Pointer to the entry in the priority list
    node: *mut Entry<(K, V)>,
}

/// An implementation of a Greedy Dual-Size Frequency (GDSF) cache.
///
/// GDSF combines frequency, size, and aging to make eviction decisions.
/// Items with higher frequency-to-size ratios and recent access patterns
/// are prioritized for retention in the cache.
///
/// # Examples
///
/// ```
/// use cache_rs::gdsf::GdsfCache;
/// use core::num::NonZeroUsize;
///
/// // Create a GDSF cache with capacity 3
/// let mut cache = GdsfCache::new(NonZeroUsize::new(3).unwrap());
///
/// // Add items with different sizes
/// cache.put("small", 1, 1);  // key="small", value=1, size=1
/// cache.put("large", 2, 5);  // key="large", value=2, size=5
/// cache.put("medium", 3, 3); // key="medium", value=3, size=3
///
/// // Access items to increase their frequency
/// assert_eq!(cache.get(&"small"), Some(1));
/// assert_eq!(cache.get(&"small"), Some(1)); // Higher frequency
/// ```
#[derive(Debug)]
pub struct GdsfCache<K, V, S = DefaultHashBuilder> {
    /// Configuration for the GDSF cache
    config: GdsfCacheConfig,

    /// Global age value that increases when items are evicted
    global_age: f64,

    /// Current minimum priority in the cache
    min_priority: f64,

    /// Map from keys to their metadata
    map: HashMap<K, EntryMetadata<K, V>, S>,

    /// Map from priority to list of items with that priority
    /// Items within each priority list are ordered by recency (LRU within priority)
    priority_lists: BTreeMap<u64, List<(K, V)>>,

    /// Metrics tracking for this cache instance
    metrics: GdsfCacheMetrics,
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> GdsfCache<K, V, S> {
    /// Creates a new GDSF cache with the specified capacity and hash builder.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::gdsf::GdsfCache;
    /// use core::num::NonZeroUsize;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let cache: GdsfCache<&str, u32, _> = GdsfCache::with_hasher(
    ///     NonZeroUsize::new(10).unwrap(),
    ///     RandomState::new()
    /// );
    /// ```
    pub fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        let config = GdsfCacheConfig::new(cap);
        let map_capacity = config.capacity().get().next_power_of_two();
        let max_cache_size_bytes = config.capacity().get() as u64 * 128; // Estimate based on capacity

        GdsfCache {
            config,
            global_age: config.initial_age(),
            min_priority: 0.0,
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            priority_lists: BTreeMap::new(),
            metrics: GdsfCacheMetrics::new(max_cache_size_bytes),
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
    pub fn global_age(&self) -> f64 {
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

    /// Calculates the priority for an item based on GDSF formula.
    fn calculate_priority(&self, frequency: u64, size: u64) -> f64 {
        if size == 0 {
            return f64::INFINITY; // Protect against division by zero
        }
        (frequency as f64 / size as f64) + self.global_age
    }

    /// Updates the priority of an item and moves it to the appropriate priority list.
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
        let old_priority = metadata.priority;
        let size = metadata.size;

        // Increment frequency
        metadata.frequency += 1;

        // Calculate new priority outside of borrowing context
        let global_age = self.global_age;
        let new_priority = if size == 0 {
            f64::INFINITY
        } else {
            (metadata.frequency as f64 / size as f64) + global_age
        };
        metadata.priority = new_priority;

        // Convert priority to integer key for BTreeMap (multiply by 1000 for precision)
        let old_priority_key = (old_priority * 1000.0) as u64;
        let new_priority_key = (new_priority * 1000.0) as u64;

        // If priority hasn't changed significantly, just move to front of the same list
        if old_priority_key == new_priority_key {
            let node = metadata.node;
            self.priority_lists
                .get_mut(&new_priority_key)
                .unwrap()
                .move_to_front(node);
            return node;
        }

        // Remove from old priority list
        let node = metadata.node;
        let boxed_entry = self
            .priority_lists
            .get_mut(&old_priority_key)
            .unwrap()
            .remove(node)
            .unwrap();

        // Clean up empty list
        if self
            .priority_lists
            .get(&old_priority_key)
            .unwrap()
            .is_empty()
        {
            self.priority_lists.remove(&old_priority_key);
        }

        // Add to new priority list
        let entry_ptr = Box::into_raw(boxed_entry);

        // Ensure the new priority list exists
        let capacity = self.config.capacity();
        self.priority_lists
            .entry(new_priority_key)
            .or_insert_with(|| List::new(capacity));

        // Add to the front of the new priority list (most recently used within that priority)
        self.priority_lists
            .get_mut(&new_priority_key)
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
    pub fn get<Q>(&mut self, key: &Q) -> Option<V>
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

                // Record GDSF-specific item access metrics
                self.metrics.record_item_access(
                    metadata.frequency,
                    metadata.size,
                    metadata.priority,
                );

                let new_node = self.update_priority_by_node(node);
                let (_, value) = (*new_node).get_value();
                Some(value.clone())
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
                let (key_ref, value) = (*node).get_value();

                // Record hit with object size estimation
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.core.record_hit(object_size);

                // Record GDSF-specific item access metrics
                self.metrics.record_item_access(
                    metadata.frequency,
                    metadata.size,
                    metadata.priority,
                );

                let new_node = self.update_priority_by_node(node);
                let (_, value) = (*new_node).get_value_mut();
                Some(value)
            }
        } else {
            None
        }
    }

    /// Returns `true` if the cache contains a value for the specified key.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    ///
    /// This does not modify the cache or update access frequency.
    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.map.contains_key(key)
    }

    /// Inserts a key-value pair with the specified size into the cache.
    ///
    /// If the key already exists, the value is updated and the old value is returned.
    /// If the cache is full, items are evicted based on GDSF priority until there's space.
    ///
    /// # Arguments
    ///
    /// * `key` - The key to insert
    /// * `val` - The value to associate with the key
    /// * `size` - The size/cost of storing this item (must be > 0)
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::gdsf::GdsfCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());
    ///
    /// cache.put("key1", "value1", 1);
    /// cache.put("key2", "value2", 2);
    /// assert_eq!(cache.len(), 2);
    /// ```
    pub fn put(&mut self, key: K, val: V, size: u64) -> Option<V>
    where
        K: Clone,
    {
        if size == 0 {
            // Don't allow zero-sized items as they would have infinite priority
            return None;
        }

        let object_size = self.estimate_object_size(&key, &val);

        // Check if key already exists
        if let Some(mut metadata) = self.map.remove(&key) {
            // Update existing entry
            unsafe {
                // SAFETY: metadata.node comes from our map and was just removed, so priority_lists.remove is safe
                let old_priority_key = (metadata.priority * 1000.0) as u64;

                // Remove from current list
                let list = self.priority_lists.get_mut(&old_priority_key).unwrap();
                let entry = list.remove(metadata.node).unwrap();

                // Clean up empty list
                if list.is_empty() {
                    self.priority_lists.remove(&old_priority_key);
                }

                // SAFETY: entry is a valid Box we own, converting to raw pointer for temporary access
                // Extract old value
                let entry_ptr = Box::into_raw(entry);
                let (_, old_value) = (*entry_ptr).get_value().clone();

                // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                // Free the old entry
                let _ = Box::from_raw(entry_ptr);

                // Update metadata with new size but same frequency
                metadata.size = size;
                metadata.priority = self.calculate_priority(metadata.frequency, size);

                // Create new entry and add to appropriate list
                let new_priority_key = (metadata.priority * 1000.0) as u64;
                let capacity = self.cap();
                let list = self
                    .priority_lists
                    .entry(new_priority_key)
                    .or_insert_with(|| List::new(capacity));

                if let Some(new_node) = list.add((key.clone(), val)) {
                    metadata.node = new_node;
                    self.map.insert(key, metadata);

                    // Record insertion (update)
                    self.metrics.core.record_insertion(object_size);

                    return Some(old_value);
                } else {
                    // This shouldn't happen since we just made space
                    return None;
                }
            }
        }

        // Make space if needed
        while self.len() >= self.config.capacity().get() {
            self.evict_one();
        }

        // Calculate priority for new item (frequency starts at 1)
        let priority = self.calculate_priority(1, size);
        let priority_key = (priority * 1000.0) as u64;

        // Add to appropriate priority list
        let capacity = self.config.capacity();
        let list = self
            .priority_lists
            .entry(priority_key)
            .or_insert_with(|| List::new(capacity));

        if let Some(entry) = list.add((key.clone(), val)) {
            // Update metadata
            let metadata = EntryMetadata {
                frequency: 1,
                size,
                priority,
                node: entry,
            };

            self.map.insert(key, metadata);

            // Update min_priority if this is the first item or lower
            if self.len() == 1 || priority < self.min_priority {
                self.min_priority = priority;
            }

            // Record insertion and GDSF-specific metrics
            self.metrics.core.record_insertion(object_size);
            self.metrics
                .record_item_cached(size, self.metrics.average_item_size());
            self.metrics.record_item_access(1, size, priority);

            None
        } else {
            // List is full, shouldn't happen since we made space
            None
        }
    }

    /// Evicts one item from the cache based on GDSF priority.
    /// Items with lower priority are evicted first.
    fn evict_one(&mut self) {
        if self.is_empty() {
            return;
        }

        // Find the list with minimum priority
        let min_priority_key = *self.priority_lists.keys().next().unwrap();

        // Remove the least recently used item from the minimum priority list
        let list = self.priority_lists.get_mut(&min_priority_key).unwrap();
        if let Some(entry) = list.remove_last() {
            unsafe {
                // SAFETY: entry comes from list.remove_last(), so it's a valid Box
                // that we own. Converting to raw pointer is safe for temporary access.
                let entry_ptr = Box::into_raw(entry);
                let (entry_key, _entry_value) = (*entry_ptr).get_value();

                // Get the priority before removing from map for metrics
                let priority_to_update = if let Some(metadata) = self.map.get(entry_key) {
                    metadata.priority
                } else {
                    self.global_age // fallback
                };

                // Estimate object size before moving values
                let estimated_size = mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64;

                // Record eviction with estimated object size and GDSF-specific metrics
                self.metrics.core.record_eviction(estimated_size);
                self.metrics.record_size_based_eviction();
                self.metrics.record_aging_event(priority_to_update);

                // Update global age to the evicted item's priority
                self.global_age = priority_to_update;

                // Remove from map using the owned key
                self.map.remove(entry_key);

                // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                // Free the entry
                let _ = Box::from_raw(entry_ptr);
            }
        }

        // Clean up empty list
        if list.is_empty() {
            self.priority_lists.remove(&min_priority_key);
        }
    }

    /// Removes a key from the cache, returning the value if the key was in the cache.
    ///
    /// The key may be any borrowed form of the cache's key type, but
    /// [`Hash`] and [`Eq`] on the borrowed form *must* match those for
    /// the key type.
    pub fn pop<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(metadata) = self.map.remove(key) {
            unsafe {
                // SAFETY: metadata.node comes from our map and was just removed, so priority_lists.remove is safe
                let priority_key = (metadata.priority * 1000.0) as u64;
                let list = self.priority_lists.get_mut(&priority_key).unwrap();
                let entry = list.remove(metadata.node).unwrap();

                // Clean up empty list
                if list.is_empty() {
                    self.priority_lists.remove(&priority_key);
                }

                // SAFETY: entry is a valid Box we own, converting to raw pointer for temporary access
                let entry_ptr = Box::into_raw(entry);
                let (_, value) = (*entry_ptr).get_value();
                let result = value.clone();

                // SAFETY: Reconstructing Box from the same raw pointer to properly free memory
                // Free the entry
                let _ = Box::from_raw(entry_ptr);

                Some(result)
            }
        } else {
            None
        }
    }

    /// Clears the cache, removing all key-value pairs.
    pub fn clear(&mut self) {
        self.map.clear();
        self.priority_lists.clear();
        self.global_age = 0.0;
        self.min_priority = 0.0;
    }
}

impl<K, V, S> CacheMetrics for GdsfCache<K, V, S> {
    /// Returns all GDSF cache metrics as key-value pairs in deterministic order
    ///
    /// # Returns
    /// A BTreeMap containing all metrics tracked by this GDSF cache instance
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.metrics.metrics()
    }

    /// Returns the algorithm name for this cache implementation
    ///
    /// # Returns
    /// "GDSF" - identifying this as a Greedy Dual-Size Frequency cache
    fn algorithm_name(&self) -> &'static str {
        self.metrics.algorithm_name()
    }
}

impl<K: Hash + Eq, V: Clone> GdsfCache<K, V, DefaultHashBuilder> {
    /// Creates a new GDSF cache with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::gdsf::GdsfCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let cache: GdsfCache<&str, u32> = GdsfCache::new(NonZeroUsize::new(10).unwrap());
    /// ```
    pub fn new(cap: NonZeroUsize) -> Self {
        let config = GdsfCacheConfig::new(cap);
        Self::with_hasher(config.capacity(), DefaultHashBuilder::default())
    }
}

#[cfg(test)]
mod tests {
    use super::GdsfCache;
    use core::num::NonZeroUsize;

    #[test]
    fn test_gdsf_basic_operations() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(3).unwrap());

        // Test insertion
        assert_eq!(cache.put("a", 1, 1), None);
        assert_eq!(cache.put("b", 2, 2), None);
        assert_eq!(cache.put("c", 3, 1), None);
        assert_eq!(cache.len(), 3);

        // Test retrieval
        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));

        // Test contains_key
        assert!(cache.contains_key(&"a"));
        assert!(!cache.contains_key(&"d"));
    }

    #[test]
    fn test_gdsf_frequency_priority() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        // Insert items with same size
        cache.put("a", 1, 1);
        cache.put("b", 2, 1);

        // Access "a" more frequently
        cache.get(&"a");
        cache.get(&"a");

        // Add new item, "b" should be evicted due to lower frequency
        cache.put("c", 3, 1);

        assert!(cache.contains_key(&"a"));
        assert!(!cache.contains_key(&"b"));
        assert!(cache.contains_key(&"c"));
    }

    #[test]
    fn test_gdsf_size_consideration() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        // Insert items with different sizes but same access patterns
        cache.put("small", 1, 1); // frequency/size = 1/1 = 1.0
        cache.put("large", 2, 10); // frequency/size = 1/10 = 0.1

        // Add new item, "large" should be evicted due to lower priority
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

        // Update with new value and size
        assert_eq!(cache.put("key", 2, 2), Some(1));
        assert_eq!(cache.get(&"key"), Some(2));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn test_gdsf_zero_size_rejection() {
        let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());

        // Zero-sized items should be rejected
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

        // Force eviction
        cache.put("c", 3, 1);

        // Global age should have increased
        assert!(cache.global_age() > initial_age);
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
        let mut cache = GdsfCache::new(NonZeroUsize::new(10).unwrap());

        // Insert some items
        cache.put("a", 1, 10);
        cache.put("b", 2, 20);
        cache.put("c", 3, 15);

        // Access items multiple times to trigger priority updates
        // This would fail under Miri with the original buggy code
        for _ in 0..3 {
            assert_eq!(cache.get(&"a"), Some(1));
            assert_eq!(cache.get(&"b"), Some(2));
            assert_eq!(cache.get(&"c"), Some(3));
        }

        assert_eq!(cache.len(), 3);

        // Test with get_mut as well
        if let Some(val) = cache.get_mut(&"a") {
            *val += 10;
        }
        assert_eq!(cache.get(&"a"), Some(11));
    }
}
