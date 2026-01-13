//! Greedy Dual-Size Frequency (GDSF) cache implementation.
//!
//! GDSF is a sophisticated cache replacement algorithm that combines frequency, size,
//! and aging to optimize cache performance for variable-sized objects.
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
    frequency: u64,
    size: u64,
    priority: f64,
    node: *mut Entry<(K, V)>,
}

/// Internal GDSF segment containing the actual cache algorithm.
pub(crate) struct GdsfSegment<K, V, S = DefaultHashBuilder> {
    config: GdsfCacheConfig,
    global_age: f64,
    min_priority: f64,
    map: HashMap<K, EntryMetadata<K, V>, S>,
    priority_lists: BTreeMap<u64, List<(K, V)>>,
    metrics: GdsfCacheMetrics,
}

// SAFETY: GdsfSegment owns all data and raw pointers point only to nodes owned by
// `priority_lists`. Concurrent access is safe when wrapped in proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for GdsfSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for GdsfSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> GdsfSegment<K, V, S> {
    pub(crate) fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        let config = GdsfCacheConfig::new(cap);
        let map_capacity = config.capacity().get().next_power_of_two();
        let max_cache_size_bytes = config.capacity().get() as u64 * 128;

        GdsfSegment {
            config,
            global_age: config.initial_age(),
            min_priority: 0.0,
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            priority_lists: BTreeMap::new(),
            metrics: GdsfCacheMetrics::new(max_cache_size_bytes),
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

    unsafe fn update_priority_by_node(&mut self, node: *mut Entry<(K, V)>) -> *mut Entry<(K, V)>
    where
        K: Clone + Hash + Eq,
    {
        // SAFETY: node is guaranteed valid by caller's contract
        let (key_ref, _) = (*node).get_value();
        let key_cloned = key_ref.clone();

        let metadata = self.map.get_mut(&key_cloned).unwrap();
        let old_priority = metadata.priority;
        let size = metadata.size;

        metadata.frequency += 1;

        let global_age = self.global_age;
        let new_priority = if size == 0 {
            f64::INFINITY
        } else {
            (metadata.frequency as f64 / size as f64) + global_age
        };
        metadata.priority = new_priority;

        let old_priority_key = (old_priority * 1000.0) as u64;
        let new_priority_key = (new_priority * 1000.0) as u64;

        if old_priority_key == new_priority_key {
            let node = metadata.node;
            self.priority_lists
                .get_mut(&new_priority_key)
                .unwrap()
                .move_to_front(node);
            return node;
        }

        let node = metadata.node;
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

        metadata.node = entry_ptr;
        entry_ptr
    }

    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<V>
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

        if let Some(mut metadata) = self.map.remove(&key) {
            unsafe {
                // SAFETY: metadata.node comes from our map
                let old_priority_key = (metadata.priority * 1000.0) as u64;
                let list = self.priority_lists.get_mut(&old_priority_key).unwrap();
                let entry = list.remove(metadata.node).unwrap();

                if list.is_empty() {
                    self.priority_lists.remove(&old_priority_key);
                }

                let entry_ptr = Box::into_raw(entry);
                let (_, old_value) = (*entry_ptr).get_value().clone();
                let _ = Box::from_raw(entry_ptr);

                metadata.size = size;
                metadata.priority = self.calculate_priority(metadata.frequency, size);

                let new_priority_key = (metadata.priority * 1000.0) as u64;
                let capacity = self.cap();
                let list = self
                    .priority_lists
                    .entry(new_priority_key)
                    .or_insert_with(|| List::new(capacity));

                if let Some(new_node) = list.add((key.clone(), val)) {
                    metadata.node = new_node;
                    self.map.insert(key, metadata);
                    self.metrics.core.record_insertion(object_size);
                    return Some(old_value);
                } else {
                    return None;
                }
            }
        }

        while self.len() >= self.config.capacity().get() {
            self.evict_one();
        }

        let priority = self.calculate_priority(1, size);
        let priority_key = (priority * 1000.0) as u64;

        let capacity = self.config.capacity();
        let list = self
            .priority_lists
            .entry(priority_key)
            .or_insert_with(|| List::new(capacity));

        if let Some(entry) = list.add((key.clone(), val)) {
            let metadata = EntryMetadata {
                frequency: 1,
                size,
                priority,
                node: entry,
            };

            self.map.insert(key, metadata);

            if self.len() == 1 || priority < self.min_priority {
                self.min_priority = priority;
            }

            self.metrics.core.record_insertion(object_size);
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

        if let Some(entry) = list.remove_last() {
            unsafe {
                // SAFETY: entry comes from list.remove_last()
                let entry_ptr = Box::into_raw(entry);
                let (entry_key, _entry_value) = (*entry_ptr).get_value();

                let priority_to_update = if let Some(metadata) = self.map.get(entry_key) {
                    metadata.priority
                } else {
                    self.global_age
                };

                let estimated_size = mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64;

                self.metrics.core.record_eviction(estimated_size);
                self.metrics.record_size_based_eviction();
                self.metrics.record_aging_event(priority_to_update);

                self.global_age = priority_to_update;
                self.map.remove(entry_key);

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
        if let Some(metadata) = self.map.remove(key) {
            unsafe {
                // SAFETY: metadata.node comes from our map
                let priority_key = (metadata.priority * 1000.0) as u64;
                let list = self.priority_lists.get_mut(&priority_key).unwrap();
                let entry = list.remove(metadata.node).unwrap();

                if list.is_empty() {
                    self.priority_lists.remove(&priority_key);
                }

                let entry_ptr = Box::into_raw(entry);
                let (_, value) = (*entry_ptr).get_value();
                let result = value.clone();
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
    pub fn new(cap: NonZeroUsize) -> Self {
        let config = GdsfCacheConfig::new(cap);
        Self::with_hasher(config.capacity(), DefaultHashBuilder::default())
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

        let guard = cache.lock().unwrap();
        assert!(guard.len() <= 100);
    }
}
