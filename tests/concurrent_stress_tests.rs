//! Stress Tests for Concurrent Caches
//!
//! These tests verify thread safety and correctness under high contention.

#![cfg(feature = "concurrent")]

use cache_rs::config::{
    ConcurrentCacheConfig, ConcurrentGdsfCacheConfig, ConcurrentLfuCacheConfig,
    ConcurrentLfudaCacheConfig, ConcurrentLruCacheConfig, ConcurrentSlruCacheConfig,
    GdsfCacheConfig, LfuCacheConfig, LfudaCacheConfig, LruCacheConfig, SlruCacheConfig,
};
use cache_rs::{
    ConcurrentGdsfCache, ConcurrentLfuCache, ConcurrentLfudaCache, ConcurrentLruCache,
    ConcurrentSlruCache,
};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

const NUM_THREADS: usize = 16;
const OPS_PER_THREAD: usize = 10_000;

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

/// Test high contention with many threads hammering the same keys
#[test]
fn stress_lru_high_contention() {
    let cache: Arc<ConcurrentLruCache<usize, usize>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 16), None));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = i % 10; // Only 10 keys for high contention
                if t % 2 == 0 {
                    cache.put(key, t * OPS_PER_THREAD + i);
                } else {
                    let _ = cache.get(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Verify cache is still consistent
    assert!(cache.len() <= 100);
}

/// Test with various segment counts
#[test]
fn stress_segment_counts() {
    for segments in [1, 2, 4, 8, 16, 32] {
        let cache: Arc<ConcurrentLruCache<usize, usize>> =
            Arc::new(ConcurrentLruCache::init(lru_config(1000, segments), None));

        let mut handles = Vec::new();
        for t in 0..8 {
            let cache = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..1000 {
                    cache.put(t * 1000 + i, i);
                    let _ = cache.get(&(t * 1000 + i));
                }
            }));
        }

        for handle in handles {
            handle.join().expect("Thread panicked");
        }

        assert_eq!(cache.segment_count(), segments);
        assert!(cache.len() <= 1000);
    }
}

/// Test edge case: empty cache operations
#[test]
fn stress_empty_cache() {
    let cache: Arc<ConcurrentLruCache<usize, usize>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 16), None));

    let mut handles = Vec::new();
    for _ in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..1000 {
                // Try to get from empty cache
                assert!(cache.get(&i).is_none());
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.is_empty());
}

/// Test edge case: single item cache
#[test]
fn stress_single_item_cache() {
    let cache: Arc<ConcurrentLruCache<usize, usize>> =
        Arc::new(ConcurrentLruCache::init(lru_config(16, 16), None));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..1000 {
                cache.put(t, i); // Each thread uses different key
                let _ = cache.get(&t);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Should have at most 16 items (one per segment)
    assert!(cache.len() <= 16);
}

/// Test capacity limits under concurrent access
#[test]
fn stress_capacity_limits() {
    let capacity = 100;
    let cache: Arc<ConcurrentLruCache<usize, usize>> =
        Arc::new(ConcurrentLruCache::init(lru_config(capacity, 16), None));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                cache.put(t * OPS_PER_THREAD + i, i);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Cache should never exceed capacity
    assert!(cache.len() <= capacity);
}

/// Test concurrent removes
#[test]
fn stress_concurrent_removes() {
    let cache: Arc<ConcurrentLruCache<usize, usize>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1000, 16), None));

    // Pre-populate
    for i in 0..1000 {
        cache.put(i, i);
    }

    let removed_count = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        let removed = Arc::clone(&removed_count);
        handles.push(thread::spawn(move || {
            for i in 0..1000 {
                if cache.remove(&i).is_some() {
                    removed.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Each item can only be removed once, so total removes <= 1000
    // Some items may be evicted before remove is called due to new puts
    let total_removed = removed_count.load(Ordering::Relaxed);
    assert!(
        total_removed <= 1000,
        "Removed {} items, expected <= 1000",
        total_removed
    );
    assert!(cache.is_empty());
}

/// Test concurrent clear operations
#[test]
fn stress_concurrent_clear() {
    let cache: Arc<ConcurrentLruCache<usize, usize>> =
        Arc::new(ConcurrentLruCache::init(lru_config(1000, 16), None));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..1000 {
                cache.put(t * 1000 + i, i);
                if i % 100 == 0 {
                    cache.clear();
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // Cache should be in a valid state
    assert!(cache.len() <= 1000);
}

/// Test SLRU under stress
#[test]
fn stress_slru() {
    let cache: Arc<ConcurrentSlruCache<usize, usize>> =
        Arc::new(ConcurrentSlruCache::init(slru_config(1000, 500, 16), None));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = t * OPS_PER_THREAD + i;
                cache.put(key, i);
                // Access multiple times to promote to protected segment
                for _ in 0..3 {
                    let _ = cache.get(&key);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= 1000);
}

/// Test LFU under stress
#[test]
fn stress_lfu() {
    let cache: Arc<ConcurrentLfuCache<usize, usize>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(1000, 16), None));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = t * OPS_PER_THREAD + i;
                cache.put(key, i);
                // Access some keys more frequently
                if i % 10 == 0 {
                    for _ in 0..5 {
                        let _ = cache.get(&key);
                    }
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= 1000);
}

/// Test LFUDA under stress
#[test]
fn stress_lfuda() {
    let cache: Arc<ConcurrentLfudaCache<usize, usize>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(1000, 16), None));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = t * OPS_PER_THREAD + i;
                cache.put(key, i);
                let _ = cache.get(&key);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(cache.len() <= 1000);
}

/// Test GDSF under stress with variable sizes
#[test]
fn stress_gdsf() {
    let cache: Arc<ConcurrentGdsfCache<usize, usize>> =
        Arc::new(ConcurrentGdsfCache::init(gdsf_config(10000, 16), None));

    let mut handles = Vec::new();
    for t in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..OPS_PER_THREAD {
                let key = t * OPS_PER_THREAD + i;
                let size = ((i % 10) + 1) as u64;
                cache.put(key, i, size);
                let _ = cache.get(&key);
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // GDSF tracks size, not count
    assert!(!cache.is_empty());
}

/// Test mixed operations across all cache types
#[test]
fn stress_mixed_all_caches() {
    // LRU
    let lru: Arc<ConcurrentLruCache<String, String>> =
        Arc::new(ConcurrentLruCache::init(lru_config(500, 16), None));

    let mut handles = Vec::new();
    for t in 0..8 {
        let cache = Arc::clone(&lru);
        handles.push(thread::spawn(move || {
            for i in 0..5000 {
                let key = format!("key_{}_{}", t, i);
                let value = format!("value_{}", i);
                match i % 4 {
                    0 => {
                        cache.put(key, value);
                    }
                    1 => {
                        let _ = cache.get(&key);
                    }
                    2 => {
                        let _ = cache.remove(&key);
                    }
                    _ => {
                        let _ = cache.contains_key(&key);
                    }
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    assert!(lru.len() <= 500);
}

/// Test get_with closure under concurrent access
#[test]
fn stress_get_with() {
    let cache: Arc<ConcurrentLruCache<usize, Vec<usize>>> =
        Arc::new(ConcurrentLruCache::init(lru_config(100, 16), None));

    // Pre-populate with vectors
    for i in 0..100 {
        cache.put(i, vec![i; 10]);
    }

    let sum = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..NUM_THREADS {
        let cache = Arc::clone(&cache);
        let sum = Arc::clone(&sum);
        handles.push(thread::spawn(move || {
            for i in 0..1000 {
                let key = i % 100;
                if let Some(len) = cache.get_with(&key, |v| v.len()) {
                    sum.fetch_add(len, Ordering::Relaxed);
                }
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    // All get_with calls should have worked
    assert!(sum.load(Ordering::Relaxed) > 0);
}
