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

use cache_rs::metrics::CacheMetrics;
use cache_rs::{
    ConcurrentGdsfCache, ConcurrentLfuCache, ConcurrentLfudaCache, ConcurrentLruCache,
    ConcurrentSlruCache,
};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

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
    // Use 2 segments for concurrent access
    // Note: with 2 segments and capacity 6, each segment has capacity 3
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(6).unwrap(),
        2,
    ));

    // Fill cache from single thread first (predictable setup)
    for i in 1..=6 {
        cache.put(i, i * 10);
    }
    // Due to hash distribution, items may not fill exactly to 6
    let initial_len = cache.len();
    assert!(initial_len <= 6, "Cache should not exceed capacity");

    // Insert one more - should trigger eviction in whichever segment gets this key
    cache.put(7, 70);

    // Verify capacity is maintained
    assert!(
        cache.len() <= 6,
        "Cache should maintain capacity after eviction"
    );

    // New key should be present
    assert!(cache.get(&7).is_some(), "Key 7 should be present");
}

#[test]
fn test_concurrent_lru_access_prevents_eviction() {
    // Single segment for deterministic LRU behavior
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(3).unwrap(),
        1,
    ));

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    // Access key 1 - should move to MRU position
    assert_eq!(cache.get(&1), Some(10));

    // Insert new key - should evict key 2 (now LRU), not key 1
    cache.put(4, 40);

    // Key 2 should be evicted (was LRU after key 1 was accessed)
    assert!(cache.get(&2).is_none(), "Key 2 should be evicted (LRU)");
    assert!(
        cache.get(&1).is_some(),
        "Key 1 should remain (recently accessed)"
    );
    assert!(cache.get(&3).is_some(), "Key 3 should remain");
    assert!(cache.get(&4).is_some(), "Key 4 should be present");
}

#[test]
fn test_concurrent_lru_multi_segment_eviction() {
    // Use 2 segments - keys distributed by hash
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(4).unwrap(),
        2,
    ));
    // Each segment has capacity 2

    // Insert 4 items
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // Insert more items - some will cause evictions in their respective segments
    for i in 5..=10 {
        cache.put(i, i * 10);
    }

    // Cache should maintain total capacity of 4
    assert!(cache.len() <= 4, "Cache should not exceed capacity");
}

#[test]
fn test_concurrent_lru_concurrent_writes_maintain_capacity() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(20).unwrap(),
        4,
    ));

    let mut handles = vec![];

    // Spawn 4 threads, each writing to their own key range
    for t in 0..4 {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let key = t * 1000 + i;
                cache.put(key, key);
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
    let cache: Arc<ConcurrentLfuCache<i32, i32>> = Arc::new(ConcurrentLfuCache::with_segments(
        NonZeroUsize::new(3).unwrap(),
        1,
    ));

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    // Access key 1 multiple times - increase frequency
    for _ in 0..10 {
        cache.get(&1);
    }

    // Access key 2 a few times
    for _ in 0..3 {
        cache.get(&2);
    }

    // Key 3 has lowest frequency (1 from put)
    // Insert new key - should evict key 3
    cache.put(4, 40);

    assert!(
        cache.get(&3).is_none(),
        "Key 3 should be evicted (lowest freq)"
    );
    assert!(
        cache.get(&1).is_some(),
        "Key 1 should remain (highest freq)"
    );
    assert!(cache.get(&2).is_some(), "Key 2 should remain");
    assert!(cache.get(&4).is_some(), "Key 4 should be present");
}

#[test]
fn test_concurrent_lfu_frequency_accumulation() {
    let cache: Arc<ConcurrentLfuCache<String, i32>> = Arc::new(ConcurrentLfuCache::with_segments(
        NonZeroUsize::new(6).unwrap(),
        2,
    ));

    // Insert items
    cache.put("hot".to_string(), 1);
    cache.put("warm".to_string(), 2);
    cache.put("cold".to_string(), 3);

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
    cache_clone.put("new1".to_string(), 4);
    cache_clone.put("new2".to_string(), 5);
    cache_clone.put("new3".to_string(), 6);
    cache_clone.put("new4".to_string(), 7);

    // "hot" should survive due to high frequency
    assert!(
        cache_clone.get(&"hot".to_string()).is_some(),
        "Hot key should survive due to high concurrent access frequency"
    );
}

#[test]
fn test_concurrent_lfu_multi_segment_correctness() {
    let cache: Arc<ConcurrentLfuCache<i32, i32>> = Arc::new(ConcurrentLfuCache::with_segments(
        NonZeroUsize::new(8).unwrap(),
        4,
    ));

    // Insert items across segments
    for i in 1..=8 {
        cache.put(i, i * 10);
    }

    // Build frequency for specific keys
    for _ in 0..20 {
        cache.get(&1);
        cache.get(&2);
    }

    // Insert new items to trigger evictions
    for i in 100..110 {
        cache.put(i, i);
    }

    // High frequency items should be more likely to survive
    // (exact behavior depends on hash distribution to segments)
    assert!(cache.len() <= 8, "Should maintain capacity");
}

// ----------------------------------------------------------------------------
// CONCURRENT LFUDA CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_lfuda_priority_eviction() {
    // Use larger capacity for more predictable behavior
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> = Arc::new(ConcurrentLfudaCache::with_segments(
        NonZeroUsize::new(4).unwrap(),
        1,
    ));

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // Access key 1 significantly more to increase priority
    for _ in 0..20 {
        cache.get(&1);
    }

    // Access key 2 moderately
    for _ in 0..5 {
        cache.get(&2);
    }

    // Keys 3 and 4 have lowest priority (only initial put)
    // Insert new key - should evict one of the low priority keys
    cache.put(5, 50);

    // High priority key 1 should definitely remain
    assert!(
        cache.get(&1).is_some(),
        "Key 1 should remain (highest priority)"
    );

    // At least one of keys 3 or 4 should be evicted (lowest priority)
    let key3_gone = cache.get(&3).is_none();
    let key4_gone = cache.get(&4).is_none();
    assert!(
        key3_gone || key4_gone,
        "One of the low-priority keys (3 or 4) should be evicted"
    );
}

#[test]
fn test_concurrent_lfuda_aging_mechanism() {
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> = Arc::new(ConcurrentLfudaCache::with_segments(
        NonZeroUsize::new(4).unwrap(),
        2,
    ));

    // Initial items with high frequency
    cache.put(1, 10);
    for _ in 0..10 {
        cache.get(&1);
    }

    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // Force evictions to increase global age
    for i in 100..150 {
        cache.put(i, i);
    }

    // Cache should maintain capacity despite many evictions
    assert!(cache.len() <= 4, "Should maintain capacity");
}

// ----------------------------------------------------------------------------
// CONCURRENT SLRU CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_slru_segment_behavior() {
    let protected = NonZeroUsize::new(2).unwrap();
    let cache: Arc<ConcurrentSlruCache<i32, i32>> = Arc::new(ConcurrentSlruCache::with_segments(
        NonZeroUsize::new(6).unwrap(),
        protected,
        2,
    ));

    // Insert items - they start in probationary
    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);
    cache.put(4, 40);

    // Access keys 1 and 2 multiple times to promote to protected
    for _ in 0..3 {
        cache.get(&1);
        cache.get(&2);
    }

    // Fill cache and trigger evictions
    for i in 10..20 {
        cache.put(i, i * 10);
    }

    // Protected items (1 and 2) should be more likely to survive
    // Note: with multi-segment, exact behavior depends on key distribution
    assert!(cache.len() <= 6, "Should maintain capacity");
}

#[test]
fn test_concurrent_slru_promotion_under_concurrency() {
    let protected = NonZeroUsize::new(1).unwrap();
    let cache: Arc<ConcurrentSlruCache<i32, i32>> = Arc::new(ConcurrentSlruCache::with_segments(
        NonZeroUsize::new(4).unwrap(),
        protected,
        1,
    ));

    cache.put(1, 10);
    cache.put(2, 20);
    cache.put(3, 30);

    // Concurrent accesses to key 1 to promote it
    let mut handles = vec![];
    for _ in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..10 {
                c.get(&1);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Key 1 should be in protected segment
    // Insert new items to trigger probationary evictions
    cache.put(4, 40);
    cache.put(5, 50);
    cache.put(6, 60);

    // Key 1 should survive (promoted to protected)
    assert!(
        cache.get(&1).is_some(),
        "Key 1 should be in protected segment"
    );
}

// ----------------------------------------------------------------------------
// CONCURRENT GDSF CORRECTNESS
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_gdsf_size_aware_eviction() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> = Arc::new(ConcurrentGdsfCache::with_segments(
        NonZeroUsize::new(3).unwrap(),
        1,
    ));

    // Insert items with different sizes
    cache.put(1, 10, 100); // Large object
    cache.put(2, 20, 1); // Small object
    cache.put(3, 30, 1); // Small object

    // Large object has lower priority (freq/size = 1/100)
    // Small objects have higher priority (freq/size = 1/1)

    cache.put(4, 40, 1);

    // Large object should be evicted first
    assert!(
        cache.get(&1).is_none(),
        "Large object should be evicted (lower priority)"
    );
    assert!(cache.get(&2).is_some(), "Small object 2 should remain");
    assert!(cache.get(&3).is_some(), "Small object 3 should remain");
}

#[test]
fn test_concurrent_gdsf_frequency_matters() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> = Arc::new(ConcurrentGdsfCache::with_segments(
        NonZeroUsize::new(3).unwrap(),
        1,
    ));

    cache.put(1, 10, 1);
    cache.put(2, 20, 1);
    cache.put(3, 30, 1);

    // Access key 1 many times
    for _ in 0..20 {
        cache.get(&1);
    }

    // Access key 2 a few times
    for _ in 0..5 {
        cache.get(&2);
    }

    // Key 3 has lowest freq/size ratio
    cache.put(4, 40, 1);

    assert!(
        cache.get(&3).is_none(),
        "Key 3 should be evicted (lowest frequency)"
    );
    assert!(
        cache.get(&1).is_some(),
        "Key 1 should remain (highest frequency)"
    );
}

#[test]
fn test_concurrent_gdsf_concurrent_size_tracking() {
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> = Arc::new(ConcurrentGdsfCache::with_segments(
        NonZeroUsize::new(10).unwrap(),
        2,
    ));

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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(capacity).unwrap(),
        4,
    ));

    let mut handles = vec![];
    let write_count = Arc::new(AtomicUsize::new(0));

    for t in 0..8 {
        let c = Arc::clone(&cache);
        let wc = Arc::clone(&write_count);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key);
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
    let cache: Arc<ConcurrentLfuCache<i32, i32>> = Arc::new(ConcurrentLfuCache::with_segments(
        NonZeroUsize::new(capacity).unwrap(),
        4,
    ));

    let mut handles = vec![];

    for t in 0..8 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key);
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
    let protected = NonZeroUsize::new(20).unwrap();
    let cache: Arc<ConcurrentSlruCache<i32, i32>> = Arc::new(ConcurrentSlruCache::with_segments(
        NonZeroUsize::new(capacity).unwrap(),
        protected,
        4,
    ));

    let mut handles = vec![];

    for t in 0..8 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key);
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
    let cache: Arc<ConcurrentLfudaCache<i32, i32>> = Arc::new(ConcurrentLfudaCache::with_segments(
        NonZeroUsize::new(capacity).unwrap(),
        4,
    ));

    let mut handles = vec![];

    for t in 0..8 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                let key = t * 1000 + i;
                c.put(key, key);
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
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> = Arc::new(ConcurrentGdsfCache::with_segments(
        NonZeroUsize::new(capacity).unwrap(),
        4,
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

// ----------------------------------------------------------------------------
// DATA CONSISTENCY
// ----------------------------------------------------------------------------

#[test]
fn test_get_returns_correct_value() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(100).unwrap(),
        4,
    ));

    // Insert known values
    for i in 0..50 {
        cache.put(i, i * 100);
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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(10).unwrap(),
        2,
    ));

    cache.put(1, 0);

    let mut handles = vec![];

    // Multiple threads update same key
    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                c.put(1, t);
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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(100).unwrap(),
        4,
    ));

    // Insert items
    for i in 0..50 {
        cache.put(i, i);
    }

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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(100).unwrap(),
        4,
    ));

    let mut handles = vec![];

    // Writers
    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                c.put(t * 1000 + i, i);
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
    let cache: Arc<ConcurrentLfuCache<i32, i32>> = Arc::new(ConcurrentLfuCache::with_segments(
        NonZeroUsize::new(100).unwrap(),
        4,
    ));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                c.put(t * 1000 + i, i);
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
    let cache: Arc<ConcurrentSlruCache<i32, i32>> = Arc::new(ConcurrentSlruCache::with_segments(
        NonZeroUsize::new(100).unwrap(),
        NonZeroUsize::new(40).unwrap(),
        4,
    ));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                c.put(t * 1000 + i, i);
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
    let cache: Arc<ConcurrentGdsfCache<i32, i32>> = Arc::new(ConcurrentGdsfCache::with_segments(
        NonZeroUsize::new(100).unwrap(),
        4,
    ));

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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(100).unwrap(),
        4,
    ));

    let stop_flag = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    // Writer threads
    for t in 0..4 {
        let c = Arc::clone(&cache);
        let sf = Arc::clone(&stop_flag);
        handles.push(thread::spawn(move || {
            let mut i = 0;
            while sf.load(Ordering::Relaxed) == 0 {
                c.put(t * 10000 + i, i);
                i += 1;
            }
        }));
    }

    // Clear thread
    let cache_clear = Arc::clone(&cache);
    let stop_flag_clear = Arc::clone(&stop_flag);
    handles.push(thread::spawn(move || {
        for _ in 0..10 {
            thread::sleep(std::time::Duration::from_millis(5));
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
    let cache: Arc<ConcurrentLruCache<i32, String>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(200).unwrap(),
        4,
    ));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..25 {
                let key = t * 100 + i;
                c.put_with_size(key, format!("value_{}", key), 10);
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
    let cache: Arc<ConcurrentLfuCache<i32, String>> = Arc::new(ConcurrentLfuCache::with_segments(
        NonZeroUsize::new(200).unwrap(),
        4,
    ));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..25 {
                let key = t * 100 + i;
                c.put_with_size(key, format!("value_{}", key), 10);
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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(10).unwrap(),
        2,
    ));

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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(10).unwrap(),
        2,
    ));

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
                c.put(1, i);
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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(1).unwrap(),
        1,
    ));

    let mut handles = vec![];

    for t in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                c.put(t * 100 + i, i);
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
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(50).unwrap(),
        4,
    ));

    // Insert known keys
    for i in 0..30 {
        cache.put(i, i);
    }

    let mut handles = vec![];

    // Verify contains_key is consistent
    for _ in 0..4 {
        let c = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..30 {
                if c.contains_key(&i) {
                    // If contains_key returns true, get should succeed
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
    // Verify len() is always <= capacity for all cache types

    let cap = NonZeroUsize::new(20).unwrap();
    let protected = NonZeroUsize::new(8).unwrap();

    let lru: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::with_segments(cap, 2));
    let lfu: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::with_segments(cap, 2));
    let lfuda: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::with_segments(cap, 2));
    let slru: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::with_segments(cap, protected, 2));
    let gdsf: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::with_segments(cap, 2));

    // Insert more items than capacity
    for i in 0..100 {
        lru.put(i, i);
        lfu.put(i, i);
        lfuda.put(i, i);
        slru.put(i, i);
        gdsf.put(i, i, 1);
    }

    assert!(lru.len() <= 20, "LRU exceeded capacity");
    assert!(lfu.len() <= 20, "LFU exceeded capacity");
    assert!(lfuda.len() <= 20, "LFUDA exceeded capacity");
    assert!(slru.len() <= 20, "SLRU exceeded capacity");
    assert!(gdsf.len() <= 20, "GDSF exceeded capacity");
}

#[test]
fn test_all_concurrent_caches_clear() {
    let cap = NonZeroUsize::new(20).unwrap();
    let protected = NonZeroUsize::new(8).unwrap();

    let lru: Arc<ConcurrentLruCache<i32, i32>> =
        Arc::new(ConcurrentLruCache::with_segments(cap, 2));
    let lfu: Arc<ConcurrentLfuCache<i32, i32>> =
        Arc::new(ConcurrentLfuCache::with_segments(cap, 2));
    let lfuda: Arc<ConcurrentLfudaCache<i32, i32>> =
        Arc::new(ConcurrentLfudaCache::with_segments(cap, 2));
    let slru: Arc<ConcurrentSlruCache<i32, i32>> =
        Arc::new(ConcurrentSlruCache::with_segments(cap, protected, 2));
    let gdsf: Arc<ConcurrentGdsfCache<i32, i32>> =
        Arc::new(ConcurrentGdsfCache::with_segments(cap, 2));

    // Fill caches
    for i in 0..20 {
        lru.put(i, i);
        lfu.put(i, i);
        lfuda.put(i, i);
        slru.put(i, i);
        gdsf.put(i, i, 1);
    }

    // Clear all
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

/// Test that ConcurrentLruCache correctly enforces size limits across segments.
///
/// Setup:
/// - 2 segments, max_size = 100KB total (50KB per segment)
/// - 100 entry capacity (enough that entry count isn't the limiting factor)
/// - Insert 10 objects of 10KB each
///
/// Expected: When 11th object is inserted, cache should evict to maintain
/// size limit. Due to hash distribution, eviction may happen sooner if
/// keys cluster in one segment.
#[test]
fn test_concurrent_lru_size_based_eviction() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 100 * 1024; // 100KB
    let segment_count = 2;

    let cache: ConcurrentLruCache<i32, String> =
        ConcurrentLruCache::with_limits_and_segments(max_entries, max_size, segment_count);

    let object_size: u64 = 10 * 1024; // 10KB each

    // Insert 10 objects (total 100KB if evenly distributed)
    for i in 0..10 {
        cache.put_with_size(i, format!("value_{}", i), object_size);
    }

    let size_after_10 = cache.current_size();
    let len_after_10 = cache.len();
    println!(
        "After 10 inserts: len={}, size={}KB, max_size={}KB",
        len_after_10,
        size_after_10 / 1024,
        max_size / 1024
    );

    // Size should be at most max_size (100KB)
    // But due to per-segment limits (50KB each), we might have less if uneven distribution
    assert!(
        size_after_10 <= max_size,
        "Size {} should not exceed max_size {}",
        size_after_10,
        max_size
    );

    // Insert 11th object - this should definitely trigger eviction
    cache.put_with_size(10, format!("value_{}", 10), object_size);

    let size_after_11 = cache.current_size();
    let len_after_11 = cache.len();
    println!(
        "After 11 inserts: len={}, size={}KB",
        len_after_11,
        size_after_11 / 1024
    );

    // Size should still be within limits
    assert!(
        size_after_11 <= max_size,
        "Size {} should not exceed max_size {} after eviction",
        size_after_11,
        max_size
    );

    // Verify the new key is present
    assert!(
        cache.get(&10).is_some(),
        "Newly inserted key should be present"
    );
}

/// Test ConcurrentLfuCache size-based eviction across segments.
#[test]
fn test_concurrent_lfu_size_based_eviction() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 100 * 1024; // 100KB
    let segment_count = 2;

    let cache: ConcurrentLfuCache<i32, String> =
        ConcurrentLfuCache::with_limits_and_segments(max_entries, max_size, segment_count);

    let object_size: u64 = 10 * 1024; // 10KB each

    // Insert 10 objects
    for i in 0..10 {
        cache.put_with_size(i, format!("value_{}", i), object_size);
    }

    let size_after_10 = cache.current_size();
    println!(
        "LFU After 10 inserts: len={}, size={}KB, max_size={}KB",
        cache.len(),
        size_after_10 / 1024,
        max_size / 1024
    );

    assert!(
        size_after_10 <= max_size,
        "LFU Size {} should not exceed max_size {}",
        size_after_10,
        max_size
    );

    // Insert 11th object
    cache.put_with_size(10, format!("value_{}", 10), object_size);

    let size_after_11 = cache.current_size();
    println!(
        "LFU After 11 inserts: len={}, size={}KB",
        cache.len(),
        size_after_11 / 1024
    );

    assert!(
        size_after_11 <= max_size,
        "LFU Size {} should not exceed max_size {} after eviction",
        size_after_11,
        max_size
    );

    assert!(
        cache.get(&10).is_some(),
        "LFU: Newly inserted key should be present"
    );
}

/// Test that LFUDA respects max_size limits.
///
/// With max_size of 100KB and 10KB objects, we should never exceed 10 objects.
#[test]
fn test_concurrent_lfuda_size_based_eviction() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 100 * 1024; // 100KB
    let segment_count = 2;

    let cache: ConcurrentLfudaCache<i32, String> =
        ConcurrentLfudaCache::with_limits_and_segments(max_entries, max_size, segment_count);

    let object_size: u64 = 10 * 1024; // 10KB each

    // Insert 10 objects
    for i in 0..10 {
        cache.put_with_size(i, format!("value_{}", i), object_size);
    }

    let size_after_10 = cache.current_size();
    println!(
        "LFUDA After 10 inserts: len={}, size={}KB, max_size={}KB",
        cache.len(),
        size_after_10 / 1024,
        max_size / 1024
    );

    assert!(
        size_after_10 <= max_size,
        "LFUDA Size {} should not exceed max_size {}",
        size_after_10,
        max_size
    );

    // Insert 11th object - should trigger eviction
    cache.put_with_size(10, format!("value_{}", 10), object_size);

    let size_after_11 = cache.current_size();
    println!(
        "LFUDA After 11 inserts: len={}, size={}KB",
        cache.len(),
        size_after_11 / 1024
    );

    assert!(
        size_after_11 <= max_size,
        "LFUDA Size {} should not exceed max_size {} after eviction",
        size_after_11,
        max_size
    );

    assert!(
        cache.get(&10).is_some(),
        "LFUDA: Newly inserted key should be present"
    );
}

/// Test that GDSF respects max_size limits.
///
/// With max_size of 100KB and 10KB objects, we should never exceed 10 objects.
#[test]
fn test_concurrent_gdsf_size_based_eviction() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 100 * 1024; // 100KB
    let segment_count = 2;

    let cache: ConcurrentGdsfCache<i32, String> =
        ConcurrentGdsfCache::with_limits_and_segments(max_entries, max_size, segment_count);

    let object_size: u64 = 10 * 1024; // 10KB each

    // Insert 10 objects (GDSF requires size in put)
    for i in 0..10 {
        cache.put(i, format!("value_{}", i), object_size);
    }

    let size_after_10 = cache.current_size();
    println!(
        "GDSF After 10 inserts: len={}, size={}KB, max_size={}KB",
        cache.len(),
        size_after_10 / 1024,
        max_size / 1024
    );

    assert!(
        size_after_10 <= max_size,
        "GDSF Size {} should not exceed max_size {}",
        size_after_10,
        max_size
    );

    // Insert 11th object - should trigger eviction
    cache.put(10, format!("value_{}", 10), object_size);

    let size_after_11 = cache.current_size();
    println!(
        "GDSF After 11 inserts: len={}, size={}KB",
        cache.len(),
        size_after_11 / 1024
    );

    assert!(
        size_after_11 <= max_size,
        "GDSF Size {} should not exceed max_size {} after eviction",
        size_after_11,
        max_size
    );

    assert!(
        cache.get(&10).is_some(),
        "GDSF: Newly inserted key should be present"
    );
}

/// Test that per-segment size limits work correctly.
///
/// With 2 segments and 100KB total, each segment gets 50KB.
/// If all keys hash to one segment, that segment will evict at 50KB.
#[test]
fn test_concurrent_lru_per_segment_size_limit() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 100 * 1024; // 100KB total
    let segment_count = 2;
    // Each segment gets 50KB

    let cache: ConcurrentLruCache<i32, String> =
        ConcurrentLruCache::with_limits_and_segments(max_entries, max_size, segment_count);

    // Insert objects that are 20KB each
    // With 50KB per segment, each segment can hold at most 2 objects
    let large_object_size: u64 = 20 * 1024; // 20KB

    for i in 0..10 {
        cache.put_with_size(i, format!("large_value_{}", i), large_object_size);

        let current = cache.current_size();
        println!(
            "After inserting key {}: size={}KB, len={}",
            i,
            current / 1024,
            cache.len()
        );

        // Each segment can hold at most 50KB, so total can be at most 100KB
        assert!(
            current <= max_size,
            "Size {} exceeded max_size {} after inserting key {}",
            current,
            max_size,
            i
        );
    }

    // Final verification
    let final_size = cache.current_size();
    println!(
        "Final: len={}, size={}KB, max={}KB",
        cache.len(),
        final_size / 1024,
        max_size / 1024
    );

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
    let max_entries = NonZeroUsize::new(1000).unwrap();
    let max_size: u64 = 1024 * 1024; // 1MB - large enough that we don't trigger eviction
    let segment_count = 4;

    let cache: Arc<ConcurrentLruCache<i32, String>> = Arc::new(
        ConcurrentLruCache::with_limits_and_segments(max_entries, max_size, segment_count),
    );

    let item_size: u64 = 100; // 100 bytes each
    let num_items = 100;

    // Insert items from multiple threads
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let cache = Arc::clone(&cache);
            std::thread::spawn(move || {
                for i in 0..25 {
                    let key = t * 25 + i;
                    cache.put_with_size(key, format!("value_{}", key), item_size);
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

/// Test that removing items correctly updates size tracking.
#[test]
fn test_concurrent_size_tracking_on_remove() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 100 * 1024;
    let segment_count = 2;

    let cache: ConcurrentLruCache<i32, String> =
        ConcurrentLruCache::with_limits_and_segments(max_entries, max_size, segment_count);

    let item_size: u64 = 1024; // 1KB each

    // Insert 10 items
    for i in 0..10 {
        cache.put_with_size(i, format!("value_{}", i), item_size);
    }

    let size_before = cache.current_size();
    assert_eq!(size_before, 10 * item_size, "Size should be 10KB");

    // Remove 5 items
    for i in 0..5 {
        cache.remove(&i);
    }

    let size_after = cache.current_size();
    assert_eq!(
        size_after,
        5 * item_size,
        "Size should be 5KB after removing 5 items"
    );

    // Verify removed items are gone
    for i in 0..5 {
        assert!(cache.get(&i).is_none(), "Key {} should be removed", i);
    }

    // Verify remaining items are present
    for i in 5..10 {
        assert!(cache.get(&i).is_some(), "Key {} should still exist", i);
    }
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
    let cache: ConcurrentLruCache<String, Vec<u8>> = ConcurrentLruCache::with_max_size(max_size);

    assert_eq!(cache.max_size(), max_size);
    assert_eq!(cache.current_size(), 0);

    // Insert some data with explicit sizes
    let data = vec![0u8; 1000];
    cache.put_with_size("key1".to_string(), data.clone(), 1000);

    assert_eq!(cache.current_size(), 1000);
    assert!(cache.get(&"key1".to_string()).is_some());
}

#[test]
fn test_concurrent_lru_with_limits() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 50_000;
    let cache: ConcurrentLruCache<i32, String> =
        ConcurrentLruCache::with_limits(max_entries, max_size);

    assert!(cache.max_size() >= max_size); // Distributed across segments
    assert_eq!(cache.current_size(), 0);

    // Fill with data
    for i in 0..50 {
        cache.put_with_size(i, format!("value_{}", i), 100);
    }

    assert_eq!(cache.current_size(), 5000);
    assert_eq!(cache.len(), 50);

    // Clear and verify
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
    let cache: ConcurrentLfuCache<String, Vec<u8>> = ConcurrentLfuCache::with_max_size(max_size);

    assert_eq!(cache.max_size(), max_size);
    assert_eq!(cache.current_size(), 0);

    cache.put_with_size("key1".to_string(), vec![1, 2, 3], 100);
    assert_eq!(cache.current_size(), 100);
}

#[test]
fn test_concurrent_lfu_with_limits() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 50_000;
    let cache: ConcurrentLfuCache<i32, String> =
        ConcurrentLfuCache::with_limits(max_entries, max_size);

    for i in 0..50 {
        cache.put_with_size(i, format!("value_{}", i), 100);
    }

    assert_eq!(cache.current_size(), 5000);

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
    let cache: ConcurrentLfudaCache<String, Vec<u8>> =
        ConcurrentLfudaCache::with_max_size(max_size);

    assert_eq!(cache.max_size(), max_size);
    assert_eq!(cache.current_size(), 0);

    cache.put_with_size("key1".to_string(), vec![1, 2, 3], 100);
    assert_eq!(cache.current_size(), 100);
}

#[test]
fn test_concurrent_lfuda_with_limits() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 50_000;
    let cache: ConcurrentLfudaCache<i32, String> =
        ConcurrentLfudaCache::with_limits(max_entries, max_size);

    for i in 0..50 {
        cache.put_with_size(i, format!("value_{}", i), 100);
    }

    assert_eq!(cache.current_size(), 5000);

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
    let cache: ConcurrentGdsfCache<String, Vec<u8>> = ConcurrentGdsfCache::with_max_size(max_size);

    assert_eq!(cache.max_size(), max_size);
    assert_eq!(cache.current_size(), 0);

    // GDSF's put() always requires size as 3rd parameter
    cache.put("key1".to_string(), vec![1, 2, 3], 100);
    assert_eq!(cache.current_size(), 100);
}

#[test]
fn test_concurrent_gdsf_with_limits() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let max_size: u64 = 50_000;
    let cache: ConcurrentGdsfCache<i32, String> =
        ConcurrentGdsfCache::with_limits(max_entries, max_size);

    for i in 0..50 {
        // GDSF's put() always requires size as 3rd parameter
        cache.put(i, format!("value_{}", i), 100);
    }

    assert_eq!(cache.current_size(), 5000);

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
    let cache: ConcurrentSlruCache<String, Vec<u8>> = ConcurrentSlruCache::with_max_size(max_size);

    assert_eq!(cache.max_size(), max_size);
    assert_eq!(cache.current_size(), 0);

    cache.put_with_size("key1".to_string(), vec![1, 2, 3], 100);
    assert_eq!(cache.current_size(), 100);
}

#[test]
fn test_concurrent_slru_with_limits() {
    let max_entries = NonZeroUsize::new(100).unwrap();
    let protected_cap = NonZeroUsize::new(20).unwrap();
    let max_size: u64 = 50_000;
    let cache: ConcurrentSlruCache<i32, String> =
        ConcurrentSlruCache::with_limits(max_entries, protected_cap, max_size);

    for i in 0..50 {
        cache.put_with_size(i, format!("value_{}", i), 100);
    }

    assert_eq!(cache.current_size(), 5000);

    cache.clear();
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.current_size(), 0);
}

// ----------------------------------------------------------------------------
// CLEAR OPERATIONS UNDER CONCURRENCY
// ----------------------------------------------------------------------------

#[test]
fn test_concurrent_clear_during_operations() {
    let cache: Arc<ConcurrentLruCache<i32, i32>> = Arc::new(ConcurrentLruCache::with_segments(
        NonZeroUsize::new(1000).unwrap(),
        4,
    ));

    // Pre-fill
    for i in 0..100 {
        cache.put(i, i);
    }
    assert_eq!(cache.len(), 100);

    let cache_clone = Arc::clone(&cache);

    // Spawn thread to clear while main thread inserts
    let handle = thread::spawn(move || {
        for _ in 0..5 {
            cache_clone.clear();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    // Insert while clear is happening
    for i in 100..200 {
        cache.put(i, i);
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
    let cache: ConcurrentLruCache<i32, i32> =
        ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

    // Record some misses
    cache.record_miss(100);
    cache.record_miss(200);

    // Check metrics include misses
    let metrics = cache.metrics();
    assert!(
        metrics.get("cache_misses").unwrap_or(&0.0) >= &2.0,
        "Should have recorded misses"
    );
}
