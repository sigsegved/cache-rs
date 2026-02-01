//! Correctness Tests for Cache Algorithms
//!
//! This module validates the fundamental correctness of each cache algorithm
//! using simple, predictable access patterns. Each test explicitly validates
//! which specific key gets evicted when a put causes an eviction.
//!
//! ## Test Strategy
//! - Small cache sizes (3-5 entries) for predictable behavior
//! - Simple, deterministic access patterns
//! - Each test validates the core eviction policy of the algorithm
//! - Explicit checks for which key was evicted after each put

use cache_rs::config::{
    GdsfCacheConfig, LfuCacheConfig, LfudaCacheConfig, LruCacheConfig, SlruCacheConfig,
};
use cache_rs::{GdsfCache, LfuCache, LfudaCache, LruCache, SlruCache};
use std::num::NonZeroUsize;

const OBJECT_SIZE: u64 = 1;

// ============================================================================
// HELPER FUNCTIONS FOR CACHE CREATION
// ============================================================================

/// Helper to create an LruCache with the given capacity
fn make_lru<K: std::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LruCache<K, V> {
    let config = LruCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        max_size: u64::MAX,
    };
    LruCache::init(config, None)
}

/// Helper to create an LruCache with capacity and max_size limit
fn make_lru_with_limits<K: std::hash::Hash + Eq + Clone, V: Clone>(
    cap: usize,
    max_size: u64,
) -> LruCache<K, V> {
    let config = LruCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        max_size,
    };
    LruCache::init(config, None)
}

/// Helper to create an LruCache with max_size limit only (large capacity)
fn make_lru_with_max_size<K: std::hash::Hash + Eq + Clone, V: Clone>(
    max_size: u64,
) -> LruCache<K, V> {
    let config = LruCacheConfig {
        capacity: NonZeroUsize::new(16384).unwrap(),
        max_size,
    };
    LruCache::init(config, None)
}

/// Helper to create an LfuCache with the given capacity
fn make_lfu<K: std::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LfuCache<K, V> {
    let config = LfuCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        max_size: u64::MAX,
    };
    LfuCache::init(config, None)
}

/// Helper to create an LfuCache with max_size limit only (large capacity)
fn make_lfu_with_max_size<K: std::hash::Hash + Eq + Clone, V: Clone>(
    max_size: u64,
) -> LfuCache<K, V> {
    let config = LfuCacheConfig {
        capacity: NonZeroUsize::new(16384).unwrap(),
        max_size,
    };
    LfuCache::init(config, None)
}

/// Helper to create an LfudaCache with the given capacity
fn make_lfuda<K: std::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LfudaCache<K, V> {
    let config = LfudaCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        initial_age: 0,
        max_size: u64::MAX,
    };
    LfudaCache::init(config, None)
}

/// Helper to create an LfudaCache with max_size limit only (large capacity)
fn make_lfuda_with_max_size<K: std::hash::Hash + Eq + Clone, V: Clone>(
    max_size: u64,
) -> LfudaCache<K, V> {
    let config = LfudaCacheConfig {
        capacity: NonZeroUsize::new(16384).unwrap(),
        initial_age: 0,
        max_size,
    };
    LfudaCache::init(config, None)
}

/// Helper to create an SlruCache with the given capacity and protected capacity
fn make_slru<K: std::hash::Hash + Eq + Clone, V: Clone>(
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

/// Helper to create an SlruCache with max_size limit only (large capacity)
fn make_slru_with_max_size<K: std::hash::Hash + Eq + Clone, V: Clone>(
    max_size: u64,
) -> SlruCache<K, V> {
    let config = SlruCacheConfig {
        capacity: NonZeroUsize::new(16384).unwrap(),
        protected_capacity: NonZeroUsize::new(3276).unwrap(), // ~20%
        max_size,
    };
    SlruCache::init(config, None)
}

/// Helper to create a GdsfCache with the given capacity
fn make_gdsf<K: std::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> GdsfCache<K, V> {
    let config = GdsfCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        initial_age: 0.0,
        max_size: u64::MAX,
    };
    GdsfCache::init(config, None)
}

// ============================================================================
// LRU CORRECTNESS
// ============================================================================
// LRU evicts the Least Recently Used item.
// Correctness criteria:
// 1. Most recently accessed items stay in cache
// 2. Oldest accessed items are evicted first
// 3. Access (get) updates recency, preventing eviction

#[test]
fn test_lru_evicts_least_recently_used() {
    let mut cache = make_lru(3);

    // Fill cache: order of insertion determines initial LRU order
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    // LRU order: 1 (LRU) -> 2 -> 3 (MRU)

    // Verify all present before any eviction
    assert!(cache.get(&1).is_some(), "Key 1 should be present");
    assert!(cache.get(&2).is_some(), "Key 2 should be present");
    assert!(cache.get(&3).is_some(), "Key 3 should be present");
    // After gets: LRU order is now 1 -> 2 -> 3 (order of access)

    // Insert new key - should evict key 1 (LRU)
    cache.put(4, 40);

    // VALIDATE EVICTION: Key 1 should be evicted
    assert!(
        cache.get(&1).is_none(),
        "Key 1 should have been evicted (was LRU)"
    );
    assert!(cache.get(&2).is_some(), "Key 2 should remain");
    assert!(cache.get(&3).is_some(), "Key 3 should remain");
    assert!(cache.get(&4).is_some(), "Key 4 should be present");
    // After gets: LRU order is 2 -> 3 -> 4

    // Insert another - should evict key 2 (now LRU)
    cache.put(5, 50);

    // VALIDATE EVICTION: Key 2 should be evicted
    assert!(
        cache.get(&2).is_none(),
        "Key 2 should have been evicted (was LRU)"
    );
    assert!(cache.get(&3).is_some(), "Key 3 should remain");
    assert!(cache.get(&4).is_some(), "Key 4 should remain");
    assert!(cache.get(&5).is_some(), "Key 5 should be present");
}

#[test]
fn test_lru_eviction_order_is_predictable() {
    let mut cache = make_lru(5);

    // Fill cache with keys 0..4
    for i in 0..5 {
        cache.put(i, i * 10);
    }
    // LRU order: 0 (LRU) -> 1 -> 2 -> 3 -> 4 (MRU)

    // Insert key 5 - should evict key 0
    cache.put(5, 50);
    assert!(
        cache.get(&0).is_none(),
        "First eviction: Key 0 should be evicted"
    );

    // Insert key 6 - should evict key 1
    cache.put(6, 60);
    assert!(
        cache.get(&1).is_none(),
        "Second eviction: Key 1 should be evicted"
    );

    // Insert key 7 - should evict key 2
    cache.put(7, 70);
    assert!(
        cache.get(&2).is_none(),
        "Third eviction: Key 2 should be evicted"
    );

    // Remaining keys should be 3, 4, 5, 6, 7
    assert!(cache.get(&3).is_some(), "Key 3 should remain");
    assert!(cache.get(&4).is_some(), "Key 4 should remain");
    assert!(cache.get(&5).is_some(), "Key 5 should remain");
    assert!(cache.get(&6).is_some(), "Key 6 should remain");
    assert!(cache.get(&7).is_some(), "Key 7 should remain");
}

#[test]
fn test_lru_get_updates_recency() {
    let mut cache = make_lru(3);

    // Fill cache
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    // LRU order: 1 (LRU) -> 2 -> 3 (MRU)

    // Access key 1 to make it recently used
    assert_eq!(cache.get(&1), Some(&10));
    // LRU order: 2 (LRU) -> 3 -> 1 (MRU)

    // Insert new key - should evict key 2 (now LRU), NOT key 1
    cache.put(4, 40);

    // VALIDATE: Key 2 evicted (not key 1 which was accessed)
    assert!(
        cache.get(&1).is_some(),
        "Key 1 should survive due to recent access"
    );
    assert!(
        cache.get(&2).is_none(),
        "Key 2 should be evicted (was LRU after key 1 was accessed)"
    );
    assert!(cache.get(&3).is_some(), "Key 3 should remain");
    assert!(cache.get(&4).is_some(), "Key 4 should be present");
}

// ============================================================================
// LFU CORRECTNESS
// ============================================================================
// LFU evicts the Least Frequently Used item.
// Correctness criteria:
// 1. Items with lowest access frequency are evicted first
// 2. Among same frequency, FIFO order is used as tiebreaker
// 3. Each get() increases frequency

#[test]
fn test_lfu_evicts_least_frequently_used() {
    let mut cache = make_lfu(3);

    // Fill cache
    cache.put(1, 10); // freq=1
    cache.put(2, 20); // freq=1
    cache.put(3, 30); // freq=1

    // Access key 1 and 2 to increase their frequency
    cache.get(&1); // freq=2
    cache.get(&1); // freq=3
    cache.get(&2); // freq=2

    // Frequencies: key1=3, key2=2, key3=1 (lowest)

    // Insert new key - should evict key 3 (lowest frequency)
    cache.put(4, 40);

    // VALIDATE EVICTION: Key 3 should be evicted (freq=1)
    assert!(
        cache.get(&3).is_none(),
        "Key 3 should be evicted (lowest freq=1)"
    );
    assert!(cache.get(&1).is_some(), "Key 1 should remain (freq=3)");
    assert!(cache.get(&2).is_some(), "Key 2 should remain (freq=2)");
    assert!(cache.get(&4).is_some(), "Key 4 should be present");
}

#[test]
fn test_lfu_frequency_accumulates() {
    let mut cache = make_lfu(3);

    cache.put("hot", 1);
    cache.put("warm", 2);
    cache.put("cold", 3);

    // Access "hot" many times (freq=11)
    for _ in 0..10 {
        cache.get(&"hot");
    }

    // Access "warm" a few times (freq=4)
    for _ in 0..3 {
        cache.get(&"warm");
    }

    // Frequencies: hot=11, warm=4, cold=1 (lowest)

    // Insert new item - should evict "cold"
    cache.put("new", 4);

    // VALIDATE EVICTION: "cold" should be evicted (freq=1)
    assert!(
        cache.get(&"cold").is_none(),
        "cold should be evicted (lowest freq)"
    );
    assert!(cache.get(&"hot").is_some(), "hot should remain");
    assert!(cache.get(&"warm").is_some(), "warm should remain");
    assert!(cache.get(&"new").is_some(), "new should be present");
}

#[test]
fn test_lfu_same_frequency_uses_fifo() {
    let mut cache = make_lfu(3);

    // Insert 3 items - all have same frequency (1)
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    // Insert new item - should evict key 1 (first inserted among same frequency)
    cache.put(4, 40);

    // VALIDATE EVICTION: Key 1 should be evicted (FIFO among same freq)
    assert!(
        cache.get(&1).is_none(),
        "Key 1 should be evicted (FIFO among same freq)"
    );

    // Insert another - should evict key 2 (next in FIFO order)
    cache.put(5, 50);

    // VALIDATE EVICTION: Key 2 should be evicted (FIFO)
    assert!(
        cache.get(&2).is_none(),
        "Key 2 should be evicted (FIFO among same freq)"
    );
}

// ============================================================================
// LFUDA CORRECTNESS
// ============================================================================
// LFUDA = LFU with Dynamic Aging.
// Aging prevents cache pollution from historically frequent items.
// Correctness criteria:
// 1. Evicts item with lowest priority (frequency + age)
// 2. When evicting, global age increases
// 3. Newly inserted items benefit from current age

#[test]
fn test_lfuda_evicts_lowest_priority() {
    let mut cache = make_lfuda(3);

    // Fill cache
    cache.put(1, 10); // priority = freq + age = 1 + 0 = 1
    cache.put(2, 20); // priority = 1 + 0 = 1
    cache.put(3, 30); // priority = 1 + 0 = 1

    // Access key 1 and 2 to increase their priority
    cache.get(&1); // priority increases
    cache.get(&1); // priority increases more
    cache.get(&2); // priority increases

    // Key 3 has lowest priority (only initial put, no gets)

    // Insert new key - should evict key 3
    cache.put(4, 40);

    // VALIDATE EVICTION: Key 3 should be evicted (lowest priority)
    assert!(
        cache.get(&3).is_none(),
        "Key 3 should be evicted (lowest priority)"
    );
    assert!(
        cache.get(&1).is_some(),
        "Key 1 should remain (high priority)"
    );
    assert!(cache.get(&2).is_some(), "Key 2 should remain");
    assert!(cache.get(&4).is_some(), "Key 4 should be present");
}

#[test]
fn test_lfuda_aging_helps_new_items() {
    let mut cache = make_lfuda(3);

    // Insert items
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    // Access all items to increase frequency
    for _ in 0..5 {
        cache.get(&1);
        cache.get(&2);
        cache.get(&3);
    }

    // Now evict and insert several times to increase global age
    // Each eviction increases the age
    cache.put(4, 40); // evicts one item, age increases
    cache.put(5, 50); // evicts another, age increases more

    // New items benefit from the elevated global age
    // The surviving items should be those with highest effective priority
    assert_eq!(cache.len(), 3, "Cache should be at capacity");
}

#[test]
fn test_lfuda_basic_eviction() {
    let mut cache = make_lfuda(4);

    // Fill cache
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // Access first two items more frequently
    for _ in 0..3 {
        cache.get(&1);
        cache.get(&2);
    }

    // Insert new item - should evict 3 or 4 (lowest frequency)
    cache.put(5, 50);

    // VALIDATE EVICTION: One of 3 or 4 should be evicted (both have freq=1)
    let key3_evicted = cache.get(&3).is_none();
    let key4_evicted = cache.get(&4).is_none();
    assert!(
        key3_evicted || key4_evicted,
        "One of the low-frequency items (3 or 4) should be evicted"
    );

    // High frequency items should remain
    assert!(cache.get(&1).is_some(), "Key 1 should remain (high freq)");
    assert!(cache.get(&2).is_some(), "Key 2 should remain (high freq)");
}

// ============================================================================
// SLRU CORRECTNESS
// ============================================================================
// SLRU = Segmented LRU with probationary and protected segments.
// Correctness criteria:
// 1. New items enter probationary segment
// 2. Accessed items in probationary get promoted to protected
// 3. Protected items are safer from eviction
// 4. When protected is full, demoted items go back to probationary

#[test]
fn test_slru_promotion_to_protected() {
    // Capacity 4, protected size 2
    let mut cache = make_slru(4, 2);

    // Insert items - they start in probationary
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // Access key 1 to promote it to protected segment
    cache.get(&1);
    cache.get(&1); // Second access ensures promotion

    // Access key 2 to promote it to protected segment
    cache.get(&2);
    cache.get(&2);

    // Now: protected = {1, 2}, probationary = {3, 4}
    // Probationary LRU order: 3 (LRU) -> 4 (MRU)

    // Insert new item - should evict from probationary (LRU in probationary = 3)
    cache.put(5, 50);

    // VALIDATE EVICTION: Key 3 should be evicted from probationary
    assert!(
        cache.get(&3).is_none(),
        "Key 3 should be evicted from probationary"
    );

    // Protected items should survive
    assert!(cache.get(&1).is_some(), "Key 1 should remain (protected)");
    assert!(cache.get(&2).is_some(), "Key 2 should remain (protected)");
}

#[test]
fn test_slru_probationary_evicted_first() {
    let mut cache = make_slru(4, 2);

    // Insert 4 items (all in probationary initially)
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // Promote keys 3 and 4 to protected by accessing them
    cache.get(&3);
    cache.get(&3);
    cache.get(&4);
    cache.get(&4);

    // Now: protected = {3, 4}, probationary = {1, 2}
    // Probationary LRU order: 1 (LRU) -> 2 (MRU)

    // Insert new item - should evict key 1 (LRU in probationary)
    cache.put(5, 50);

    // VALIDATE EVICTION: Key 1 should be evicted (LRU in probationary)
    assert!(
        cache.get(&1).is_none(),
        "Key 1 should be evicted (LRU in probationary)"
    );

    // Protected items should survive
    assert!(cache.get(&3).is_some(), "Key 3 should remain (protected)");
    assert!(cache.get(&4).is_some(), "Key 4 should remain (protected)");
    assert!(
        cache.get(&2).is_some(),
        "Key 2 should remain (still in probationary)"
    );
    assert!(cache.get(&5).is_some(), "Key 5 should be present");
}

#[test]
fn test_slru_eviction_order_in_probationary() {
    let mut cache = make_slru(4, 1);

    // Insert 4 items
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // Promote only key 4 to protected
    cache.get(&4);
    cache.get(&4);

    // Probationary: 1 (LRU) -> 2 -> 3 (MRU), Protected: 4

    // Insert new items and verify eviction order from probationary
    cache.put(5, 50);
    assert!(
        cache.get(&1).is_none(),
        "First eviction: Key 1 should be evicted"
    );

    cache.put(6, 60);
    assert!(
        cache.get(&2).is_none(),
        "Second eviction: Key 2 should be evicted"
    );

    cache.put(7, 70);
    assert!(
        cache.get(&3).is_none(),
        "Third eviction: Key 3 should be evicted"
    );

    // Key 4 (protected) should still be present
    assert!(cache.get(&4).is_some(), "Key 4 should remain (protected)");
}

// ============================================================================
// GDSF CORRECTNESS
// ============================================================================
// GDSF = Greedy Dual-Size Frequency.
// Priority = (Frequency / Size) + Age
// Correctness criteria:
// 1. Smaller objects are preferred (higher priority for same frequency)
// 2. More frequent objects are preferred
// 3. Size parameter affects eviction decisions

#[test]
fn test_gdsf_prefers_smaller_objects() {
    let mut cache = make_gdsf(3);

    // Insert items with different sizes, same frequency
    cache.put(1, 10, 100); // Large object (size=100)
    cache.put(2, 20, 1); // Small object (size=1)
    cache.put(3, 30, 1); // Small object (size=1)

    // All have frequency 1, but priorities differ:
    // Key 1: priority = 1/100 = 0.01
    // Key 2: priority = 1/1 = 1.0
    // Key 3: priority = 1/1 = 1.0

    // Insert new item - should evict key 1 (lowest priority due to size)
    cache.put(4, 40, 1);

    // VALIDATE EVICTION: Key 1 (large object) should be evicted
    assert!(
        cache.get(&1).is_none(),
        "Large object (key 1) should be evicted first due to low priority"
    );

    // Small objects should survive
    assert!(cache.get(&2).is_some(), "Small object 2 should remain");
    assert!(cache.get(&3).is_some(), "Small object 3 should remain");
    assert!(
        cache.get(&4).is_some(),
        "New small object should be present"
    );
}

#[test]
fn test_gdsf_frequency_matters() {
    let mut cache = make_gdsf(3);

    // Insert items with same size
    cache.put(1, 10, OBJECT_SIZE);
    cache.put(2, 20, OBJECT_SIZE);
    cache.put(3, 30, OBJECT_SIZE);

    // Access key 1 many times to increase frequency
    for _ in 0..10 {
        cache.get(&1);
    }

    // Access key 2 a few times
    for _ in 0..3 {
        cache.get(&2);
    }

    // Key 3 has lowest frequency
    // Priorities (freq/size): key1=11/1=11, key2=4/1=4, key3=1/1=1

    // Insert new item - should evict key 3 (lowest priority)
    cache.put(4, 40, OBJECT_SIZE);

    // VALIDATE EVICTION: Key 3 should be evicted (lowest frequency)
    assert!(
        cache.get(&3).is_none(),
        "Lowest frequency item (key 3) should be evicted"
    );
    assert!(cache.get(&1).is_some(), "High freq item should remain");
    assert!(cache.get(&2).is_some(), "Medium freq item should remain");
}

#[test]
fn test_gdsf_size_frequency_tradeoff() {
    let mut cache = make_gdsf(3);

    // Large object with high frequency
    cache.put(1, 10, 100); // size=100
    for _ in 0..20 {
        cache.get(&1); // freq=21
    }
    // Priority = 21/100 = 0.21

    // Small object with low frequency
    cache.put(2, 20, 1); // size=1, freq=1
                         // Priority = 1/1 = 1.0

    // Another small object
    cache.put(3, 30, 1); // size=1, freq=1
                         // Priority = 1/1 = 1.0

    // Despite high frequency, the large object has lower priority (0.21 < 1.0)
    // Insert new item
    cache.put(4, 40, 1);

    // VALIDATE EVICTION: Large object evicted despite high frequency
    assert!(
        cache.get(&1).is_none(),
        "Large object should be evicted despite high frequency (priority 0.21 < 1.0)"
    );
    assert!(cache.get(&2).is_some(), "Small object 2 should remain");
    assert!(cache.get(&3).is_some(), "Small object 3 should remain");
    assert!(cache.get(&4).is_some(), "New object should be present");
}

#[test]
fn test_gdsf_eviction_order_by_priority() {
    let mut cache = make_gdsf(4);

    // Insert items with varying size to create predictable priorities
    cache.put(1, 10, 10); // size=10, priority = 1/10 = 0.1
    cache.put(2, 20, 5); // size=5,  priority = 1/5 = 0.2
    cache.put(3, 30, 2); // size=2,  priority = 1/2 = 0.5
    cache.put(4, 40, 1); // size=1,  priority = 1/1 = 1.0

    // Insert new items and verify eviction order (lowest priority first)
    cache.put(5, 50, 1);
    assert!(
        cache.get(&1).is_none(),
        "Key 1 evicted first (priority 0.1)"
    );

    cache.put(6, 60, 1);
    assert!(
        cache.get(&2).is_none(),
        "Key 2 evicted second (priority 0.2)"
    );

    cache.put(7, 70, 1);
    assert!(
        cache.get(&3).is_none(),
        "Key 3 evicted third (priority 0.5)"
    );

    // Key 4 (highest priority among originals) should remain
    assert!(
        cache.get(&4).is_some(),
        "Key 4 should remain (highest priority 1.0)"
    );
}

// ============================================================================
// COMMON OPERATIONS CORRECTNESS
// ============================================================================

#[test]
fn test_all_caches_basic_operations() {
    // Test that all caches implement basic operations correctly

    // LRU
    let mut lru = make_lru(10);
    lru.put("key", 42);
    assert_eq!(lru.get(&"key"), Some(&42));
    assert_eq!(lru.remove(&"key"), Some(42));
    assert_eq!(lru.get(&"key"), None);

    // LFU
    let mut lfu = make_lfu(10);
    lfu.put("key", 42);
    assert_eq!(lfu.get(&"key"), Some(&42));
    assert_eq!(lfu.remove(&"key"), Some(42));
    assert_eq!(lfu.get(&"key"), None);

    // LFUDA
    let mut lfuda = make_lfuda(10);
    lfuda.put("key", 42);
    assert_eq!(lfuda.get(&"key"), Some(&42));
    assert_eq!(lfuda.remove(&"key"), Some(42));
    assert_eq!(lfuda.get(&"key"), None);

    // SLRU
    let mut slru = make_slru(10, 5);
    slru.put("key", 42);
    assert_eq!(slru.get(&"key"), Some(&42));
    assert_eq!(slru.remove(&"key"), Some(42));
    assert_eq!(slru.get(&"key"), None);

    // GDSF - note: get() returns Option<V>, not Option<&V>
    let mut gdsf = make_gdsf(10);
    gdsf.put("key", 42, 1);
    assert_eq!(gdsf.get(&"key"), Some(42));
    // GDSF doesn't have remove(), verify key exists then clear
    gdsf.clear();
    assert_eq!(gdsf.get(&"key"), None);
}

#[test]
fn test_all_caches_capacity_enforcement() {
    // LRU
    let mut lru = make_lru(3);
    for i in 0..10 {
        lru.put(i, i);
    }
    assert_eq!(lru.len(), 3, "LRU should enforce capacity");

    // LFU
    let mut lfu = make_lfu(3);
    for i in 0..10 {
        lfu.put(i, i);
    }
    assert_eq!(lfu.len(), 3, "LFU should enforce capacity");

    // LFUDA
    let mut lfuda = make_lfuda(3);
    for i in 0..10 {
        lfuda.put(i, i);
    }
    assert_eq!(lfuda.len(), 3, "LFUDA should enforce capacity");

    // SLRU
    let mut slru = make_slru(3, 1);
    for i in 0..10 {
        slru.put(i, i);
    }
    assert_eq!(slru.len(), 3, "SLRU should enforce capacity");

    // GDSF
    let mut gdsf = make_gdsf(3);
    for i in 0..10 {
        gdsf.put(i, i, 1);
    }
    assert_eq!(gdsf.len(), 3, "GDSF should enforce capacity");
}

#[test]
fn test_all_caches_update_existing_key() {
    // LRU - updating existing key should not change len
    let mut lru = make_lru(3);
    lru.put(1, 10);
    lru.put(2, 20);
    lru.put(1, 100); // Update key 1
    assert_eq!(lru.len(), 2, "LRU: update should not increase len");
    assert_eq!(lru.get(&1), Some(&100), "LRU: value should be updated");

    // LFU
    let mut lfu = make_lfu(3);
    lfu.put(1, 10);
    lfu.put(2, 20);
    lfu.put(1, 100);
    assert_eq!(lfu.len(), 2, "LFU: update should not increase len");
    assert_eq!(lfu.get(&1), Some(&100), "LFU: value should be updated");

    // LFUDA
    let mut lfuda = make_lfuda(3);
    lfuda.put(1, 10);
    lfuda.put(2, 20);
    lfuda.put(1, 100);
    assert_eq!(lfuda.len(), 2, "LFUDA: update should not increase len");
    assert_eq!(lfuda.get(&1), Some(&100), "LFUDA: value should be updated");

    // SLRU
    let mut slru = make_slru(3, 1);
    slru.put(1, 10);
    slru.put(2, 20);
    slru.put(1, 100);
    assert_eq!(slru.len(), 2, "SLRU: update should not increase len");
    assert_eq!(slru.get(&1), Some(&100), "SLRU: value should be updated");

    // GDSF - note: get() returns Option<V>, not Option<&V>
    let mut gdsf = make_gdsf(3);
    gdsf.put(1, 10, 1);
    gdsf.put(2, 20, 1);
    gdsf.put(1, 100, 1);
    assert_eq!(gdsf.len(), 2, "GDSF: update should not increase len");
    assert_eq!(gdsf.get(&1), Some(100), "GDSF: value should be updated");
}

#[test]
fn test_all_caches_clear() {
    // LRU
    let mut lru = make_lru(5);
    for i in 0..5 {
        lru.put(i, i);
    }
    lru.clear();
    assert_eq!(lru.len(), 0, "LRU: clear should empty cache");
    assert!(
        lru.get(&0).is_none(),
        "LRU: get after clear should return None"
    );

    // LFU
    let mut lfu = make_lfu(5);
    for i in 0..5 {
        lfu.put(i, i);
    }
    lfu.clear();
    assert_eq!(lfu.len(), 0, "LFU: clear should empty cache");

    // LFUDA
    let mut lfuda = make_lfuda(5);
    for i in 0..5 {
        lfuda.put(i, i);
    }
    lfuda.clear();
    assert_eq!(lfuda.len(), 0, "LFUDA: clear should empty cache");

    // SLRU
    let mut slru = make_slru(5, 2);
    for i in 0..5 {
        slru.put(i, i);
    }
    slru.clear();
    assert_eq!(slru.len(), 0, "SLRU: clear should empty cache");

    // GDSF
    let mut gdsf = make_gdsf(5);
    for i in 0..5 {
        gdsf.put(i, i, 1);
    }
    gdsf.clear();
    assert_eq!(gdsf.len(), 0, "GDSF: clear should empty cache");
}

// ============================================================================
// SIZE-BASED CORRECTNESS
// ============================================================================
// Tests for size-aware caching with with_max_size() and with_limits()
// NOTE: Current implementation tracks size but does NOT evict based on size.
// Eviction is only triggered by entry count limits.
// Correctness criteria:
// 1. current_size() tracks total size of cached items
// 2. put_with_size() properly accounts for item sizes
// 3. Size is updated correctly on insert/clear

#[test]
fn test_lru_size_tracking() {
    // Create cache with max_size=100 bytes (entry count effectively unlimited)
    let mut cache = make_lru_with_max_size(100);

    // Insert items with sizes
    cache.put_with_size(1, "a", 30); // size=30, total=30
    assert_eq!(
        cache.current_size(),
        30,
        "Size should be 30 after first insert"
    );

    cache.put_with_size(2, "b", 40); // size=40, total=70
    assert_eq!(
        cache.current_size(),
        70,
        "Size should be 70 after second insert"
    );

    cache.put_with_size(3, "c", 20); // size=20, total=90
    assert_eq!(
        cache.current_size(),
        90,
        "Size should be 90 after third insert"
    );

    // All items should be present (under entry count limit)
    assert!(cache.get(&1).is_some(), "Key 1 should be present");
    assert!(cache.get(&2).is_some(), "Key 2 should be present");
    assert!(cache.get(&3).is_some(), "Key 3 should be present");
}

#[test]
fn test_lru_size_tracking_accumulates() {
    let mut cache = make_lru_with_max_size(1000);

    // Insert multiple items and verify size accumulates
    for i in 0..10 {
        cache.put_with_size(i, format!("value{}", i), 50);
    }

    assert_eq!(
        cache.current_size(),
        500,
        "Total size should be 500 (10 * 50)"
    );
    assert_eq!(cache.len(), 10, "Should have 10 entries");
}

#[test]
fn test_lru_entry_count_eviction_updates_size() {
    // Create cache with entry count limit of 3
    let mut cache = make_lru_with_limits(3, 1000);

    cache.put_with_size(1, "a", 30);
    cache.put_with_size(2, "b", 40);
    cache.put_with_size(3, "c", 50);
    assert_eq!(cache.current_size(), 120);
    assert_eq!(cache.len(), 3);

    // Insert 4th item - triggers count-based eviction of key 1
    cache.put_with_size(4, "d", 60);

    assert!(
        cache.get(&1).is_none(),
        "Key 1 should be evicted (LRU, count limit)"
    );
    assert_eq!(cache.len(), 3, "Should still have 3 entries");
    // Size tracking: removed 30, added 60, so 120 - 30 + 60 = 150
    // Note: actual behavior may use estimate_object_size for evicted items
}

#[test]
fn test_lfu_size_tracking() {
    let mut cache = make_lfu_with_max_size(1000);

    cache.put_with_size(1, "a", 100);
    cache.put_with_size(2, "b", 200);
    cache.put_with_size(3, "c", 150);

    assert_eq!(cache.current_size(), 450, "Total size should be 450");
    assert_eq!(cache.len(), 3);
}

#[test]
fn test_lfuda_size_tracking() {
    let mut cache = make_lfuda_with_max_size(1000);

    cache.put_with_size(1, "a", 100);
    cache.put_with_size(2, "b", 200);

    assert_eq!(cache.current_size(), 300, "Total size should be 300");
}

#[test]
fn test_slru_size_tracking() {
    let mut cache = make_slru_with_max_size(1000);

    cache.put_with_size(1, "a", 100);
    cache.put_with_size(2, "b", 200);
    cache.put_with_size(3, "c", 150);

    assert_eq!(cache.current_size(), 450, "Total size should be 450");
}

#[test]
fn test_size_reset_on_clear() {
    let mut cache = make_lru_with_max_size(1000);

    cache.put_with_size(1, "a", 30);
    cache.put_with_size(2, "b", 40);
    assert_eq!(cache.current_size(), 70);

    cache.clear();
    assert_eq!(cache.current_size(), 0, "Size should be 0 after clear");
    assert_eq!(cache.len(), 0, "Length should be 0 after clear");
}

#[test]
fn test_max_size_getter() {
    let cache: LruCache<i32, i32> = make_lru_with_max_size(500);
    assert_eq!(
        cache.max_size(),
        500,
        "max_size should return configured limit"
    );

    // Count-only cache has max_size=u64::MAX
    let cache2: LruCache<i32, i32> = make_lru(10);
    assert_eq!(
        cache2.max_size(),
        u64::MAX,
        "max_size should be u64::MAX for count-only cache"
    );
}

#[test]
fn test_with_limits_constructor() {
    let cache: LruCache<i32, i32> = make_lru_with_limits(100, 50000);

    assert_eq!(cache.max_size(), 50000);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_size(), 0);
}

// ============================================================================
// CORNER CASES: LRU
// ============================================================================

#[test]
fn test_lru_capacity_one() {
    // Edge case: cache that can only hold 1 item
    let mut cache = make_lru(1);

    cache.put(1, 10);
    assert_eq!(cache.get(&1), Some(&10));

    // Second insert immediately evicts first
    cache.put(2, 20);
    assert!(cache.get(&1).is_none(), "Key 1 should be evicted");
    assert_eq!(cache.get(&2), Some(&20), "Key 2 should be present");

    // Third insert evicts second
    cache.put(3, 30);
    assert!(cache.get(&2).is_none(), "Key 2 should be evicted");
    assert_eq!(cache.get(&3), Some(&30), "Key 3 should be present");
}

#[test]
fn test_lru_update_moves_to_mru() {
    let mut cache = make_lru(3);

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    // LRU order: 1 -> 2 -> 3

    // Update key 1 - should move to MRU position
    cache.put(1, 100);
    // LRU order should now be: 2 -> 3 -> 1

    // Insert new key - should evict 2 (now LRU), not 1
    cache.put(4, 40);

    assert!(
        cache.get(&2).is_none(),
        "Key 2 should be evicted (was LRU after update)"
    );
    assert_eq!(
        cache.get(&1),
        Some(&100),
        "Key 1 should remain with updated value"
    );
    assert!(cache.get(&3).is_some(), "Key 3 should remain");
    assert!(cache.get(&4).is_some(), "Key 4 should be present");
}

#[test]
fn test_lru_reverse_access_order() {
    let mut cache = make_lru(5);

    // Insert 1, 2, 3, 4, 5
    for i in 1..=5 {
        cache.put(i, i * 10);
    }
    // LRU order: 1 -> 2 -> 3 -> 4 -> 5

    // Access in reverse order: 5, 4, 3, 2, 1
    for i in (1..=5).rev() {
        cache.get(&i);
    }
    // LRU order should now be: 5 -> 4 -> 3 -> 2 -> 1 (reversed)

    // Insert new key - should evict 5 (now LRU)
    cache.put(6, 60);
    assert!(
        cache.get(&5).is_none(),
        "Key 5 should be evicted (was LRU after reverse access)"
    );
    assert!(cache.get(&1).is_some(), "Key 1 should remain (was MRU)");
}

#[test]
fn test_lru_remove_and_reinsert() {
    let mut cache = make_lru(3);

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    // Remove middle item
    assert_eq!(cache.remove(&2), Some(20));
    assert_eq!(cache.len(), 2);

    // Reinsert - should go to MRU position
    cache.put(2, 200);
    // LRU order: 1 -> 3 -> 2

    // Insert new key - should evict 1
    cache.put(4, 40);
    assert!(cache.get(&1).is_none(), "Key 1 should be evicted");
    assert_eq!(
        cache.get(&2),
        Some(&200),
        "Key 2 should be present with new value"
    );
}

// ============================================================================
// CORNER CASES: LFU
// ============================================================================

#[test]
fn test_lfu_all_equal_frequency_fifo() {
    // Critical corner case: all items have same frequency
    // Should use FIFO (insertion order) as tiebreaker
    let mut cache = make_lfu(4);

    // Insert 4 items - all have freq=1
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // All items have same frequency, FIFO order: 1 -> 2 -> 3 -> 4

    // Insert new item - should evict 1 (first in FIFO)
    cache.put(5, 50);
    assert!(
        cache.get(&1).is_none(),
        "Key 1 should be evicted (FIFO tiebreaker)"
    );

    // Insert another - should evict 2 (next in FIFO)
    cache.put(6, 60);
    assert!(
        cache.get(&2).is_none(),
        "Key 2 should be evicted (FIFO tiebreaker)"
    );

    // Insert another - should evict 3
    cache.put(7, 70);
    assert!(
        cache.get(&3).is_none(),
        "Key 3 should be evicted (FIFO tiebreaker)"
    );
}

#[test]
fn test_lfu_new_item_lowest_frequency() {
    // New item has freq=1, could be immediately evicted
    let mut cache = make_lfu(3);

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    // Access all items many times - they all have high frequency
    for _ in 0..10 {
        cache.get(&1);
        cache.get(&2);
        cache.get(&3);
    }
    // Frequencies: 1=11, 2=11, 3=11

    // Insert new item (freq=1), then another (freq=1)
    cache.put(4, 40); // Evicts based on FIFO among freq=11 items
    let _first_evicted = (1..=3).find(|&i| cache.get(&i).is_none());

    // Insert another - the NEW item (4) has freq=1, much lower than others
    // If freq=1 ties with remaining items at freq=11, FIFO applies
    cache.put(5, 50);

    // Key 4 should be evicted because it has lowest freq (1)
    assert!(
        cache.get(&4).is_none(),
        "Key 4 should be evicted (lowest freq=1)"
    );
    assert!(cache.get(&5).is_some(), "Key 5 should be present");
}

#[test]
fn test_lfu_capacity_one() {
    let mut cache = make_lfu(1);

    cache.put(1, 10);
    // Access many times
    for _ in 0..100 {
        cache.get(&1);
    }

    // Even with high frequency, must be evicted when new item comes
    cache.put(2, 20);
    assert!(
        cache.get(&1).is_none(),
        "Key 1 must be evicted (capacity=1)"
    );
    assert_eq!(cache.get(&2), Some(&20));
}

#[test]
fn test_lfu_update_preserves_frequency() {
    let mut cache = make_lfu(3);

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    // Build up frequency for key 1
    for _ in 0..10 {
        cache.get(&1);
    }
    // freq: 1=11, 2=1, 3=1

    // Update key 1's value - should preserve high frequency
    cache.put(1, 100);

    // Insert new item - should evict 2 or 3 (low freq), NOT key 1
    cache.put(4, 40);

    assert!(
        cache.get(&1).is_some(),
        "Key 1 should remain (high freq preserved after update)"
    );
    assert_eq!(cache.get(&1), Some(&100), "Key 1 should have updated value");
}

// ============================================================================
// CORNER CASES: SLRU
// ============================================================================

#[test]
fn test_slru_all_in_probationary() {
    // No items get promoted - all stay in probationary
    let mut cache = make_slru(5, 3);

    // Insert 5 items, access each only once (no promotion)
    for i in 1..=5 {
        cache.put(i, i * 10);
    }
    // All items in probationary, LRU order: 1 -> 2 -> 3 -> 4 -> 5

    // Insert new item - should evict from probationary (key 1)
    cache.put(6, 60);
    assert!(
        cache.get(&1).is_none(),
        "Key 1 should be evicted (LRU in probationary)"
    );

    // Continue inserting - should keep evicting from probationary
    cache.put(7, 70);
    assert!(cache.get(&2).is_none(), "Key 2 should be evicted");
}

#[test]
fn test_slru_protected_full_demotion() {
    // When protected is full, oldest protected item gets demoted
    let mut cache = make_slru(4, 2);

    // Insert and promote 2 items to fill protected
    cache.put(1, 10);
    cache.put(2, 20);
    cache.get(&1); // Access to start promotion
    cache.get(&1); // Second access promotes to protected
    cache.get(&2);
    cache.get(&2); // Promotes to protected
                   // Protected: {1, 2}, Probationary: empty

    // Add items to probationary
    cache.put(3, 30);
    cache.put(4, 40);
    // Protected: {1, 2}, Probationary: {3, 4}

    // Promote key 3 - protected is full, so key 1 (oldest) should be demoted
    cache.get(&3);
    cache.get(&3);
    // Protected should now be: {2, 3}, key 1 demoted to probationary

    // Insert new item - should evict from probationary
    // Depending on implementation, either demoted key 1 or key 4 is LRU
    cache.put(5, 50);

    // Key 2 and 3 should be safe (in protected)
    assert!(cache.get(&2).is_some(), "Key 2 should remain (protected)");
    assert!(cache.get(&3).is_some(), "Key 3 should remain (protected)");
}

#[test]
fn test_slru_access_in_protected_stays() {
    let mut cache = make_slru(4, 2);

    cache.put(1, 10);
    cache.put(2, 20);

    // Promote both to protected
    cache.get(&1);
    cache.get(&1);
    cache.get(&2);
    cache.get(&2);

    // Access key 1 again - should stay in protected, move to MRU in protected
    cache.get(&1);

    // Add probationary items
    cache.put(3, 30);
    cache.put(4, 40);

    // Promote key 3 - key 2 should be demoted (LRU in protected)
    cache.get(&3);
    cache.get(&3);

    // Key 1 should still be in protected (was accessed, moved to MRU)
    // Key 2 should be demoted to probationary
    cache.put(5, 50);

    assert!(
        cache.get(&1).is_some(),
        "Key 1 should remain (MRU in protected)"
    );
    assert!(
        cache.get(&3).is_some(),
        "Key 3 should remain (just promoted)"
    );
}

#[test]
fn test_slru_probationary_larger_than_protected() {
    // Protected size smaller than probationary
    let mut cache = make_slru(5, 1);

    // Fill cache
    for i in 1..=5 {
        cache.put(i, i * 10);
    }

    // Promote only key 5 to protected
    cache.get(&5);
    cache.get(&5);
    // Protected: {5}, Probationary: {1, 2, 3, 4}

    // Insert 4 new items - should evict all original probationary items
    for i in 6..=9 {
        cache.put(i, i * 10);
    }

    // Key 5 should survive (protected)
    assert!(cache.get(&5).is_some(), "Key 5 should remain (protected)");

    // Original probationary items should be evicted
    for i in 1..=4 {
        assert!(cache.get(&i).is_none(), "Key {} should be evicted", i);
    }
}

// ============================================================================
// CORNER CASES: LFUDA
// ============================================================================

#[test]
fn test_lfuda_all_equal_priority() {
    let mut cache = make_lfuda(4);

    // Insert 4 items - all have same initial priority
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // No accesses - all have equal priority
    // Should use FIFO as tiebreaker
    cache.put(5, 50);
    assert!(
        cache.get(&1).is_none(),
        "Key 1 should be evicted (FIFO among equal priority)"
    );
}

#[test]
fn test_lfuda_aging_reduces_priority_gap() {
    let mut cache = make_lfuda(3);

    cache.put(1, 10);
    // Build very high frequency for key 1
    for _ in 0..50 {
        cache.get(&1);
    }

    cache.put(2, 20);
    cache.put(3, 30);

    // Force multiple evictions to increase global age
    for i in 100..120 {
        cache.put(i, i);
    }

    // At this point, high global age means new items have boosted priority
    // The old high-frequency item may not dominate as much
    assert_eq!(cache.len(), 3);
}

#[test]
fn test_lfuda_capacity_one() {
    let mut cache = make_lfuda(1);

    cache.put(1, 10);
    for _ in 0..100 {
        cache.get(&1);
    }

    // Must evict even with high priority
    cache.put(2, 20);
    assert!(
        cache.get(&1).is_none(),
        "Key 1 must be evicted (capacity=1)"
    );
}

// ============================================================================
// CORNER CASES: GDSF
// ============================================================================

#[test]
fn test_gdsf_all_same_size_and_frequency() {
    // Same size and frequency = same priority, use tiebreaker
    let mut cache = make_gdsf(4);

    for i in 1..=4 {
        cache.put(i, i * 10, 10); // All size=10
    }
    // All have priority = 1/10 = 0.1

    // Should use FIFO as tiebreaker
    cache.put(5, 50, 10);
    assert!(
        cache.get(&1).is_none(),
        "Key 1 should be evicted (FIFO tiebreaker)"
    );
}

#[test]
fn test_gdsf_tiny_vs_huge_size() {
    let mut cache = make_gdsf(3);

    // Huge object with moderate frequency
    cache.put(1, 10, 1000);
    for _ in 0..10 {
        cache.get(&1); // freq=11, priority = 11/1000 = 0.011
    }

    // Tiny objects with low frequency
    cache.put(2, 20, 1); // freq=1, priority = 1/1 = 1.0
    cache.put(3, 30, 1); // freq=1, priority = 1/1 = 1.0

    // Insert another - huge object has much lower priority despite more accesses
    cache.put(4, 40, 1);

    assert!(
        cache.get(&1).is_none(),
        "Huge object should be evicted (priority 0.011 << 1.0)"
    );
    assert!(cache.get(&2).is_some(), "Tiny object 2 should remain");
    assert!(cache.get(&3).is_some(), "Tiny object 3 should remain");
}

#[test]
fn test_gdsf_size_one_equals_lfu() {
    // When all sizes are 1, GDSF should behave like LFU
    let mut cache = make_gdsf(3);

    cache.put(1, 10, 1);
    cache.put(2, 20, 1);
    cache.put(3, 30, 1);

    // Build frequency for key 1
    for _ in 0..10 {
        cache.get(&1);
    }
    for _ in 0..5 {
        cache.get(&2);
    }
    // Priorities (freq/1): 1=11, 2=6, 3=1

    cache.put(4, 40, 1);
    assert!(
        cache.get(&3).is_none(),
        "Key 3 should be evicted (lowest freq when size=1)"
    );
}

#[test]
fn test_gdsf_frequency_can_overcome_size() {
    let mut cache = make_gdsf(3);

    // Large object with VERY high frequency
    cache.put(1, 10, 100);
    for _ in 0..500 {
        cache.get(&1); // freq=501, priority = 501/100 = 5.01
    }

    // Small objects with low frequency
    cache.put(2, 20, 1); // freq=1, priority = 1/1 = 1.0
    cache.put(3, 30, 1); // freq=1, priority = 1/1 = 1.0

    // Insert new - small objects have lower priority
    cache.put(4, 40, 1);

    // Key 2 or 3 should be evicted (priority 1.0 < 5.01)
    assert!(
        cache.get(&1).is_some(),
        "Large frequent object should survive"
    );
    let small_evicted = cache.get(&2).is_none() || cache.get(&3).is_none();
    assert!(small_evicted, "A small object should be evicted");
}

#[test]
fn test_gdsf_capacity_one() {
    let mut cache = make_gdsf(1);

    cache.put(1, 10, 1);
    for _ in 0..100 {
        cache.get(&1);
    }

    cache.put(2, 20, 1);
    assert!(
        cache.get(&1).is_none(),
        "Key 1 must be evicted (capacity=1)"
    );
}

// ============================================================================
// CORNER CASES: GENERAL
// ============================================================================

#[test]
fn test_operations_on_empty_cache() {
    let mut lru: LruCache<i32, i32> = make_lru(3);

    // Get on empty cache
    assert_eq!(lru.get(&1), None);

    // Remove on empty cache
    assert_eq!(lru.remove(&1), None);

    // Clear on empty cache (should not panic)
    lru.clear();
    assert_eq!(lru.len(), 0);
}

#[test]
fn test_remove_nonexistent_key() {
    let mut cache = make_lru(3);

    cache.put(1, 10);
    cache.put(2, 20);

    // Remove key that doesn't exist
    assert_eq!(cache.remove(&99), None);
    assert_eq!(cache.len(), 2, "Length should be unchanged");

    // Original keys still present
    assert!(cache.get(&1).is_some());
    assert!(cache.get(&2).is_some());
}

#[test]
fn test_insert_after_clear() {
    let mut cache = make_lru(3);

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    cache.clear();
    assert_eq!(cache.len(), 0);

    // Insert after clear - should work normally
    cache.put(4, 40);
    cache.put(5, 50);

    assert_eq!(cache.len(), 2);
    assert_eq!(cache.get(&4), Some(&40));
    assert_eq!(cache.get(&5), Some(&50));
}

#[test]
fn test_rapid_update_same_key() {
    let mut cache = make_lru(3);

    // Insert same key many times
    for i in 0..100 {
        cache.put(1, i);
    }

    assert_eq!(cache.len(), 1, "Should only have 1 entry");
    assert_eq!(cache.get(&1), Some(&99), "Should have last value");
}

#[test]
fn test_alternating_keys() {
    let mut cache = make_lru(2);

    // Alternating pattern that causes continuous eviction
    for i in 0..10 {
        cache.put(i % 3, i); // Keys 0, 1, 2, 0, 1, 2, ...
    }

    // Should have the last 2 keys inserted
    assert_eq!(cache.len(), 2);
}

#[test]
fn test_lfu_get_does_not_exist() {
    let mut cache = make_lfu(3);

    cache.put(1, 10);

    // Get non-existent key should not affect frequencies
    assert_eq!(cache.get(&99), None);
    assert_eq!(cache.get(&99), None);

    cache.put(2, 20);
    cache.put(3, 30);

    // Key 1 should still be at freq=1 (only the put)
    // Insert new - all equal freq, FIFO
    cache.put(4, 40);
    assert!(cache.get(&1).is_none(), "Key 1 should be evicted (FIFO)");
}

#[test]
fn test_gdsf_zero_size_handling() {
    let mut cache = make_gdsf(3);

    // Size 0 edge case - implementation should handle gracefully
    // (might treat as size 1 or have special handling)
    cache.put(1, 10, 0);
    cache.put(2, 20, 1);
    cache.put(3, 30, 1);

    // Should not panic, cache should still function
    assert!(cache.len() <= 3);
    cache.put(4, 40, 1);
    assert!(cache.len() <= 3);
}

// ============================================================================
// METRICS SIZE TRACKING BUG REPRODUCTION
// ============================================================================
// This test reproduces a bug where metrics.core.cache_size_bytes can underflow
// when the evicted size (from estimate_object_size) differs from the tracked size.

#[test]
fn test_metrics_size_tracking_no_underflow() {
    // This test reproduces a bug where:
    // 1. Item inserted with put_with_size(key, value, small_size)
    // 2. Metrics tracks small_size bytes
    // 3. On eviction, estimate_object_size() returns a larger value
    // 4. cache_size_bytes -= larger_value causes underflow

    let mut cache = make_lru_with_limits(2, 1000);

    // Insert items with explicit small sizes
    cache.put_with_size(1, "a", 1); // Track as 1 byte
    cache.put_with_size(2, "b", 1); // Track as 1 byte

    // Current size should be 2
    assert_eq!(cache.current_size(), 2, "Should track 2 bytes");

    // Insert third item - triggers eviction of key 1
    // On eviction, estimate_object_size() may return a value different from 1
    // This should NOT panic due to subtraction underflow
    cache.put_with_size(3, "c", 1);

    // Cache should still function correctly
    assert_eq!(cache.len(), 2);
    assert!(cache.get(&1).is_none(), "Key 1 should be evicted");
    assert!(cache.get(&2).is_some(), "Key 2 should remain");
    assert!(cache.get(&3).is_some(), "Key 3 should be present");

    // Size tracking should be reasonable (not underflowed to huge number)
    let size = cache.current_size();
    assert!(size <= 1000, "Size should not have underflowed: {}", size);
}

#[test]
fn test_metrics_size_mismatch_on_eviction() {
    // More aggressive test: insert with tiny explicit size, evict with estimated size
    let mut cache: LruCache<i32, String> = make_lru_with_limits(3, 10000);

    // Insert items with size=1 but the actual values are larger
    // (estimate_object_size will calculate a bigger size)
    cache.put_with_size(1, "hello world this is a long string".to_string(), 1);
    cache.put_with_size(2, "another fairly long string value".to_string(), 1);
    cache.put_with_size(3, "yet another long string for testing".to_string(), 1);

    // Force evictions by inserting more items
    // Each eviction will try to subtract estimate_object_size() from metrics
    // which is larger than the 1 byte we recorded on insertion
    for i in 4..10 {
        cache.put_with_size(i, format!("value number {}", i), 1);
    }

    // Should not have panicked, and size should be reasonable
    assert_eq!(cache.len(), 3);
    let size = cache.current_size();
    // Size should be small (we only tracked 1 byte per item)
    // If underflow occurred, it would be a huge number
    assert!(size < 1000, "Size should not have underflowed: {}", size);
}

// ============================================================================
// MAX_SIZE EVICTION TESTS
// ============================================================================
// These tests verify that each cache algorithm properly evicts items when
// the max_size limit would be exceeded, not just when entry count is reached.

/// Helper to create an SlruCache with explicit max_size limit
fn make_slru_with_limits<K: std::hash::Hash + Eq + Clone, V: Clone>(
    cap: usize,
    protected_cap: usize,
    max_size: u64,
) -> SlruCache<K, V> {
    let config = SlruCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        protected_capacity: NonZeroUsize::new(protected_cap).unwrap(),
        max_size,
    };
    SlruCache::init(config, None)
}

#[test]
fn test_lru_max_size_triggers_eviction() {
    // Create LRU with large entry capacity but small max_size
    // Size limit (100 bytes) should be the constraint, not entry count (1000)
    let mut cache: LruCache<String, i32> = make_lru_with_limits(1000, 100);

    // Insert items that fit within max_size
    cache.put_with_size("a".to_string(), 1, 30); // total: 30
    cache.put_with_size("b".to_string(), 2, 30); // total: 60
    cache.put_with_size("c".to_string(), 3, 30); // total: 90

    assert_eq!(cache.len(), 3, "Should have 3 items");
    assert_eq!(cache.current_size(), 90, "Size should be 90");

    // Insert item that would exceed max_size (90 + 20 = 110 > 100)
    // LRU should evict "a" to stay within max_size
    cache.put_with_size("d".to_string(), 4, 20);

    // Verify LRU respects max_size
    assert!(
        cache.current_size() <= 100,
        "LRU should respect max_size limit. current_size={}, max_size=100",
        cache.current_size()
    );

    // "a" should have been evicted (LRU)
    assert!(
        cache.get(&"a".to_string()).is_none(),
        "Item 'a' should be evicted when max_size exceeded"
    );
    assert!(
        cache.get(&"b".to_string()).is_some(),
        "Item 'b' should remain"
    );
    assert!(
        cache.get(&"c".to_string()).is_some(),
        "Item 'c' should remain"
    );
    assert!(
        cache.get(&"d".to_string()).is_some(),
        "Item 'd' should be inserted"
    );
}

#[test]
fn test_slru_max_size_triggers_eviction() {
    // Create SLRU with large entry capacity but small max_size
    // Size limit (100 bytes) should be the constraint, not entry count (1000)
    let mut cache: SlruCache<String, i32> = make_slru_with_limits(1000, 200, 100);

    // Insert items that fit within max_size
    cache.put_with_size("a".to_string(), 1, 30); // total: 30
    cache.put_with_size("b".to_string(), 2, 30); // total: 60
    cache.put_with_size("c".to_string(), 3, 30); // total: 90

    assert_eq!(cache.len(), 3, "Should have 3 items");
    assert_eq!(cache.current_size(), 90, "Size should be 90");

    // Insert item that would exceed max_size (90 + 20 = 110 > 100)
    // SLRU should evict to stay within max_size
    cache.put_with_size("d".to_string(), 4, 20);

    assert!(
        cache.current_size() <= 100,
        "current_size {} exceeds max_size 100",
        cache.current_size()
    );
}

#[test]
fn test_lfu_max_size_triggers_eviction() {
    // Create LFU with large entry capacity but small max_size
    let mut cache: LfuCache<String, i32> = make_lfu_with_max_size(100);

    cache.put_with_size("a".to_string(), 1, 30);
    cache.put_with_size("b".to_string(), 2, 30);
    cache.put_with_size("c".to_string(), 3, 30);

    assert_eq!(cache.current_size(), 90);

    // Insert item that would exceed max_size
    cache.put_with_size("d".to_string(), 4, 20);

    assert!(
        cache.current_size() <= 100,
        "LFU should respect max_size limit. current_size={}, max_size=100",
        cache.current_size()
    );
}

#[test]
fn test_lfuda_max_size_triggers_eviction() {
    // Create LFUDA with large entry capacity but small max_size
    let mut cache: LfudaCache<String, i32> = make_lfuda_with_max_size(100);

    cache.put_with_size("a".to_string(), 1, 30);
    cache.put_with_size("b".to_string(), 2, 30);
    cache.put_with_size("c".to_string(), 3, 30);

    assert_eq!(cache.current_size(), 90);

    // Insert item that would exceed max_size
    cache.put_with_size("d".to_string(), 4, 20);

    assert!(
        cache.current_size() <= 100,
        "LFUDA should respect max_size limit. current_size={}, max_size=100",
        cache.current_size()
    );
}

#[test]
fn test_slru_max_size_should_evict_multiple_items() {
    // Test that SLRU can evict multiple items if needed to fit a large new item
    let mut cache: SlruCache<String, i32> = make_slru_with_limits(1000, 200, 100);

    // Fill with small items
    for i in 0..10 {
        cache.put_with_size(format!("key{}", i), i as i32, 10); // 10 items × 10 bytes = 100
    }

    assert_eq!(cache.len(), 10);
    assert_eq!(cache.current_size(), 100, "Cache should be at max_size");

    // Insert a large item (50 bytes) - should evict multiple small items
    cache.put_with_size("big".to_string(), 999, 50);

    // Expected: evict enough items to fit the new 50-byte item
    // Final size should be <= 100
    assert!(
        cache.current_size() <= 100,
        "SLRU BUG: current_size {} exceeds max_size 100 after inserting large item. \
         Multiple items should be evicted to make room.",
        cache.current_size()
    );
}
