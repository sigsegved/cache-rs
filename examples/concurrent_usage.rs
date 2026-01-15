//! Concurrent Cache Usage Examples
//!
//! This example demonstrates multi-threaded usage patterns for cache-rs concurrent caches.
//!
//! Run with: cargo run --example concurrent_usage --features concurrent

extern crate cache_rs;

use cache_rs::{
    ConcurrentGdsfCache, ConcurrentLfuCache, ConcurrentLfudaCache, ConcurrentLruCache,
    ConcurrentSlruCache,
};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

fn main() {
    println!("Concurrent Cache Usage Examples");
    println!("================================\n");

    basic_concurrent_usage();
    println!();

    zero_copy_get_with();
    println!();

    segment_tuning();
    println!();

    all_concurrent_cache_types();
    println!();

    throughput_comparison();
}

/// Basic multi-threaded cache usage
fn basic_concurrent_usage() {
    println!("1. Basic Concurrent Usage");
    println!("   -----------------------");

    // Create a concurrent LRU cache with default settings
    let cache = Arc::new(ConcurrentLruCache::new(NonZeroUsize::new(1000).unwrap()));

    // Spawn multiple threads that read and write concurrently
    let num_threads = 4;
    let ops_per_thread = 1000;

    let handles: Vec<_> = (0..num_threads)
        .map(|thread_id| {
            let cache = Arc::clone(&cache);
            thread::spawn(move || {
                for i in 0..ops_per_thread {
                    let key = format!("thread{}-key{}", thread_id, i);
                    let value = thread_id * 10000 + i;

                    // Write
                    cache.put(key.clone(), value);

                    // Read
                    if let Some(v) = cache.get(&key) {
                        assert_eq!(v, value);
                    }
                }
            })
        })
        .collect();

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread panicked");
    }

    println!(
        "   Completed {} operations across {} threads",
        num_threads * ops_per_thread * 2, // 2 ops per iteration (put + get)
        num_threads
    );
    println!("   Final cache size: {} items", cache.len());
}

/// Zero-copy access pattern using get_with()
fn zero_copy_get_with() {
    println!("2. Zero-Copy Access with get_with()");
    println!("   ----------------------------------");

    let cache = ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());

    // Store a large value
    let large_data = vec![1u8; 1024]; // 1KB of data
    cache.put("large_key".to_string(), large_data);

    // Process the value without cloning using get_with()
    let sum: Option<u64> = cache.get_with(&"large_key".to_string(), |data| {
        data.iter().map(|&x| x as u64).sum()
    });

    println!("   Stored 1KB of data in cache");
    println!(
        "   Computed sum without cloning: {}",
        sum.unwrap_or_default()
    );

    // Compare: get() would clone the entire 1KB vector
    let _cloned_data = cache.get(&"large_key".to_string());
    println!("   get() returns a clone - use get_with() to avoid cloning");

    // Practical example: check if value meets a condition
    let has_zeros: Option<bool> =
        cache.get_with(&"large_key".to_string(), |data| data.contains(&0));
    println!("   Data contains zeros: {}", has_zeros.unwrap_or(false));
}

/// Demonstrate segment count tuning for different workloads
fn segment_tuning() {
    println!("3. Segment Count Tuning");
    println!("   ---------------------");

    let capacity = NonZeroUsize::new(10000).unwrap();

    // Default: 16 segments (good for most workloads)
    let default_cache: ConcurrentLruCache<String, i32> = ConcurrentLruCache::new(capacity);
    println!(
        "   Default cache: {} segments",
        default_cache.segment_count()
    );

    // Custom: 32 segments (better for high-contention workloads)
    let high_concurrency = ConcurrentLruCache::<String, i32>::with_segments(capacity, 32);
    println!(
        "   High-concurrency cache: {} segments",
        high_concurrency.segment_count()
    );

    // Custom: 4 segments (lower memory overhead for low-contention)
    let low_contention = ConcurrentLruCache::<String, i32>::with_segments(capacity, 4);
    println!(
        "   Low-contention cache: {} segments",
        low_contention.segment_count()
    );

    println!();
    println!("   Segment tuning guidelines:");
    println!("   - More segments = better parallelism, higher memory");
    println!("   - Use powers of 2 for best hash distribution");
    println!("   - Start with default (16), increase if high contention");
}

/// Show all available concurrent cache types
fn all_concurrent_cache_types() {
    println!("4. All Concurrent Cache Types");
    println!("   ----------------------------");

    let capacity = NonZeroUsize::new(100).unwrap();
    let protected = NonZeroUsize::new(20).unwrap();

    // ConcurrentLruCache - General purpose
    let lru = ConcurrentLruCache::<String, i32>::new(capacity);
    lru.put("key".to_string(), 1);
    println!("   ConcurrentLruCache: General purpose, recency-based");

    // ConcurrentSlruCache - Scan resistant
    let slru = ConcurrentSlruCache::<String, i32>::new(capacity, protected);
    slru.put("key".to_string(), 1);
    println!("   ConcurrentSlruCache: Scan resistant, two-segment design");

    // ConcurrentLfuCache - Frequency based
    let lfu = ConcurrentLfuCache::<String, i32>::new(capacity);
    lfu.put("key".to_string(), 1);
    println!("   ConcurrentLfuCache: Frequency-based eviction");

    // ConcurrentLfudaCache - Adaptive frequency
    let lfuda = ConcurrentLfudaCache::<String, i32>::new(capacity);
    lfuda.put("key".to_string(), 1);
    println!("   ConcurrentLfudaCache: Frequency + aging for changing patterns");

    // ConcurrentGdsfCache - Size-aware (note: put takes size parameter)
    let gdsf = ConcurrentGdsfCache::<String, Vec<u8>>::new(
        NonZeroUsize::new(10000).unwrap(), // Capacity in size units
    );
    gdsf.put("small.txt".to_string(), vec![0u8; 100], 100);
    gdsf.put("large.jpg".to_string(), vec![0u8; 5000], 5000);
    println!("   ConcurrentGdsfCache: Size-aware, for variable-size objects");
}

/// Compare throughput across different segment configurations
fn throughput_comparison() {
    println!("5. Throughput Comparison (8 threads, 10K ops each)");
    println!("   -------------------------------------------------");

    let ops_per_thread = 10_000;
    let num_threads = 8;

    for segments in [1, 4, 8, 16, 32] {
        let cache = Arc::new(ConcurrentLruCache::<i32, i32>::with_segments(
            NonZeroUsize::new(10000).unwrap(),
            segments,
        ));

        let start = Instant::now();

        let handles: Vec<_> = (0..num_threads)
            .map(|t| {
                let cache = Arc::clone(&cache);
                thread::spawn(move || {
                    let offset = t * ops_per_thread;
                    for i in 0..ops_per_thread {
                        let key = offset + i;
                        cache.put(key, key);
                        cache.get(&key);
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }

        let elapsed = start.elapsed();
        let total_ops = num_threads * ops_per_thread * 2;
        let ops_per_sec = (total_ops as f64 / elapsed.as_secs_f64()) as u64;

        println!(
            "   {:2} segments: {:>7.2?} ({:>10} ops/sec)",
            segments, elapsed, ops_per_sec
        );
    }

    println!();
    println!("   More segments generally improve throughput up to a point.");
    println!("   Optimal segment count depends on workload and hardware.");
}
