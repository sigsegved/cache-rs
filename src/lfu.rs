//! Least Frequently Used Cache Implementation.
//!
//! The LFU (Least Frequently Used) cache evicts the least frequently accessed items
//! when the cache reaches capacity. This implementation tracks the frequency of
//! access for each item and maintains items sorted by frequency.
//!
//! This implementation provides better performance for workloads where certain
//! items are accessed more frequently than others over time, as it protects
//! frequently accessed items from eviction.

extern crate alloc;

use crate::config::LfuCacheConfig;
use crate::list::{Entry, List};
use crate::metrics::{CacheMetrics, LfuCacheMetrics};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use core::borrow::Borrow;
use core::hash::{BuildHasher, Hash};
use core::mem;
use core::num::NonZeroUsize;

/// Type alias for the frequency metadata stored in the hash map
type FrequencyMetadata<K, V> = (usize, *mut Entry<(K, V)>);

#[cfg(feature = "hashbrown")]
use hashbrown::hash_map::DefaultHashBuilder;
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
/// # Safety
///
/// This struct contains raw pointers in the `map` field. These pointers
/// are always valid as long as:
/// - The pointer was obtained from a `frequency_lists` entry's `add()` call
/// - The node has not been removed from the list
/// - The segment has not been dropped
pub(crate) struct LfuSegment<K, V, S = DefaultHashBuilder> {
    /// Configuration for the LFU cache
    config: LfuCacheConfig,

    /// Current minimum frequency in the cache
    min_frequency: usize,

    /// Map from keys to their frequency and list node
    map: HashMap<K, FrequencyMetadata<K, V>, S>,

    /// Map from frequency to list of items with that frequency
    /// Items within each frequency list are ordered by recency (LRU within frequency)
    frequency_lists: BTreeMap<usize, List<(K, V)>>,

    /// Metrics for tracking cache performance and frequency distribution
    metrics: LfuCacheMetrics,
}

// SAFETY: LfuSegment owns all data and raw pointers point only to nodes owned by
// `frequency_lists`. Concurrent access is safe when wrapped in proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for LfuSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for LfuSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LfuSegment<K, V, S> {
    /// Creates a new LFU segment with the specified capacity and hash builder.
    pub(crate) fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        let config = LfuCacheConfig::new(cap);
        let map_capacity = config.capacity().get().next_power_of_two();
        let max_cache_size_bytes = config.capacity().get() as u64 * 128;
        LfuSegment {
            config,
            min_frequency: 1,
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            frequency_lists: BTreeMap::new(),
            metrics: LfuCacheMetrics::new(max_cache_size_bytes),
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
    /// The caller must ensure that `node` is a valid pointer to an Entry that exists
    /// in this cache's frequency lists and has not been freed.
    unsafe fn update_frequency_by_node(
        &mut self,
        node: *mut Entry<(K, V)>,
        old_frequency: usize,
    ) -> *mut Entry<(K, V)>
    where
        K: Clone + Hash + Eq,
    {
        let new_frequency = old_frequency + 1;

        // Record frequency increment
        self.metrics
            .record_frequency_increment(old_frequency, new_frequency);

        // SAFETY: node is guaranteed to be valid by the caller's contract
        let (key_ref, _) = (*node).get_value();
        let key_cloned = key_ref.clone();

        // Get the current node from the old frequency list
        let (_, node) = self.map.get(&key_cloned).unwrap();

        // Remove from old frequency list
        let boxed_entry = self
            .frequency_lists
            .get_mut(&old_frequency)
            .unwrap()
            .remove(*node)
            .unwrap();

        // If the old frequency list is now empty and it was the minimum frequency,
        // update the minimum frequency
        if self.frequency_lists.get(&old_frequency).unwrap().is_empty()
            && old_frequency == self.min_frequency
        {
            self.min_frequency = new_frequency;
        }

        // Add to new frequency list
        let entry_ptr = Box::into_raw(boxed_entry);

        // Ensure the new frequency list exists
        let capacity = self.config.capacity();
        self.frequency_lists
            .entry(new_frequency)
            .or_insert_with(|| List::new(capacity));

        // Add to the front of the new frequency list (most recently used within that frequency)
        self.frequency_lists
            .get_mut(&new_frequency)
            .unwrap()
            .attach_from_other_list(entry_ptr);

        // Update the map
        self.map.get_mut(&key_cloned).unwrap().0 = new_frequency;
        self.map.get_mut(&key_cloned).unwrap().1 = entry_ptr;

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
        if let Some(&(frequency, node)) = self.map.get(key) {
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our frequency list
                let (key_ref, value) = (*node).get_value();
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.record_frequency_hit(object_size, frequency);

                let new_node = self.update_frequency_by_node(node, frequency);
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
        if let Some(&(frequency, node)) = self.map.get(key) {
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our frequency list
                let (key_ref, value) = (*node).get_value();
                let object_size = self.estimate_object_size(key_ref, value);
                self.metrics.record_frequency_hit(object_size, frequency);

                let new_node = self.update_frequency_by_node(node, frequency);
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
        if let Some(&(frequency, node)) = self.map.get(&key) {
            unsafe {
                // SAFETY: node comes from our map, so it's a valid pointer to an entry in our frequency list
                let old_entry = self.frequency_lists.get_mut(&frequency).unwrap().update(
                    node,
                    (key.clone(), value),
                    true,
                );

                self.metrics.core.record_insertion(object_size);
                return old_entry.0;
            }
        }

        let mut evicted = None;

        // If at capacity, evict the least frequently used item
        if self.len() >= self.config.capacity().get() {
            if let Some(min_freq_list) = self.frequency_lists.get_mut(&self.min_frequency) {
                if let Some(old_entry) = min_freq_list.remove_last() {
                    unsafe {
                        let entry_ptr = Box::into_raw(old_entry);
                        let (old_key, old_value) = (*entry_ptr).get_value().clone();
                        let evicted_size = self.estimate_object_size(&old_key, &old_value);
                        self.metrics.core.record_eviction(evicted_size);
                        self.map.remove(&old_key);
                        evicted = Some((old_key, old_value));
                        let _ = Box::from_raw(entry_ptr);
                    }
                }
            }
        }

        // Add new item with frequency 1
        let frequency = 1;
        self.min_frequency = 1;

        // Ensure frequency list exists
        let capacity = self.config.capacity();
        self.frequency_lists
            .entry(frequency)
            .or_insert_with(|| List::new(capacity));

        if let Some(node) = self
            .frequency_lists
            .get_mut(&frequency)
            .unwrap()
            .add((key.clone(), value))
        {
            self.map.insert(key, (frequency, node));
        }

        self.metrics.core.record_insertion(object_size);
        self.metrics.update_frequency_levels(&self.frequency_lists);

        evicted
    }

    /// Removes a key from the segment, returning the value if the key was present.
    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        let (frequency, node) = self.map.remove(key)?;

        unsafe {
            // SAFETY: node comes from our map and was just removed
            let boxed_entry = self.frequency_lists.get_mut(&frequency)?.remove(node)?;
            let entry_ptr = Box::into_raw(boxed_entry);
            let value = (*entry_ptr).get_value().1.clone();
            let _ = Box::from_raw(entry_ptr);

            // Update min_frequency if necessary
            if self.frequency_lists.get(&frequency).unwrap().is_empty()
                && frequency == self.min_frequency
            {
                self.min_frequency = self
                    .frequency_lists
                    .keys()
                    .find(|&&f| f > frequency && !self.frequency_lists.get(&f).unwrap().is_empty())
                    .copied()
                    .unwrap_or(1);
            }

            Some(value)
        }
    }

    /// Clears the segment, removing all key-value pairs.
    pub(crate) fn clear(&mut self) {
        self.map.clear();
        self.frequency_lists.clear();
        self.min_frequency = 1;
    }

    /// Removes and returns the eviction candidate (least frequently used item).
    ///
    /// For LFU, this is the item with the lowest frequency. If there are multiple
    /// items with the same lowest frequency, the least recently used among them
    /// is returned.
    ///
    /// Returns `None` if the cache is empty.
    pub(crate) fn pop(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        if self.is_empty() {
            return None;
        }

        let min_frequency = self.min_frequency;
        let min_freq_list = self.frequency_lists.get_mut(&min_frequency)?;
        let old_entry = min_freq_list.remove_last()?;
        let is_list_empty = min_freq_list.is_empty();

        unsafe {
            // SAFETY: entry comes from frequency_lists.remove_last()
            let entry_ptr = Box::into_raw(old_entry);
            let (key, value) = (*entry_ptr).get_value().clone();
            let object_size = mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64;
            self.map.remove(&key);
            self.metrics.core.record_eviction(object_size);

            // Update min_frequency if the list is now empty
            if is_list_empty {
                self.frequency_lists.remove(&min_frequency);
                self.min_frequency = self.frequency_lists.keys().copied().next().unwrap_or(1);
            }

            let _ = Box::from_raw(entry_ptr);
            Some((key, value))
        }
    }

    /// Removes and returns the item with the highest frequency (reverse of pop).
    ///
    /// This is the opposite of `pop()` - instead of returning the lowest frequency item,
    /// it returns the highest frequency item. If there are multiple items with the same
    /// highest frequency, the most recently used among them is returned.
    ///
    /// Returns `None` if the cache is empty.
    pub(crate) fn popr(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        if self.is_empty() {
            return None;
        }

        // Get the highest frequency (last key in BTreeMap)
        let max_frequency = *self.frequency_lists.keys().next_back()?;
        let max_freq_list = self.frequency_lists.get_mut(&max_frequency)?;
        let entry = max_freq_list.remove_first()?;
        let is_list_empty = max_freq_list.is_empty();

        unsafe {
            // SAFETY: entry comes from frequency_lists.remove_first()
            let entry_ptr = Box::into_raw(entry);
            let (key, value) = (*entry_ptr).get_value().clone();
            let object_size = mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64;
            self.map.remove(&key);
            self.metrics.core.record_eviction(object_size);

            // Remove empty frequency list
            if is_list_empty {
                self.frequency_lists.remove(&max_frequency);
            }

            let _ = Box::from_raw(entry_ptr);
            Some((key, value))
        }
    }

    /// Records a cache miss for metrics tracking
    #[inline]
    pub(crate) fn record_miss(&mut self, object_size: u64) {
        self.metrics.record_miss(object_size);
    }
}

// Implement Debug for LfuSegment manually since it contains raw pointers
impl<K, V, S> core::fmt::Debug for LfuSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LfuSegment")
            .field("capacity", &self.config.capacity())
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
/// use core::num::NonZeroUsize;
///
/// // Create an LFU cache with capacity 3
/// let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());
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
    /// Creates a new LFU cache with the specified capacity and hash builder.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfu::LfuCache;
    /// use core::num::NonZeroUsize;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let cache: LfuCache<&str, u32, _> = LfuCache::with_hasher(
    ///     NonZeroUsize::new(10).unwrap(),
    ///     RandomState::new()
    /// );
    /// ```
    pub fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self {
            segment: LfuSegment::with_hasher(cap, hash_builder),
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
    #[inline]
    pub fn clear(&mut self) {
        self.segment.clear()
    }

    /// Removes and returns the eviction candidate (least frequently used item).
    ///
    /// For LFU, this is the item with the lowest frequency. If there are multiple
    /// items with the same lowest frequency, the least recently used among them
    /// is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfu::LfuCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.get(&"b"); // Increase frequency of "b"
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

    /// Removes and returns the item with the highest frequency (reverse of pop).
    ///
    /// This is the opposite of `pop()` - instead of returning the lowest frequency item,
    /// it returns the highest frequency item.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfu::LfuCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());
    /// cache.put("a", 1);
    /// cache.put("b", 2);
    /// cache.get(&"b"); // Increase frequency of "b"
    /// cache.get(&"b"); // Increase frequency again
    ///
    /// // Pop the highest frequency item
    /// assert_eq!(cache.popr(), Some(("b", 2)));
    /// ```
    #[inline]
    pub fn popr(&mut self) -> Option<(K, V)>
    where
        K: Clone,
    {
        self.segment.popr()
    }

    /// Records a cache miss for metrics tracking (to be called by simulation system)
    #[inline]
    pub fn record_miss(&mut self, object_size: u64) {
        self.segment.record_miss(object_size);
    }
}

impl<K: Hash + Eq, V> LfuCache<K, V>
where
    V: Clone,
{
    /// Creates a new LFU cache with the specified capacity.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::lfu::LfuCache;
    /// use core::num::NonZeroUsize;
    ///
    /// let cache: LfuCache<&str, u32> = LfuCache::new(NonZeroUsize::new(10).unwrap());
    /// ```
    pub fn new(cap: NonZeroUsize) -> LfuCache<K, V, DefaultHashBuilder> {
        let config = LfuCacheConfig::new(cap);
        LfuCache::with_hasher(config.capacity(), DefaultHashBuilder::default())
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
    use std::string::ToString;

    use super::*;
    use alloc::string::String;

    #[test]
    fn test_lfu_basic() {
        let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());

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
        let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());

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
        let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());

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
        let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());

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
        let mut cache = LfuCache::new(NonZeroUsize::new(3).unwrap());

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
        let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());

        cache.put("a", 1);

        // Modify value using get_mut
        if let Some(value) = cache.get_mut(&"a") {
            *value = 10;
        }

        assert_eq!(cache.get(&"a"), Some(&10));
    }

    #[test]
    fn test_lfu_complex_values() {
        let mut cache = LfuCache::new(NonZeroUsize::new(2).unwrap());

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
        let mut cache = LfuCache::new(NonZeroUsize::new(10).unwrap());

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
        let mut segment: LfuSegment<&str, i32, DefaultHashBuilder> =
            LfuSegment::with_hasher(NonZeroUsize::new(3).unwrap(), DefaultHashBuilder::default());

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

        let cache = Arc::new(Mutex::new(LfuCache::new(NonZeroUsize::new(100).unwrap())));
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
}
