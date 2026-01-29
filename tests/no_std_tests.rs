#![no_std]
extern crate alloc;
extern crate cache_rs;

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use cache_rs::config::{
    GdsfCacheConfig, LfuCacheConfig, LfudaCacheConfig, LruCacheConfig, SlruCacheConfig,
};
use cache_rs::GdsfCache;
use cache_rs::LfuCache;
use cache_rs::LfudaCache;
use cache_rs::LruCache;
use cache_rs::SlruCache;
use core::num::NonZeroUsize;

// Helper functions to create caches with the init pattern
fn make_lru<K: core::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LruCache<K, V> {
    let config = LruCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        max_size: u64::MAX,
    };
    LruCache::init(config, None)
}

fn make_lfu<K: core::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LfuCache<K, V> {
    let config = LfuCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        max_size: u64::MAX,
    };
    LfuCache::init(config, None)
}

fn make_lfuda<K: core::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LfudaCache<K, V> {
    let config = LfudaCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        initial_age: 0,
        max_size: u64::MAX,
    };
    LfudaCache::init(config, None)
}

fn make_slru<K: core::hash::Hash + Eq + Clone, V: Clone>(
    cap: usize,
    protected_cap: usize,
) -> SlruCache<K, V> {
    let config = SlruCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        protected_capacity: NonZeroUsize::new(protected_cap).unwrap(),
        max_size: u64::MAX,
    };
    SlruCache::init(config, None)
}

fn make_gdsf<K: core::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> GdsfCache<K, V> {
    let config = GdsfCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        initial_age: 0.0,
        max_size: u64::MAX,
    };
    GdsfCache::init(config, None)
}

#[test]
fn test_lru_in_no_std() {
    let mut cache = make_lru(2);

    // Using String as it requires the alloc crate
    let key1 = String::from("key1");
    let key2 = String::from("key2");
    let key3 = String::from("key3");

    cache.put(key1.clone(), 1);
    cache.put(key2.clone(), 2);

    // Check if keys are present
    assert_eq!(*cache.get(&key1).unwrap(), 1);
    assert_eq!(*cache.get(&key2).unwrap(), 2);

    // This should evict key1
    cache.put(key3.clone(), 3);

    assert!(cache.get(&key1).is_none());
    assert_eq!(*cache.get(&key2).unwrap(), 2);
    assert_eq!(*cache.get(&key3).unwrap(), 3);
}

#[test]
fn test_lfu_in_no_std() {
    let mut cache = make_lfu(2);

    let key1 = String::from("key1");
    let key2 = String::from("key2");

    cache.put(key1.clone(), 1);
    cache.put(key2.clone(), 2);

    // Access key1 multiple times to increase its frequency
    cache.get(&key1);
    cache.get(&key1);

    // Add a new item, which should evict key2 (lower frequency)
    let key3 = String::from("key3");
    cache.put(key3.clone(), 3);

    assert_eq!(*cache.get(&key1).unwrap(), 1);
    assert!(cache.get(&key2).is_none());
    assert_eq!(*cache.get(&key3).unwrap(), 3);
}

#[test]
fn test_lfuda_in_no_std() {
    let mut cache = make_lfuda(2);

    let key1 = String::from("key1");
    let key2 = String::from("key2");

    cache.put(key1.clone(), 1);
    cache.put(key2.clone(), 2);

    // Access key1 to increase its frequency
    cache.get(&key1);

    // Add a new key which should evict key2
    let key3 = String::from("key3");
    cache.put(key3.clone(), 3);

    assert_eq!(*cache.get(&key1).unwrap(), 1);
    assert!(cache.get(&key2).is_none());
    assert_eq!(*cache.get(&key3).unwrap(), 3);
}

#[test]
fn test_slru_in_no_std() {
    let mut cache = make_slru(4, 2);

    let keys: Vec<String> = (0..5).map(|i| format!("key{i}")).collect();

    // Add 4 items to fill the cache
    for (i, key) in keys.iter().enumerate().take(4) {
        cache.put(key.clone(), i);
    }

    // Access the first key to promote it to protected segment
    cache.get(&keys[0]);

    // Add a new item which should evict from probationary segment
    cache.put(keys[4].clone(), 4);

    // The first key should still be in the cache (protected)
    assert_eq!(*cache.get(&keys[0]).unwrap(), 0);

    // One of the unpromoted keys should have been evicted
    let mut found = 0;
    for key in keys.iter().take(4).skip(1) {
        if cache.get(key).is_some() {
            found += 1;
        }
    }

    // We should have 2 items in probationary segment
    assert_eq!(found, 2);

    // The new key should be in the cache
    assert_eq!(*cache.get(&keys[4]).unwrap(), 4);
}

#[test]
fn test_gdsf_in_no_std() {
    // We'll simply test basic operations of GDSF in no_std
    // Create a cache with small capacity
    let mut cache = make_gdsf(100);

    // Add some items
    let key1 = String::from("key1");
    let key2 = String::from("key2");

    cache.put(key1.clone(), "value1", 30);
    cache.put(key2.clone(), "value2", 50);

    // Verify we can retrieve the items - GDSF get() returns values, not references
    assert_eq!(cache.get(&key1), Some("value1"));
    assert_eq!(cache.get(&key2), Some("value2"));

    // Test eviction behavior
    let key3 = String::from("key3");
    // Access key1 multiple times to increase its frequency
    cache.get(&key1);
    cache.get(&key1);

    // Add a third item, which will cause an eviction if the total size exceeds capacity
    cache.put(key3.clone(), "value3", 40);

    // Verify we can still access at least one of the items
    assert!(cache.get(&key1).is_some() || cache.get(&key2).is_some() || cache.get(&key3).is_some());

    // Test clear
    cache.clear();
    assert!(cache.get(&key1).is_none());
    assert!(cache.get(&key2).is_none());
    assert!(cache.get(&key3).is_none());
}

#[test]
fn test_complex_types_in_no_std() {
    // Test with more complex types that require alloc
    let mut cache = make_lru(2);

    let key1 = Vec::<u8>::from([1, 2, 3]);
    let value1 = Vec::<i32>::from([10, 20, 30]);

    let key2 = Vec::<u8>::from([4, 5, 6]);
    let value2 = Vec::<i32>::from([40, 50, 60]);

    cache.put(key1.clone(), value1.clone());
    cache.put(key2.clone(), value2.clone());

    assert_eq!(*cache.get(&key1).unwrap(), value1);
    assert_eq!(*cache.get(&key2).unwrap(), value2);
}
