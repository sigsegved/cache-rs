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

/// Internal LRU segment containing the actual cache algorithm.
///
/// This is shared between `LruCache` (single-threaded) and
/// `ConcurrentLruCache` (multi-threaded). All algorithm logic is
/// implemented here to avoid code duplication.
///
/// # Safety
///
/// This struct contains raw pointers in the `map` field.
/// These pointers are always valid as long as:
/// - The pointer was obtained from a `list` entry's `add()` call
/// - The node has not been removed from the list
/// - The segment has not been dropped
pub(crate) struct LruSegment<K, V, S = DefaultHashBuilder> {
    config: LruCacheConfig,
    list: List<(K, V)>,
    map: HashMap<K, *mut Entry<(K, V)>, S>,
    metrics: LruCacheMetrics,
}

// SAFETY: LruSegment owns all data and raw pointers point only to nodes owned by `list`.
// Concurrent access is safe when wrapped in proper synchronization primitives.
unsafe impl<K: Send, V: Send, S: Send> Send for LruSegment<K, V, S> {}

// SAFETY: All mutation requires &mut self; shared references cannot cause data races.
unsafe impl<K: Send, V: Send, S: Sync> Sync for LruSegment<K, V, S> {}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LruSegment<K, V, S> {
    pub(crate) fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self::with_hasher_and_size(cap, hash_builder, cap.get() as u64 * 1024)
    }

    pub(crate) fn with_hasher_and_size(
        cap: NonZeroUsize,
        hash_builder: S,
        max_size_bytes: u64,
    ) -> Self {
        let map_capacity = cap.get().next_power_of_two();
        let config = LruCacheConfig::new(cap);
        LruSegment {
            config,
            list: List::new(cap),
            map: HashMap::with_capacity_and_hasher(map_capacity, hash_builder),
            metrics: LruCacheMetrics::new(max_size_bytes),
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
    pub(crate) fn metrics(&self) -> &LruCacheMetrics {
        &self.metrics
    }

    fn estimate_object_size(&self, _key: &K, _value: &V) -> u64 {
        mem::size_of::<K>() as u64 + mem::size_of::<V>() as u64 + 64
    }

    pub(crate) fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        if let Some(node) = self.map.get(key).copied() {
            unsafe {
                // SAFETY: node comes from our map
                self.list.move_to_front(node);
                let (k, v) = (*node).get_value();
                let object_size = self.estimate_object_size(k, v);
                self.metrics.core.record_hit(object_size);
                Some(v)
            }
        } else {
            None
        }
    }

    #[inline]
    pub(crate) fn record_miss(&mut self, object_size: u64) {
        self.metrics.core.record_miss(object_size);
    }

    pub(crate) fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        let node = self.map.get(key).copied()?;
        unsafe {
            // SAFETY: node comes from our map
            self.list.move_to_front(node);
            let (k, v) = (*node).get_value_mut();
            let object_size = self.estimate_object_size(k, v);
            self.metrics.core.record_hit(object_size);
            Some(v)
        }
    }

    pub(crate) fn put(&mut self, key: K, value: V) -> Option<(K, V)>
    where
        K: Clone,
    {
        let mut evicted = None;
        let object_size = self.estimate_object_size(&key, &value);

        if let Some(&node) = self.map.get(&key) {
            unsafe {
                // SAFETY: node comes from our map
                self.list.move_to_front(node);
                let (k, old_value) = self.list.update(node, (key, value), true).0?;
                return Some((k, old_value));
            }
        }

        if self.map.len() >= self.cap().get() {
            if let Some(old_entry) = self.list.remove_last() {
                unsafe {
                    let entry_ptr = Box::into_raw(old_entry);
                    let key_ref = &(*entry_ptr).get_value().0;
                    self.map.remove(key_ref);
                    let key = (*entry_ptr).get_value().0.clone();
                    let value = (*entry_ptr).get_value().1.clone();
                    let evicted_size = self.estimate_object_size(&key, &value);
                    self.metrics.core.record_eviction(evicted_size);
                    evicted = Some((key, value));
                    let _ = Box::from_raw(entry_ptr);
                }
            }
        }

        if let Some(node) = self.list.add((key.clone(), value)) {
            self.map.insert(key, node);
            self.metrics.core.record_insertion(object_size);
        }

        evicted
    }

    pub(crate) fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
        V: Clone,
    {
        let node = self.map.remove(key)?;
        unsafe {
            // SAFETY: node comes from our map
            let (k, v) = (*node).get_value();
            let object_size = self.estimate_object_size(k, v);
            let value = v.clone();
            self.list.remove(node);
            self.metrics.core.record_eviction(object_size);
            Some(value)
        }
    }

    pub(crate) fn clear(&mut self) {
        self.metrics.core.cache_size_bytes = 0;
        self.map.clear();
        self.list.clear();
    }
}

impl<K, V, S> core::fmt::Debug for LruSegment<K, V, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LruSegment")
            .field("capacity", &self.config.capacity())
            .field("len", &self.map.len())
            .finish()
    }
}

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
/// cache.put("cherry", 3);
/// assert_eq!(cache.get(&"banana"), None);
/// assert_eq!(cache.get(&"apple"), Some(&1));
/// assert_eq!(cache.get(&"cherry"), Some(&3));
/// ```
#[derive(Debug)]
pub struct LruCache<K, V, S = DefaultHashBuilder> {
    segment: LruSegment<K, V, S>,
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> LruCache<K, V, S> {
    /// Creates a new LRU cache with the specified capacity and hash builder.
    pub fn with_hasher(cap: NonZeroUsize, hash_builder: S) -> Self {
        Self {
            segment: LruSegment::with_hasher(cap, hash_builder),
        }
    }

    /// Creates a new LRU cache with specified capacity, hash builder, and size limit.
    pub fn with_hasher_and_size(cap: NonZeroUsize, hash_builder: S, max_size_bytes: u64) -> Self {
        Self {
            segment: LruSegment::with_hasher_and_size(cap, hash_builder, max_size_bytes),
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
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get(key)
    }

    #[inline]
    pub fn record_miss(&mut self, object_size: u64) {
        self.segment.record_miss(object_size);
    }

    #[inline]
    pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.get_mut(key)
    }
}

impl<K: Hash + Eq + Clone, V: Clone, S: BuildHasher> LruCache<K, V, S> {
    #[inline]
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        self.segment.put(key, value)
    }

    #[inline]
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq,
    {
        self.segment.remove(key)
    }

    #[inline]
    pub fn clear(&mut self) {
        self.segment.clear()
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        unimplemented!("Iteration not yet implemented")
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        unimplemented!("Mutable iteration not yet implemented")
    }
}

impl<K: Hash + Eq, V> LruCache<K, V>
where
    V: Clone,
{
    pub fn new(cap: NonZeroUsize) -> LruCache<K, V, DefaultHashBuilder> {
        LruCache::with_hasher(cap, DefaultHashBuilder::default())
    }
}

impl<K: Hash + Eq, V: Clone, S: BuildHasher> CacheMetrics for LruCache<K, V, S> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.segment.metrics().metrics()
    }

    fn algorithm_name(&self) -> &'static str {
        self.segment.metrics().algorithm_name()
    }
}

pub struct Iter<'a, K, V> {
    _marker: core::marker::PhantomData<&'a (K, V)>,
}

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
        assert_eq!(cache.put("apple", 1), None);
        assert_eq!(cache.put("banana", 2), None);
        assert_eq!(cache.get(&"apple"), Some(&1));
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), None);
        assert_eq!(cache.put("apple", 3).unwrap().1, 1);
        assert_eq!(cache.get(&"apple"), Some(&3));
        assert_eq!(cache.put("cherry", 4).unwrap().1, 2);
        assert_eq!(cache.get(&"banana"), None);
        assert_eq!(cache.get(&"apple"), Some(&3));
        assert_eq!(cache.get(&"cherry"), Some(&4));
    }

    #[test]
    fn test_lru_get_mut() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
        cache.put("apple", 1);
        cache.put("banana", 2);
        if let Some(v) = cache.get_mut(&"apple") {
            *v = 3;
        }
        assert_eq!(cache.get(&"apple"), Some(&3));
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
        assert_eq!(cache.get(&"apple"), Some(&1));
        assert_eq!(cache.get(&"banana"), Some(&2));
        assert_eq!(cache.get(&"cherry"), None);
        assert_eq!(cache.remove(&"apple"), Some(1));
        assert_eq!(cache.get(&"apple"), None);
        assert_eq!(cache.len(), 1);
        assert_eq!(cache.remove(&"cherry"), None);
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
        cache.put("cherry", 3);
        assert_eq!(cache.get(&"cherry"), Some(&3));
    }

    #[test]
    fn test_lru_capacity_limits() {
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
        cache.put("apple", 1);
        cache.put("banana", 2);
        cache.put("cherry", 3);
        assert_eq!(cache.len(), 2);
        assert_eq!(cache.get(&"apple"), None);
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
        assert_eq!(cache.get("apple"), Some(&1));
        assert_eq!(cache.get("banana"), Some(&2));
        drop(cache);
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
        let evicted = cache.put(String::from("cherry"), fruit3.clone());
        let evicted_fruit = evicted.unwrap();
        assert_eq!(evicted_fruit.1, fruit1);
        let removed = cache.remove(&key1);
        assert_eq!(removed, None);
    }

    #[test]
    fn test_lru_metrics() {
        use crate::metrics::CacheMetrics;
        let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
        let metrics = cache.metrics();
        assert_eq!(metrics.get("requests").unwrap(), &0.0);
        assert_eq!(metrics.get("cache_hits").unwrap(), &0.0);
        assert_eq!(metrics.get("cache_misses").unwrap(), &0.0);
        cache.put("apple", 1);
        cache.put("banana", 2);
        cache.get(&"apple");
        cache.get(&"banana");
        let metrics = cache.metrics();
        assert_eq!(metrics.get("cache_hits").unwrap(), &2.0);
        cache.record_miss(64);
        let metrics = cache.metrics();
        assert_eq!(metrics.get("cache_misses").unwrap(), &1.0);
        assert_eq!(metrics.get("requests").unwrap(), &3.0);
        cache.put("cherry", 3);
        let metrics = cache.metrics();
        assert_eq!(metrics.get("evictions").unwrap(), &1.0);
        assert!(metrics.get("bytes_written_to_cache").unwrap() > &0.0);
        assert_eq!(cache.algorithm_name(), "LRU");
    }

    #[test]
    fn test_lru_segment_directly() {
        let mut segment: LruSegment<&str, i32, DefaultHashBuilder> =
            LruSegment::with_hasher(NonZeroUsize::new(2).unwrap(), DefaultHashBuilder::default());
        assert_eq!(segment.len(), 0);
        assert!(segment.is_empty());
        assert_eq!(segment.cap().get(), 2);
        segment.put("a", 1);
        segment.put("b", 2);
        assert_eq!(segment.len(), 2);
        assert_eq!(segment.get(&"a"), Some(&1));
        assert_eq!(segment.get(&"b"), Some(&2));
    }

    #[test]
    fn test_lru_concurrent_access() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap())));
        let num_threads = 4;
        let ops_per_thread = 100;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        // Spawn writer threads
        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("thread_{}_key_{}", t, i);
                    let mut guard = cache.lock().unwrap();
                    guard.put(key, t * 1000 + i);
                }
            }));
        }

        // Spawn reader threads
        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("thread_{}_key_{}", t, i);
                    let mut guard = cache.lock().unwrap();
                    let _ = guard.get(&key);
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let guard = cache.lock().unwrap();
        assert!(guard.len() <= 100);
        assert!(!guard.is_empty());
    }

    #[test]
    fn test_lru_high_contention() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(50).unwrap())));
        let num_threads = 8;
        let ops_per_thread = 500;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}", i % 100); // Overlapping keys
                    let mut guard = cache.lock().unwrap();
                    if i % 2 == 0 {
                        guard.put(key, t * 1000 + i);
                    } else {
                        let _ = guard.get(&key);
                    }
                }
            }));
        }

        for handle in handles {
            handle.join().unwrap();
        }

        let guard = cache.lock().unwrap();
        assert!(guard.len() <= 50);
    }

    #[test]
    fn test_lru_concurrent_mixed_operations() {
        extern crate std;
        use std::sync::{Arc, Mutex};
        use std::thread;
        use std::vec::Vec;

        let cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(100).unwrap())));
        let num_threads = 8;
        let ops_per_thread = 1000;

        let mut handles: Vec<std::thread::JoinHandle<()>> = Vec::new();

        for t in 0..num_threads {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = std::format!("key_{}", i % 200);
                    let mut guard = cache.lock().unwrap();

                    match i % 4 {
                        0 => {
                            guard.put(key, i);
                        }
                        1 => {
                            let _ = guard.get(&key);
                        }
                        2 => {
                            let _ = guard.get_mut(&key);
                        }
                        3 => {
                            let _ = guard.remove(&key);
                        }
                        _ => unreachable!(),
                    }

                    if i == 500 && t == 0 {
                        guard.clear();
                    }
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
