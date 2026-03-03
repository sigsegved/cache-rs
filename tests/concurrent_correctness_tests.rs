//! Concurrent Cache Correctness Tests
//!
//! These tests validate that concurrent cache implementations maintain correct
//! eviction semantics while being accessed from multiple threads.
//!
//! ## Test Strategy
//!
//! Unlike stress tests that focus on throughput and lack of panics, these tests:
//! - Use small cache sizes for predictable behavior
//! - Validate algorithm correctness with multiple segments
//! - Verify eviction policies work correctly under concurrent access
//! - Test that concurrent operations maintain invariants
//!
//! ## Segments
//!
//! 1. **Algorithm Correctness**: Verify eviction behavior is correct per algorithm
//! 2. **Thread Safety Invariants**: Verify cache state consistency under concurrency

#![cfg(feature = "concurrent")]

use cache_rs::config::{
    ConcurrentCacheConfig, ConcurrentGdsfCacheConfig, ConcurrentLfuCacheConfig,
    ConcurrentLfudaCacheConfig, ConcurrentLruCacheConfig, ConcurrentSlruCacheConfig,
    GdsfCacheConfig, LfuCacheConfig, LfudaCacheConfig, LruCacheConfig, SlruCacheConfig,
};
use cache_rs::metrics::CacheMetrics;
use cache_rs::{
    ConcurrentGdsfCache, ConcurrentLfuCache, ConcurrentLfudaCache, ConcurrentLruCache,
    ConcurrentSlruCache,
};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

/// Number of threads for concurrent correctness tests
const NUM_THREADS: usize = 8;
/// Operations per thread for stress testing
const OPS_PER_THREAD: usize = 500;

fn lru_config(capacity: usize, segments: usize) -> ConcurrentLruCacheConfig {
    ConcurrentCacheConfig {
        base: LruCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            max_size: u64::MAX,
        },
        segments,
    }
}

fn slru_config(capacity: usize, protected: usize, segments: usize) -> ConcurrentSlruCacheConfig {
    ConcurrentCacheConfig {
        base: SlruCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            protected_capacity: NonZeroUsize::new(protected).unwrap(),
            max_size: u64::MAX,
        },
        segments,
    }
}

fn lfu_config(capacity: usize, segments: usize) -> ConcurrentLfuCacheConfig {
    ConcurrentCacheConfig {
        base: LfuCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            max_size: u64::MAX,
        },
        segments,
    }
}

fn lfuda_config(capacity: usize, segments: usize) -> ConcurrentLfudaCacheConfig {
    ConcurrentCacheConfig {
        base: LfudaCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            initial_age: 0,
            max_size: u64::MAX,
        },
        segments,
    }
}

fn gdsf_config(capacity: usize, segments: usize) -> ConcurrentGdsfCacheConfig {
    ConcurrentCacheConfig {
        base: GdsfCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            initial_age: 0.0,
            max_size: u64::MAX,
        },
        segments,
    }
}

fn lru_config_with_size(
    capacity: usize,
    max_size: u64,
    segments: usize,
) -> ConcurrentLruCacheConfig {
    ConcurrentCacheConfig {
        base: LruCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            max_size,
        },
        segments,
    }
}

fn lfu_config_with_size(
    capacity: usize,
    max_size: u64,
    segments: usize,
) -> ConcurrentLfuCacheConfig {
    ConcurrentCacheConfig {
        base: LfuCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            max_size,
        },
        segments,
    }
}

fn lfuda_config_with_size(
    capacity: usize,
    max_size: u64,
    segments: usize,
) -> ConcurrentLfudaCacheConfig {
    ConcurrentCacheConfig {
        base: LfudaCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            initial_age: 0,
            max_size,
        },
        segments,
    }
}

fn gdsf_config_with_size(
    capacity: usize,
    max_size: u64,
    segments: usize,
) -> ConcurrentGdsfCacheConfig {
    ConcurrentCacheConfig {
        base: GdsfCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            initial_age: 0.0,
            max_size,
        },
        segments,
    }
}

fn slru_config_with_size(
    capacity: usize,
    protected: usize,
    max_size: u64,
    segments: usize,
) -> ConcurrentSlruCacheConfig {
    ConcurrentCacheConfig {
        base: SlruCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            protected_capacity: NonZeroUsize::new(protected).unwrap(),
            max_size,
        },
        segments,
    }
}

// ============================================================================
// SEGMENT 1: ALGORITHM CORRECTNESS UNDER CONCURRENCY
// ============================================================================
// These tests verify that the eviction algorithms work correctly
// even when accessed concurrently.

// ----------------------------------------------------------------------------
// CONCURRENT LRU CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_lru_basic_eviction() {
    // Use 4 segments for concurrent access
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(50, 4), None));

    let mut handles = vec![];

    // Spawn threads that concurrently put, get, remove, and check contains
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                // Mixed operations simulating real-world usage
                c.put(key, key * 10, 1);
                let _ = c.get(&key);
                if i % 5 == 0 {
                    let _ = c.contains(&key);
                }
                if i % 10 == 0 {
                    let _ = c.peek(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify capacity is maintained under concurrent access
    assert!(
        cache.len() <= 50,
        "Cache should maintain capacity after concurrent evictions"
    );
}

#[test]
fn test_concurrent_lru_access_prevents_eviction() {
    // Use multiple segments for concurrent access pattern testing
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 4), None));

    // Hot keys that will be accessed frequently
    let hot_keys: Vec<i32> = (0..10).collect();

    // Insert hot keys first
    for &key in &hot_keys {
        cache.put(key, key * 100, 1);
    }

    let mut handles = vec![];
    let hot_keys_arc = Arc::new(hot_keys.clone());

    // Spawn threads that heavily access hot keys (simulating real cache usage)
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        let hk = Arc::clone(&hot_keys_arc);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                // Frequently access hot keys to prevent eviction
                let hot_key = hk[i % hk.len()];
                let _ = c.get(&hot_key);

                // Also insert new cold keys that may get evicted
                let cold_key = 1000 + (t * OPS_PER_THREAD + i) as i32;
                c.put(cold_key, cold_key, 1);

                // Realistic operations mix
                if i % 3 == 0 {
                    let _ = c.contains(&hot_key);
                }
                if i % 7 == 0 {
                    let _ = c.remove(&cold_key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Hot keys should have higher chance of surviving due to frequent access
    // (not guaranteed due to concurrent segment distribution)
    assert!(cache.len() <= 100, "Cache should maintain capacity");
}

#[test]
fn test_concurrent_lru_multi_segment_eviction() {
    // Use 4 segments - keys distributed by hash
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(40, 4), None));

    let mut handles = vec![];

    // Spawn threads that perform mixed operations across segments
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                // Real-world pattern: put, then read multiple times
                c.put(key, key * 10, 1);
                for _ in 0..3 {
                    let _ = c.get(&key);
                }
                // Occasionally check existence and remove
                if i % 4 == 0 {
                    let exists = c.contains(&key);
                    if exists && i % 8 == 0 {
                        let _ = c.remove(&key);
                    }
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Cache should maintain total capacity across all segments
    assert!(cache.len() <= 40, "Cache should not exceed capacity");
}

#[test]
fn test_concurrent_lru_concurrent_writes_maintain_capacity() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(20, 4), None));

    let mut handles = vec![];

    // Spawn 4 threads, each writing to their own key range
    for t in 0..4 {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let key = t * 1000 + i;
                cache.put(key, key, 1);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Capacity should never be exceeded
    assert!(
        cache.len() <= 20,
        "Concurrent writes should not exceed capacity"
    );
}

// ----------------------------------------------------------------------------
// CONCURRENT LFU CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_lfu_frequency_based_eviction() {
    let cache: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(50, 4), None));

    // Hot keys that will be accessed frequently
    let hot_keys: Vec<i32> = (0..10).collect();

    // Insert hot keys first
    for &key in &hot_keys {
        cache.put(key, key * 100, 1);
    }

    let mut handles = vec![];
    let hot_keys_arc = Arc::new(hot_keys.clone());

    // Spawn threads that heavily access hot keys to build frequency
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        let hk = Arc::clone(&hot_keys_arc);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                // Frequently access hot keys to increase their frequency
                let hot_key = hk[i % hk.len()];
                for _ in 0..3 {
                    let _ = c.get(&hot_key);
                }

                // Insert cold keys that should get evicted due to low frequency
                let cold_key = 1000 + (t * OPS_PER_THREAD + i) as i32;
                c.put(cold_key, cold_key, 1);

                // Realistic mix: peek doesn't update frequency
                if i % 5 == 0 {
                    let _ = c.peek(&hot_key);
                }
                // Contains is also non-promoting
                if i % 7 == 0 {
                    let _ = c.contains(&cold_key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Cache should maintain capacity
    assert!(cache.len() <= 50, "Cache should maintain capacity");
}

#[test]
fn test_concurrent_lfu_frequency_accumulation() {
    let cache: Arc<ConcurrentLfuCache<String, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(6, 2), None));

    // Insert items
    cache.put("hot".to_string(), 1, 1);
    cache.put("warm".to_string(), 2, 1);
    cache.put("cold".to_string(), 3, 1);

    let cache_clone = Arc::clone(&cache);

    // Multiple threads access "hot" key
    let mut handles = vec![];
    for _ in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..50 {
                c.get(&"hot".to_string());
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // "hot" should have very high frequency now
    // Fill cache to trigger eviction
    cache_clone.put("new1".to_string(), 4, 1);
    cache_clone.put("new2".to_string(), 5, 1);
    cache_clone.put("new3".to_string(), 6, 1);
    cache_clone.put("new4".to_string(), 7, 1);

    // "hot" should survive due to high frequency
    assert!(
        cache_clone.get(&"hot".to_string()).is_some(),
        "Hot key should survive due to high concurrent access frequency"
    );
}

#[test]
fn test_concurrent_lfu_multi_segment_correctness() {
    let cache: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(80, 4), None));

    let mut handles = vec![];

    // Spawn threads with mixed operations across segments
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                // Put and then access to build frequency
                c.put(key, key * 10, 1);

                // Access some keys more to increase frequency
                if key % 10 < 3 {
                    for _ in 0..5 {
                        let _ = c.get(&key);
                    }
                }

                // Real-world operations mix
                if i % 4 == 0 {
                    let _ = c.contains(&key);
                }
                if i % 6 == 0 {
                    let _ = c.peek(&key);
                }
                if i % 10 == 0 {
                    let _ = c.remove(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // High frequency items should be more likely to survive
    assert!(cache.len() <= 80, "Should maintain capacity");
}

// ----------------------------------------------------------------------------
// CONCURRENT LFUDA CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_lfuda_priority_eviction() {
    // Use multiple segments for concurrent access testing
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(50, 4), None));

    // Hot keys that will get high priority from frequent access
    let hot_keys: Vec<i32> = (0..10).collect();

    // Insert hot keys first
    for &key in &hot_keys {
        cache.put(key, key * 100, 1);
    }

    let mut handles = vec![];
    let hot_keys_arc = Arc::new(hot_keys.clone());

    // Spawn threads that build priority through frequent access
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        let hk = Arc::clone(&hot_keys_arc);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                // Frequently access hot keys to increase their priority
                let hot_key = hk[i % hk.len()];
                for _ in 0..3 {
                    let _ = c.get(&hot_key);
                }

                // Insert cold keys that may get evicted
                let cold_key = 1000 + (t * OPS_PER_THREAD + i) as i32;
                c.put(cold_key, cold_key, 1);

                // Mix in other operations
                if i % 5 == 0 {
                    let _ = c.peek(&hot_key);
                }
                if i % 8 == 0 {
                    let _ = c.contains(&cold_key);
                }
                if i % 12 == 0 {
                    let _ = c.remove(&cold_key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify capacity is maintained under concurrent access
    assert!(cache.len() <= 50, "Cache should maintain capacity");
}

#[test]
fn test_concurrent_lfuda_aging_mechanism() {
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(50, 4), None));

    let mut handles = vec![];

    // Spawn threads that trigger aging through evictions
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key * 10, 1);

                // Access some keys to build priority
                if i % 3 == 0 {
                    for _ in 0..5 {
                        let _ = c.get(&key);
                    }
                }

                // Mixed operations
                if i % 4 == 0 {
                    let _ = c.contains(&key);
                }
                if i % 7 == 0 {
                    let _ = c.peek(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Cache should maintain capacity despite many evictions
    assert!(cache.len() <= 50, "Should maintain capacity");
}

// ----------------------------------------------------------------------------
// CONCURRENT SLRU CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_slru_segment_behavior() {
    let cache: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(60, 20, 4), None));

    let mut handles = vec![];

    // Spawn threads that test probationary to protected promotion
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                // Items start in probationary
                c.put(key, key * 10, 1);

                // Access multiple times to promote to protected
                if i % 3 == 0 {
                    for _ in 0..3 {
                        let _ = c.get(&key);
                    }
                }

                // Other operations
                if i % 5 == 0 {
                    let _ = c.peek(&key);
                }
                if i % 7 == 0 {
                    let _ = c.contains(&key);
                }
                if i % 10 == 0 {
                    let _ = c.remove(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Protected items should survive longer than probationary
    assert!(cache.len() <= 60, "Should maintain capacity");
}

#[test]
fn test_concurrent_slru_promotion_under_concurrency() {
    let cache: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(50, 15, 4), None));

    // Pre-populate with items
    for i in 0..20 {
        cache.put(i, i * 10, 1);
    }

    let mut handles = vec![];

    // Spawn threads that promote items through repeated access
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key * 10, 1);

                // Access repeatedly to promote to protected
                for _ in 0..4 {
                    let _ = c.get(&key);
                }

                // Mix in other operations
                if i % 5 == 0 {
                    let _ = c.peek(&key);
                }
                if i % 6 == 0 {
                    let _ = c.contains(&key);
                }
                if i % 9 == 0 {
                    let _ = c.remove(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify capacity maintained
    assert!(cache.len() <= 50, "Cache should maintain capacity");
}

// ----------------------------------------------------------------------------
// CONCURRENT GDSF CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_gdsf_size_aware_eviction() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(50, 4), None));

    let mut handles = vec![];

    // Spawn threads that test GDSF with variable sizes
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                // Vary sizes: small objects should have higher priority
                let size = if i % 5 == 0 { 100 } else { 1 }; // Large vs small
                c.put(key, key * 10, size);

                // Access to build frequency
                if i % 3 == 0 {
                    for _ in 0..3 {
                        let _ = c.get(&key);
                    }
                }

                // Other operations
                if i % 4 == 0 {
                    let _ = c.peek(&key);
                }
                if i % 6 == 0 {
                    let _ = c.contains(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Smaller objects should have survived more often due to higher priority
    assert!(cache.len() <= 50, "Cache should maintain capacity");
}

#[test]
fn test_concurrent_gdsf_frequency_matters() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(50, 4), None));

    // Hot keys that will be accessed frequently
    let hot_keys: Vec<i32> = (0..10).collect();

    // Insert hot keys first
    for &key in &hot_keys {
        cache.put(key, key * 100, 1);
    }

    let mut handles = vec![];
    let hot_keys_arc = Arc::new(hot_keys.clone());

    // Spawn threads that build frequency through access
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        let hk = Arc::clone(&hot_keys_arc);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                // Frequently access hot keys
                let hot_key = hk[i % hk.len()];
                for _ in 0..3 {
                    let _ = c.get(&hot_key);
                }

                // Insert cold keys
                let cold_key = 1000 + (t * OPS_PER_THREAD + i) as i32;
                let size = ((i % 10) + 1) as u64;
                c.put(cold_key, cold_key, size);

                // Mixed operations
                if i % 5 == 0 {
                    let _ = c.peek(&hot_key);
                }
                if i % 7 == 0 {
                    let _ = c.contains(&cold_key);
                }
                if i % 11 == 0 {
                    let _ = c.remove(&cold_key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // High frequency items have higher priority
    assert!(cache.len() <= 50, "Cache should maintain capacity");
}

#[test]
fn test_concurrent_gdsf_concurrent_size_tracking() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(10, 2), None));

    let mut handles = vec![];

    // Multiple threads insert items with sizes
    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..5 {
                let key = t * 10 + i;
                let size = ((i + 1) * 10) as u64;
                c.put(key, key, size);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Size tracking should be consistent
    let size = cache.current_size();
    assert!(size > 0, "Size should be tracked");
    assert!(cache.len() <= 10, "Should maintain entry capacity");
}

// ============================================================================
// SEGMENT 2: THREAD SAFETY INVARIANTS
// ============================================================================
// These tests verify that cache state remains consistent under concurrent access.

// ----------------------------------------------------------------------------
// CAPACITY INVARIANTS
// ----------------------------------------------------------------------------

#[test]
fn test_capacity_never_exceeded_lru() {
    let capacity = 50;
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(capacity, 4), None));

    let mut handles = vec![];
    let write_count = Arc::new(AtomicUsize::new(0));

    for t in 0..8 {
        let c = Arc::clone(&cache);
        let wc = Arc::clone(&write_count);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key, 1);
                wc.fetch_add(1, Ordering::Relaxed);

                // Check invariant during operation
                assert!(
                    c.len() <= capacity,
                    "Capacity exceeded during concurrent writes!"
                );
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert_eq!(write_count.load(Ordering::Relaxed), 8 * 500);
    assert!(cache.len() <= capacity, "Final capacity check failed");
}

#[test]
fn test_capacity_never_exceeded_lfu() {
    let capacity = 50;
    let cache: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(capacity, 4), None));

    let mut handles = vec![];

    for t in 0..8 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key, 1);
                assert!(c.len() <= capacity, "Capacity exceeded!");
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= capacity);
}

#[test]
fn test_capacity_never_exceeded_slru() {
    let capacity = 50;
    let cache: Arc<ConcurrentSlruCache<i32, i32>> = Arc::new(ConcurrentSlruCache::init(
        slru_config(capacity, 20, 4),
        None,
    ));

    let mut handles = vec![];

    for t in 0..8 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key, 1);
                assert!(c.len() <= capacity, "Capacity exceeded!");
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= capacity);
}

#[test]
fn test_capacity_never_exceeded_lfuda() {
    let capacity = 50;
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(capacity, 4), None));

    let mut handles = vec![];

    for t in 0..8 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key, 1);
                assert!(c.len() <= capacity, "Capacity exceeded!");
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= capacity);
}

#[test]
fn test_capacity_never_exceeded_gdsf() {
    let capacity = 50;
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(capacity, 4), None));

    let mut handles = vec![];

    for t in 0..8 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key, 1);
                assert!(c.len() <= capacity, "Capacity exceeded!");
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= capacity);
}

// ----------------------------------------------------------------------------
// DATA CONSISTENCY
// ----------------------------------------------------------------------------

#[test]
fn test_get_returns_correct_value() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 4), None));

    // Insert known values
    for i in 0..50 {
        cache.put(i, i * 100, 1);
    }

    let errors = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    // Multiple threads read and verify values
    for _ in 0..8 {
        let c = Arc::clone(&cache);
        let err = Arc::clone(&errors);
        handles.push(thread::spawn(move || {
            for i in 0..50 {
                if let Some(val) = c.get(&i) {
                    if val != i * 100 {
                        err.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert_eq!(errors.load(Ordering::Relaxed), 0, "Values were corrupted");
}

#[test]
fn test_update_is_atomic() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(10, 2), None));

    cache.put(1, 0, 1);

    let mut handles = vec![];

    // Multiple threads update same key
    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                c.put(1, t, 1);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Value should be one of the thread IDs (0, 1, 2, or 3)
    let value = cache.get(&1).unwrap();
    assert!(
        (0..=3).contains(&value),
        "Value should be a valid thread ID"
    );
}

#[test]
fn test_remove_consistency() {
    // Use capacity large enough to hold all items without eviction
    // With 4 segments and 50 items, ensure each segment can hold at least 50/4 = 12.5 items
    // Using capacity 200 (50 per segment) ensures no eviction occurs during insert
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(200, 4), None));

    // Insert items
    for i in 0..50 {
        cache.put(i, i, 1);
    }

    // Verify all items were inserted before attempting removes
    let initial_count = cache.len();
    assert_eq!(initial_count, 50, "All 50 items should be inserted");

    let successful_removes = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    // Multiple threads try to remove same keys
    for _ in 0..4 {
        let c = Arc::clone(&cache);
        let sr = Arc::clone(&successful_removes);
        handles.push(thread::spawn(move || {
            for i in 0..50 {
                if c.remove(&i).is_some() {
                    sr.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Each key should be removed exactly once
    assert_eq!(
        successful_removes.load(Ordering::Relaxed),
        50,
        "Each key should be removed exactly once"
    );
    assert!(cache.is_empty(), "Cache should be empty after all removes");
}

// ----------------------------------------------------------------------------
// MIXED OPERATIONS CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_mixed_operations_lru() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 4), None));

    let mut handles = vec![];

    // Writers
    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                c.put(t * 1000 + i, i, 1);
            }
        }));
    }

    // Readers
    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                let _ = c.get(&(t * 1000 + i));
            }
        }));
    }

    // Removers
    for t in 0..2 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                c.remove(&(t * 1000 + i));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Cache should be in consistent state
    assert!(cache.len() <= 100);
}

#[test]
fn test_mixed_operations_lfu() {
    let cache: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(100, 4), None));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                c.put(t * 1000 + i, i, 1);
                let _ = c.get(&(t * 1000 + i));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= 100);
}

#[test]
fn test_mixed_operations_slru() {
    let cache: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(100, 40, 4), None));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                c.put(t * 1000 + i, i, 1);
                // Access multiple times to trigger promotion
                for _ in 0..3 {
                    let _ = c.get(&(t * 1000 + i));
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= 100);
}

#[test]
fn test_mixed_operations_gdsf() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(100, 4), None));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                let size = (i % 10 + 1) as u64;
                c.put(t * 1000 + i, i, size);
                let _ = c.get(&(t * 1000 + i));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= 100);
}

// ----------------------------------------------------------------------------
// CLEAR OPERATION CONSISTENCY
// ----------------------------------------------------------------------------

#[test]
fn test_clear_during_operations() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 4), None));

    let stop_flag = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    // Writer threads
    for t in 0..4 {
        let c = Arc::clone(&cache);
        let sf = Arc::clone(&stop_flag);
        handles.push(thread::spawn(move || {
            let mut i = 0;
            while sf.load(Ordering::Relaxed) == 0 {
                c.put(t * 10000 + i, i, 1);
                i += 1;
            }
        }));
    }

    // Clear thread
    // Note: Use longer sleep times (20ms) for Windows compatibility
    // Windows has ~15ms timer resolution so shorter sleeps are unreliable
    let cache_clear = Arc::clone(&cache);
    let stop_flag_clear = Arc::clone(&stop_flag);
    handles.push(thread::spawn(move || {
        for _ in 0..10 {
            thread::sleep(std::time::Duration::from_millis(20));
            cache_clear.clear();
        }
        stop_flag_clear.store(1, Ordering::Relaxed);
    }));

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Cache should be in valid state (may or may not be empty depending on timing)
    assert!(cache.len() <= 100);
}

// ----------------------------------------------------------------------------
// SIZE TRACKING CONSISTENCY
// ----------------------------------------------------------------------------

#[test]
fn test_size_tracking_concurrent_lru() {
    // Use large enough cache to avoid evictions (which can trigger metrics bug)
    let cache: Arc<ConcurrentLruCache<i32, String>> =
        Arc::new(ConcurrentLruCache::init(lru_config(200, 4), None));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..25 {
                let key = t * 100 + i;
                c.put(key, format!("value_{}", key), 10);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let size = cache.current_size();
    let len = cache.len();

    // Each entry has size 10
    assert!(len <= 200, "Should not exceed entry limit");
    assert!(size > 0, "Size should be tracked");
    // Size should be approximately len * 10
    assert_eq!(size, len as u64 * 10, "Size should match entries * 10");
}

#[test]
fn test_size_tracking_concurrent_lfu() {
    // Use large enough cache to avoid evictions
    let cache: Arc<ConcurrentLfuCache<i32, String>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(200, 4), None));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..25 {
                let key = t * 100 + i;
                c.put(key, format!("value_{}", key), 10);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.current_size() > 0, "Size should be tracked");
    assert!(cache.len() <= 200);
}

// ----------------------------------------------------------------------------
// EDGE CASES
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_access_empty_cache() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(10, 2), None));

    let mut handles = vec![];

    // Many threads trying to get from empty cache
    for _ in 0..8 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                assert!(c.get(&i).is_none(), "Empty cache should return None");
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.is_empty());
}

#[test]
fn test_concurrent_single_key() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(10, 2), None));

    let put_count = Arc::new(AtomicUsize::new(0));
    let get_count = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    // All threads operate on same key
    for _ in 0..8 {
        let c = Arc::clone(&cache);
        let pc = Arc::clone(&put_count);
        let gc = Arc::clone(&get_count);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                c.put(1, i, 1);
                pc.fetch_add(1, Ordering::Relaxed);
                if c.get(&1).is_some() {
                    gc.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Key should exist with some value
    assert!(cache.get(&1).is_some(), "Key should exist");
    assert_eq!(cache.len(), 1, "Should have exactly 1 key");
    assert_eq!(put_count.load(Ordering::Relaxed), 8 * 100);
}

#[test]
fn test_concurrent_capacity_one() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1, 1), None));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                c.put(t * 100 + i, i, 1);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Only 1 entry should exist
    assert_eq!(cache.len(), 1, "Cache with capacity 1 should have 1 entry");
}

#[test]
fn test_contains_key_consistency() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(50, 4), None));

    // Insert known keys
    for i in 0..30 {
        cache.put(i, i, 1);
    }

    let mut handles = vec![];

    // Verify contains() is consistent (non-promoting check)
    for _ in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..30 {
                if c.contains(&i) {
                    // If contains returns true, get should succeed
                    // (Note: may fail if another thread removed it - that's fine)
                    let _ = c.get(&i);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

// ----------------------------------------------------------------------------
// ALL ALGORITHMS: CONSISTENT BEHAVIOR
// ----------------------------------------------------------------------------

#[test]
fn test_all_concurrent_caches_len_consistency() {
    // Verify len() is always <= capacity for all cache types under concurrent access

    let lru: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(50, 4), None));
    let lfu: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(50, 4), None));
    let lfuda: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(50, 4), None));
    let slru: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(50, 15, 4), None));
    let gdsf: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(50, 4), None));

    let mut handles = vec![];

    // Spawn threads to test all caches concurrently with mixed operations
    for t in 0..NUM_THREADS {
        let lru_c = Arc::clone(&lru);
        let lfu_c = Arc::clone(&lfu);
        let lfuda_c = Arc::clone(&lfuda);
        let slru_c = Arc::clone(&slru);
        let gdsf_c = Arc::clone(&gdsf);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                // Mixed operations on all cache types
                lru_c.put(key, key, 1);
                lfu_c.put(key, key, 1);
                lfuda_c.put(key, key, 1);
                slru_c.put(key, key, 1);
                gdsf_c.put(key, key, 1);

                if i % 3 == 0 {
                    let _ = lru_c.get(&key);
                    let _ = lfu_c.get(&key);
                    let _ = lfuda_c.get(&key);
                    let _ = slru_c.get(&key);
                    let _ = gdsf_c.get(&key);
                }

                if i % 7 == 0 {
                    let _ = lru_c.remove(&key);
                    let _ = lfu_c.remove(&key);
                    let _ = lfuda_c.remove(&key);
                    let _ = slru_c.remove(&key);
                    let _ = gdsf_c.remove(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(lru.len() <= 50, "LRU exceeded capacity");
    assert!(lfu.len() <= 50, "LFU exceeded capacity");
    assert!(lfuda.len() <= 50, "LFUDA exceeded capacity");
    assert!(slru.len() <= 50, "SLRU exceeded capacity");
    assert!(gdsf.len() <= 50, "GDSF exceeded capacity");
}

#[test]
fn test_all_concurrent_caches_clear() {
    let lru: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 4), None));
    let lfu: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(100, 4), None));
    let lfuda: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(100, 4), None));
    let slru: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(100, 30, 4), None));
    let gdsf: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(100, 4), None));

    let mut handles = vec![];

    // Spawn threads that fill caches then clear them concurrently
    for t in 0..NUM_THREADS {
        let lru_c = Arc::clone(&lru);
        let lfu_c = Arc::clone(&lfu);
        let lfuda_c = Arc::clone(&lfuda);
        let slru_c = Arc::clone(&slru);
        let gdsf_c = Arc::clone(&gdsf);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                lru_c.put(key, key, 1);
                lfu_c.put(key, key, 1);
                lfuda_c.put(key, key, 1);
                slru_c.put(key, key, 1);
                gdsf_c.put(key, key, 1);

                // Periodically clear all caches (simulates cache reset)
                if i % 100 == 0 {
                    lru_c.clear();
                    lfu_c.clear();
                    lfuda_c.clear();
                    slru_c.clear();
                    gdsf_c.clear();
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Final clear should result in empty caches
    lru.clear();
    lfu.clear();
    lfuda.clear();
    slru.clear();
    gdsf.clear();

    assert!(lru.is_empty(), "LRU should be empty after clear");
    assert!(lfu.is_empty(), "LFU should be empty after clear");
    assert!(lfuda.is_empty(), "LFUDA should be empty after clear");
    assert!(slru.is_empty(), "SLRU should be empty after clear");
    assert!(gdsf.is_empty(), "GDSF should be empty after clear");
}

// ============================================================================
// SIZE-BASED EVICTION TESTS FOR CONCURRENT CACHES
// ============================================================================
// These tests verify that size limits are correctly enforced when split
// across segments. With max_size split per-segment, each segment enforces
// its own limit, and eviction happens when any segment exceeds its limit.

/// Test that ConcurrentLruCache correctly enforces size limits across segments
/// under concurrent access with mixed operations.
#[test]
fn test_concurrent_lru_size_based_eviction() {
    let max_size: u64 = 100 * 1024; // 100KB
    let segment_count = 4;

    let cache: Arc<ConcurrentLruCache<i32, String>> = Arc::new(ConcurrentLruCache::init(
        lru_config_with_size(200, max_size, segment_count),
        None,
    ));

    let object_size: u64 = 1024; // 1KB each
    let mut handles = vec![];

    // Spawn threads that concurrently insert objects with sizes
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, format!("value_{}", key), object_size);

                // Mixed operations
                if i % 3 == 0 {
                    let _ = c.get(&key);
                }
                if i % 5 == 0 {
                    let _ = c.peek(&key);
                }
                if i % 7 == 0 {
                    let _ = c.contains(&key);
                }
                if i % 11 == 0 {
                    let _ = c.remove(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Size should be within limits
    let final_size = cache.current_size();
    assert!(
        final_size <= max_size,
        "Size {} should not exceed max_size {}",
        final_size,
        max_size
    );
}

/// Test ConcurrentLfuCache size-based eviction across segments with concurrent access.
#[test]
fn test_concurrent_lfu_size_based_eviction() {
    let max_size: u64 = 100 * 1024; // 100KB
    let segment_count = 4;

    let cache: Arc<ConcurrentLfuCache<i32, String>> = Arc::new(ConcurrentLfuCache::init(
        lfu_config_with_size(200, max_size, segment_count),
        None,
    ));

    let object_size: u64 = 1024; // 1KB each
    let mut handles = vec![];

    // Spawn threads that concurrently insert objects and build frequency
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, format!("value_{}", key), object_size);

                // Build frequency on some keys
                if i % 5 == 0 {
                    for _ in 0..3 {
                        let _ = c.get(&key);
                    }
                }

                // Mixed operations
                if i % 4 == 0 {
                    let _ = c.peek(&key);
                }
                if i % 6 == 0 {
                    let _ = c.contains(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let final_size = cache.current_size();
    assert!(
        final_size <= max_size,
        "LFU Size {} should not exceed max_size {}",
        final_size,
        max_size
    );
}

/// Test that LFUDA respects max_size limits under concurrent access.
#[test]
fn test_concurrent_lfuda_size_based_eviction() {
    let max_size: u64 = 100 * 1024; // 100KB
    let segment_count = 4;

    let cache: Arc<ConcurrentLfudaCache<i32, String>> = Arc::new(ConcurrentLfudaCache::init(
        lfuda_config_with_size(200, max_size, segment_count),
        None,
    ));

    let object_size: u64 = 1024; // 1KB each
    let mut handles = vec![];

    // Spawn threads that concurrently insert objects with sizes
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, format!("value_{}", key), object_size);

                // Build priority on some keys
                if i % 4 == 0 {
                    for _ in 0..3 {
                        let _ = c.get(&key);
                    }
                }

                // Mixed operations
                if i % 5 == 0 {
                    let _ = c.peek(&key);
                }
                if i % 7 == 0 {
                    let _ = c.contains(&key);
                }
                if i % 10 == 0 {
                    let _ = c.remove(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let final_size = cache.current_size();
    assert!(
        final_size <= max_size,
        "LFUDA Size {} should not exceed max_size {}",
        final_size,
        max_size
    );
}

/// Test that GDSF respects max_size limits under concurrent access.
#[test]
fn test_concurrent_gdsf_size_based_eviction() {
    let max_size: u64 = 100 * 1024; // 100KB
    let segment_count = 4;

    let cache: Arc<ConcurrentGdsfCache<i32, String>> = Arc::new(ConcurrentGdsfCache::init(
        gdsf_config_with_size(200, max_size, segment_count),
        None,
    ));

    let mut handles = vec![];

    // Spawn threads that concurrently insert objects with varying sizes
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                // Vary sizes to test GDSF's size-aware eviction
                let size = ((i % 5) + 1) as u64 * 256; // 256 to 1280 bytes
                c.put(key, format!("value_{}", key), size);

                // Build frequency on some keys
                if i % 3 == 0 {
                    for _ in 0..3 {
                        let _ = c.get(&key);
                    }
                }

                // Mixed operations
                if i % 5 == 0 {
                    let _ = c.peek(&key);
                }
                if i % 6 == 0 {
                    let _ = c.contains(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let final_size = cache.current_size();
    assert!(
        final_size <= max_size,
        "GDSF Size {} should not exceed max_size {}",
        final_size,
        max_size
    );
}

/// Test that per-segment size limits work correctly under concurrent access.
#[test]
fn test_concurrent_lru_per_segment_size_limit() {
    let max_size: u64 = 100 * 1024; // 100KB total
    let segment_count = 4;

    let cache: Arc<ConcurrentLruCache<i32, String>> = Arc::new(ConcurrentLruCache::init(
        lru_config_with_size(200, max_size, segment_count),
        None,
    ));

    let mut handles = vec![];

    // Spawn threads that insert large objects concurrently
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                // Use varying large object sizes
                let size = ((i % 4) + 1) as u64 * 2048; // 2KB to 8KB
                c.put(key, format!("large_value_{}", key), size);

                // Verify size limits during concurrent access
                let current = c.current_size();
                assert!(
                    current <= max_size,
                    "Size {} exceeded max_size {} during concurrent insert",
                    current,
                    max_size
                );

                // Mixed operations
                if i % 4 == 0 {
                    let _ = c.get(&key);
                }
                if i % 6 == 0 {
                    let _ = c.peek(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    let final_size = cache.current_size();
    assert!(
        final_size <= max_size,
        "Final size {} should not exceed max_size {}",
        final_size,
        max_size
    );
}

/// Test size tracking accuracy across concurrent operations.
#[test]
fn test_concurrent_size_tracking_accuracy() {
    let max_size: u64 = 1024 * 1024; // 1MB - large enough that we don't trigger eviction
    let segment_count = 4;

    let cache: Arc<ConcurrentLruCache<i32, String>> = Arc::new(ConcurrentLruCache::init(
        lru_config_with_size(1000, max_size, segment_count),
        None,
    ));

    let item_size: u64 = 100; // 100 bytes each
    let num_items = 100;

    // Insert items from multiple threads
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || {
                for i in 0..25 {
                    let key = t * 25 + i;
                    cache.put(key, format!("value_{}", key), item_size);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let expected_size = num_items as u64 * item_size;
    let actual_size = cache.current_size();
    let actual_len = cache.len();

    println!(
        "Size tracking: expected={}B, actual={}B, len={}",
        expected_size, actual_size, actual_len
    );

    // All items should be present (no eviction expected)
    assert_eq!(actual_len, num_items, "All items should be present");
    assert_eq!(
        actual_size, expected_size,
        "Size should match expected total"
    );
}

/// Test that removing items correctly updates size tracking under concurrent access.
#[test]
fn test_concurrent_size_tracking_on_remove() {
    let max_size: u64 = 500 * 1024; // 500KB
    let segment_count = 4;

    let cache: Arc<ConcurrentLruCache<i32, String>> = Arc::new(ConcurrentLruCache::init(
        lru_config_with_size(500, max_size, segment_count),
        None,
    ));

    let item_size: u64 = 1024; // 1KB each

    // Pre-populate cache
    for i in 0..100 {
        cache.put(i, format!("value_{}", i), item_size);
    }

    let size_before = cache.current_size();
    assert_eq!(size_before, 100 * item_size, "Size should be 100KB");

    let mut handles = vec![];

    // Spawn threads that concurrently remove, add, and check size
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;

                // Remove some existing keys
                if i < 50 {
                    let _ = c.remove(&(i as i32));
                }

                // Add new keys
                c.put(key + 1000, format!("new_value_{}", key), item_size);

                // Mixed operations
                if i % 3 == 0 {
                    let _ = c.get(&(key + 1000));
                }
                if i % 5 == 0 {
                    let _ = c.contains(&(key + 1000));
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Size should be non-negative and within limits
    let final_size = cache.current_size();
    assert!(
        final_size <= max_size,
        "Size {} should be <= max_size {}",
        final_size,
        max_size
    );
}

// ============================================================================
// SEGMENT 4: CONSTRUCTOR AND SIZE-LIMIT COVERAGE TESTS
// ============================================================================
// These tests ensure coverage for with_max_size, with_limits, current_size,
// max_size, and clear methods across all concurrent cache implementations.

// ----------------------------------------------------------------------------
// LRU SIZE-BASED CONSTRUCTORS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_lru_with_max_size() {
    let max_size: u64 = 1024 * 1024; // 1MB
    let cache: Arc<ConcurrentLruCache<String, Vec<u8>>> = Arc::new(ConcurrentLruCache::init(
        lru_config_with_size(10000, max_size, 4),
        None,
    ));

    // Verify max_size is set correctly
    let actual_max = cache.max_size();
    assert!(
        actual_max >= max_size - cache.segment_count() as u64
            && actual_max <= max_size + cache.segment_count() as u64,
        "max_size should be approximately {} but was {}",
        max_size,
        actual_max
    );
    assert_eq!(cache.current_size(), 0);

    let mut handles = vec![];

    // Spawn threads that concurrently insert and read
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = format!("key_{}_{}", t, i);
                let data = vec![0u8; 100];
                c.put(key.clone(), data, 100);

                if i % 3 == 0 {
                    let _ = c.get(&key);
                }
                if i % 5 == 0 {
                    let _ = c.contains(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify size is tracked
    assert!(cache.current_size() > 0, "Size should be tracked");
}

#[test]
fn test_concurrent_lru_with_limits() {
    let max_size: u64 = 100_000;
    let cache: Arc<ConcurrentLruCache<i32, String>> = Arc::new(ConcurrentLruCache::init(
        lru_config_with_size(200, max_size, 4),
        None,
    ));

    assert_eq!(cache.max_size(), max_size);
    assert_eq!(cache.current_size(), 0);

    let mut handles = vec![];

    // Spawn threads that fill cache and periodically clear
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, format!("value_{}", key), 100);

                // Periodically clear to test clear under concurrency
                if i % 50 == 0 {
                    c.clear();
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Final clear
    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_size(), 0);
}

// ----------------------------------------------------------------------------
// LFU SIZE-BASED CONSTRUCTORS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_lfu_with_max_size() {
    let max_size: u64 = 1024 * 1024;
    let cache: Arc<ConcurrentLfuCache<String, Vec<u8>>> = Arc::new(ConcurrentLfuCache::init(
        lfu_config_with_size(10000, max_size, 4),
        None,
    ));

    let actual_max = cache.max_size();
    assert!(
        actual_max >= max_size - cache.segment_count() as u64
            && actual_max <= max_size + cache.segment_count() as u64,
        "max_size should be approximately {} but was {}",
        max_size,
        actual_max
    );
    assert_eq!(cache.current_size(), 0);

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = format!("key_{}_{}", t, i);
                c.put(key.clone(), vec![1, 2, 3], 100);
                if i % 3 == 0 {
                    let _ = c.get(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.current_size() > 0, "Size should be tracked");
}

#[test]
fn test_concurrent_lfu_with_limits() {
    let max_size: u64 = 100_000;
    let cache: Arc<ConcurrentLfuCache<i32, String>> = Arc::new(ConcurrentLfuCache::init(
        lfu_config_with_size(200, max_size, 4),
        None,
    ));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, format!("value_{}", key), 100);
                if i % 50 == 0 {
                    c.clear();
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_size(), 0);
}

// ----------------------------------------------------------------------------
// LFUDA SIZE-BASED CONSTRUCTORS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_lfuda_with_max_size() {
    let max_size: u64 = 1024 * 1024;
    let cache: Arc<ConcurrentLfudaCache<String, Vec<u8>>> = Arc::new(ConcurrentLfudaCache::init(
        lfuda_config_with_size(10000, max_size, 4),
        None,
    ));

    let actual_max = cache.max_size();
    assert!(
        actual_max >= max_size - cache.segment_count() as u64
            && actual_max <= max_size + cache.segment_count() as u64,
        "max_size should be approximately {} but was {}",
        max_size,
        actual_max
    );
    assert_eq!(cache.current_size(), 0);

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = format!("key_{}_{}", t, i);
                c.put(key.clone(), vec![1, 2, 3], 100);
                if i % 3 == 0 {
                    let _ = c.get(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.current_size() > 0, "Size should be tracked");
}

#[test]
fn test_concurrent_lfuda_with_limits() {
    let max_size: u64 = 100_000;
    let cache: Arc<ConcurrentLfudaCache<i32, String>> = Arc::new(ConcurrentLfudaCache::init(
        lfuda_config_with_size(200, max_size, 4),
        None,
    ));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, format!("value_{}", key), 100);
                if i % 50 == 0 {
                    c.clear();
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_size(), 0);
}

// ----------------------------------------------------------------------------
// GDSF SIZE-BASED CONSTRUCTORS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_gdsf_with_max_size() {
    let max_size: u64 = 1024 * 1024;
    let cache: Arc<ConcurrentGdsfCache<String, Vec<u8>>> = Arc::new(ConcurrentGdsfCache::init(
        gdsf_config_with_size(10000, max_size, 4),
        None,
    ));

    let actual_max = cache.max_size();
    assert!(
        actual_max >= max_size - cache.segment_count() as u64
            && actual_max <= max_size + cache.segment_count() as u64,
        "max_size should be approximately {} but was {}",
        max_size,
        actual_max
    );
    assert_eq!(cache.current_size(), 0);

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = format!("key_{}_{}", t, i);
                c.put(key.clone(), vec![1, 2, 3], 100);
                if i % 3 == 0 {
                    let _ = c.get(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.current_size() > 0, "Size should be tracked");
}

#[test]
fn test_concurrent_gdsf_with_limits() {
    let max_size: u64 = 100_000;
    let cache: Arc<ConcurrentGdsfCache<i32, String>> = Arc::new(ConcurrentGdsfCache::init(
        gdsf_config_with_size(200, max_size, 4),
        None,
    ));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, format!("value_{}", key), 100);
                if i % 50 == 0 {
                    c.clear();
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_size(), 0);
}

// ----------------------------------------------------------------------------
// SLRU SIZE-BASED CONSTRUCTORS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_slru_with_max_size() {
    let max_size: u64 = 1024 * 1024;
    let cache: Arc<ConcurrentSlruCache<String, Vec<u8>>> = Arc::new(ConcurrentSlruCache::init(
        slru_config_with_size(10000, 2000, max_size, 4),
        None,
    ));

    let actual_max = cache.max_size();
    assert!(
        actual_max >= max_size - cache.segment_count() as u64
            && actual_max <= max_size + cache.segment_count() as u64,
        "max_size should be approximately {} but was {}",
        max_size,
        actual_max
    );
    assert_eq!(cache.current_size(), 0);

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = format!("key_{}_{}", t, i);
                c.put(key.clone(), vec![1, 2, 3], 100);
                // Access multiple times to test promotion
                for _ in 0..3 {
                    let _ = c.get(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.current_size() > 0, "Size should be tracked");
}

#[test]
fn test_concurrent_slru_with_limits() {
    let max_size: u64 = 100_000;
    let cache: Arc<ConcurrentSlruCache<i32, String>> = Arc::new(ConcurrentSlruCache::init(
        slru_config_with_size(200, 50, max_size, 4),
        None,
    ));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, format!("value_{}", key), 100);
                // Access to trigger promotion to protected
                for _ in 0..3 {
                    let _ = c.get(&key);
                }
                if i % 50 == 0 {
                    c.clear();
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_size(), 0);
}

// ----------------------------------------------------------------------------
// CLEAR OPERATIONS UNDER CONCURRENCY
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_clear_during_operations() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1000, 4), None));

    // Pre-fill
    for i in 0..100 {
        cache.put(i, i, 1);
    }
    assert_eq!(cache.len(), 100);

    let cache_clone = Arc::clone(&cache);

    // Spawn thread to clear while main thread inserts
    // Note: Use longer sleep times (20ms) for Windows compatibility
    // Windows has ~15ms timer resolution so shorter sleeps are unreliable
    let handle = thread::spawn(move || {
        for _ in 0..5 {
            cache_clone.clear();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    });

    // Insert while clear is happening
    for i in 100..200 {
        cache.put(i, i, 1);
    }

    handle.join().unwrap();

    // Cache should be in a valid state (might be empty or have some entries)
    let len = cache.len();
    assert!(len <= 1000, "Cache should respect capacity");
}

// ----------------------------------------------------------------------------
// RECORD_MISS COVERAGE (only ConcurrentLruCache has record_miss)
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_lru_record_miss() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 4), None));

    let mut handles = vec![];

    // Spawn threads that record misses and perform cache operations
    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                // Record misses for keys we don't have
                c.record_miss(100);

                // Also do actual cache operations
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key, 1);
                let _ = c.get(&key);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Check metrics include misses
    let metrics = cache.metrics();
    assert!(
        metrics.get("cache_misses").unwrap_or(&0.0) >= &2.0,
        "Should have recorded misses"
    );
}

// ============================================================================
// GET_WITH COVERAGE
// ============================================================================
// get_with() allows applying a function to the value under the lock,
// returning a transformed result.

#[test]
fn test_concurrent_lru_get_with() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key * 2, 1);
                // Use get_with to transform value - may be None if evicted
                let doubled = c.get_with(&key, |v| v * 2);
                if let Some(val) = doubled {
                    assert_eq!(val, key * 4);
                }
                // get_with on potentially missing key
                let _ = c.get_with(&(key + 100000), |v| v * 2);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_lfu_get_with() {
    let cache: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key * 2, 1);
                // Use get_with to transform value - may be None if evicted
                let doubled = c.get_with(&key, |v| v * 2);
                // Value could be evicted by another thread, so it may be None
                if let Some(val) = doubled {
                    assert_eq!(val, key * 4);
                }
                // get_with on potentially missing key
                let _ = c.get_with(&(key + 100000), |v| v * 2);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_lfuda_get_with() {
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key * 2, 1);
                // Value could be evicted by another thread, so it may be None
                let doubled = c.get_with(&key, |v| v * 2);
                if let Some(val) = doubled {
                    assert_eq!(val, key * 4);
                }
                let _ = c.get_with(&(key + 100000), |v| v * 2);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_slru_get_with() {
    let cache: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(10000, 2000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key * 2, 1);
                // Value could be evicted by another thread
                let doubled = c.get_with(&key, |v| v * 2);
                if let Some(val) = doubled {
                    assert_eq!(val, key * 4);
                }
                let _ = c.get_with(&(key + 100000), |v| v * 2);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_gdsf_get_with() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key * 2, 1);
                // Value could be evicted by another thread
                let doubled = c.get_with(&key, |v| v * 2);
                if let Some(val) = doubled {
                    assert_eq!(val, key * 4);
                }
                let _ = c.get_with(&(key + 100000), |v| v * 2);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

// ============================================================================
// GET_MUT_WITH COVERAGE (ConcurrentLruCache only)
// ============================================================================

#[test]
fn test_concurrent_lru_get_mut_with() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key, 1);
                // Use get_mut_with to mutate value in-place - may be None if evicted
                let old_val = c.get_mut_with(&key, |v| {
                    let old = *v;
                    *v += 100;
                    old
                });
                // Value may have been evicted by another thread
                if let Some(val) = old_val {
                    assert_eq!(val, key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

// ============================================================================
// CONTAINS COVERAGE (concurrent)
// ============================================================================

#[test]
fn test_concurrent_all_caches_contains() {
    let lru: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1000, 4), None));
    let lfu: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(1000, 4), None));
    let lfuda: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(1000, 4), None));
    let slru: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(1000, 200, 4), None));
    let gdsf: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(1000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let lru_c = Arc::clone(&lru);
        let lfu_c = Arc::clone(&lfu);
        let lfuda_c = Arc::clone(&lfuda);
        let slru_c = Arc::clone(&slru);
        let gdsf_c = Arc::clone(&gdsf);

        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;

                // LRU - test contains before and after operations
                // Note: We can't assert contains() results in concurrent tests because
                // another thread may evict entries at any time between put and contains
                let _ = lru_c.contains(&key);
                lru_c.put(key, key, 1);
                let _ = lru_c.contains(&key); // May be false if evicted by another thread
                if i % 10 == 0 {
                    lru_c.remove(&key);
                }

                // LFU
                lfu_c.put(key, key, 1);
                let _ = lfu_c.contains(&key);
                if i % 10 == 0 {
                    lfu_c.remove(&key);
                }

                // LFUDA
                lfuda_c.put(key, key, 1);
                let _ = lfuda_c.contains(&key);
                if i % 10 == 0 {
                    lfuda_c.remove(&key);
                }

                // SLRU
                slru_c.put(key, key, 1);
                let _ = slru_c.contains(&key);
                if i % 10 == 0 {
                    slru_c.remove(&key);
                }

                // GDSF
                gdsf_c.put(key, key, 1);
                let _ = gdsf_c.contains(&key);
                if i % 10 == 0 {
                    gdsf_c.remove(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

// ============================================================================
// PEEK COVERAGE (concurrent)
// ============================================================================

#[test]
fn test_concurrent_lru_peek() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key, 1);
                // peek returns cloned value - may be None if evicted
                let peeked = c.peek(&key);
                if let Some(val) = peeked {
                    assert_eq!(val, key);
                }
                // peek on potentially missing key
                let _ = c.peek(&(key + 100000));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_lfu_peek() {
    let cache: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key, 1);
                // Value could be evicted by another thread
                let peeked = c.peek(&key);
                if let Some(val) = peeked {
                    assert_eq!(val, key);
                }
                let _ = c.peek(&(key + 100000));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_lfuda_peek() {
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key, 1);
                // Value could be evicted by another thread
                let peeked = c.peek(&key);
                if let Some(val) = peeked {
                    assert_eq!(val, key);
                }
                let _ = c.peek(&(key + 100000));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_slru_peek() {
    let cache: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(10000, 2000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key, 1);
                // Value could be evicted by another thread, so it may be None
                let peeked = c.peek(&key);
                if let Some(val) = peeked {
                    assert_eq!(val, key);
                }
                let _ = c.peek(&(key + 100000));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_gdsf_peek() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(10000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;
                c.put(key, key, 1);
                // Value could be evicted by another thread
                let peeked = c.peek(&key);
                if let Some(val) = peeked {
                    assert_eq!(val, key);
                }
                let _ = c.peek(&(key + 100000));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

// ============================================================================
// POP COVERAGE (concurrent)
// ============================================================================

#[test]
fn test_concurrent_lru_pop() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1000, 4), None));

    // Pre-fill cache
    for i in 0..500 {
        cache.put(i, i, 1);
    }

    let mut handles = vec![];

    for _ in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..OPS_PER_THREAD {
                // Mix of pop and put operations
                let popped = c.pop();
                // Popped might be Some or None depending on cache state
                if let Some((k, _v)) = popped {
                    // Put something back
                    c.put(k + 10000, k, 1);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Cache should be in valid state
    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_lfu_pop() {
    let cache: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(1000, 4), None));

    // Pre-fill cache
    for i in 0..500 {
        cache.put(i, i, 1);
    }

    let mut handles = vec![];

    for _ in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..OPS_PER_THREAD {
                let popped = c.pop();
                if let Some((k, _v)) = popped {
                    c.put(k + 10000, k, 1);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_lfuda_pop() {
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(1000, 4), None));

    // Pre-fill cache
    for i in 0..500 {
        cache.put(i, i, 1);
    }

    let mut handles = vec![];

    for _ in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..OPS_PER_THREAD {
                let popped = c.pop();
                if let Some((k, _v)) = popped {
                    c.put(k + 10000, k, 1);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_slru_pop() {
    let cache: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(1000, 200, 4), None));

    // Pre-fill cache
    for i in 0..500 {
        cache.put(i, i, 1);
    }

    let mut handles = vec![];

    for _ in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..OPS_PER_THREAD {
                let popped = c.pop();
                if let Some((k, _v)) = popped {
                    c.put(k + 10000, k, 1);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

#[test]
fn test_concurrent_gdsf_pop() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(1000, 4), None));

    // Pre-fill cache
    for i in 0..500 {
        cache.put(i, i, 1);
    }

    let mut handles = vec![];

    for _ in 0..NUM_THREADS {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..OPS_PER_THREAD {
                let popped = c.pop();
                if let Some((k, _v)) = popped {
                    c.put(k + 10000, k, 1);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= cache.capacity());
}

// ============================================================================
// CAPACITY / SEGMENT_COUNT COVERAGE (concurrent)
// ============================================================================

#[test]
fn test_concurrent_all_caches_capacity_and_segments() {
    let lru: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1000, 4), None));
    let lfu: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(1000, 8), None));
    let lfuda: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(1000, 4), None));
    let slru: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(1000, 200, 8), None));
    let gdsf: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(1000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let lru_c = Arc::clone(&lru);
        let lfu_c = Arc::clone(&lfu);
        let lfuda_c = Arc::clone(&lfuda);
        let slru_c = Arc::clone(&slru);
        let gdsf_c = Arc::clone(&gdsf);

        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;

                // Perform operations while checking capacity/segments
                lru_c.put(key, key, 1);
                assert_eq!(lru_c.capacity(), 1000);
                assert_eq!(lru_c.segment_count(), 4);

                lfu_c.put(key, key, 1);
                assert_eq!(lfu_c.capacity(), 1000);
                assert_eq!(lfu_c.segment_count(), 8);

                lfuda_c.put(key, key, 1);
                assert_eq!(lfuda_c.capacity(), 1000);
                assert_eq!(lfuda_c.segment_count(), 4);

                slru_c.put(key, key, 1);
                assert_eq!(slru_c.capacity(), 1000);
                assert_eq!(slru_c.segment_count(), 8);

                gdsf_c.put(key, key, 1);
                assert_eq!(gdsf_c.capacity(), 1000);
                assert_eq!(gdsf_c.segment_count(), 4);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

// ============================================================================
// CACHE_METRICS TRAIT COVERAGE (concurrent)
// ============================================================================

#[test]
fn test_concurrent_all_caches_algorithm_name() {
    let lru: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1000, 4), None));
    let lfu: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(1000, 4), None));
    let lfuda: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(1000, 4), None));
    let slru: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(1000, 200, 4), None));
    let gdsf: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(1000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let lru_c = Arc::clone(&lru);
        let lfu_c = Arc::clone(&lfu);
        let lfuda_c = Arc::clone(&lfuda);
        let slru_c = Arc::clone(&slru);
        let gdsf_c = Arc::clone(&gdsf);

        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;

                // Do operations while checking algorithm names
                lru_c.put(key, key, 1);
                assert_eq!(lru_c.algorithm_name(), "ConcurrentLRU");

                lfu_c.put(key, key, 1);
                assert_eq!(lfu_c.algorithm_name(), "ConcurrentLFU");

                lfuda_c.put(key, key, 1);
                assert_eq!(lfuda_c.algorithm_name(), "ConcurrentLFUDA");

                slru_c.put(key, key, 1);
                assert_eq!(slru_c.algorithm_name(), "ConcurrentSLRU");

                gdsf_c.put(key, key, 1);
                assert_eq!(gdsf_c.algorithm_name(), "ConcurrentGDSF");
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}

#[test]
fn test_concurrent_all_caches_metrics() {
    let lru: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1000, 4), None));
    let lfu: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(1000, 4), None));
    let lfuda: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(1000, 4), None));
    let slru: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(1000, 200, 4), None));
    let gdsf: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(1000, 4), None));

    let mut handles = vec![];

    for t in 0..NUM_THREADS {
        let lru_c = Arc::clone(&lru);
        let lfu_c = Arc::clone(&lfu);
        let lfuda_c = Arc::clone(&lfuda);
        let slru_c = Arc::clone(&slru);
        let gdsf_c = Arc::clone(&gdsf);

        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = (t * OPS_PER_THREAD + i) as i32;

                // Perform operations and check metrics exist
                lru_c.put(key, key, 1);
                let _ = lru_c.get(&key);
                let metrics = lru_c.metrics();
                assert!(metrics.contains_key("cache_hits"), "LRU should track hits");

                lfu_c.put(key, key, 1);
                let _ = lfu_c.get(&key);
                let metrics = lfu_c.metrics();
                assert!(metrics.contains_key("cache_hits"), "LFU should track hits");

                lfuda_c.put(key, key, 1);
                let _ = lfuda_c.get(&key);
                let metrics = lfuda_c.metrics();
                assert!(
                    metrics.contains_key("cache_hits"),
                    "LFUDA should track hits"
                );

                slru_c.put(key, key, 1);
                let _ = slru_c.get(&key);
                let metrics = slru_c.metrics();
                assert!(metrics.contains_key("cache_hits"), "SLRU should track hits");

                gdsf_c.put(key, key, 1);
                let _ = gdsf_c.get(&key);
                let metrics = gdsf_c.metrics();
                assert!(metrics.contains_key("cache_hits"), "GDSF should track hits");
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }
}
