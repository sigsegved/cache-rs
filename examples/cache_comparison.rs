extern crate cache_rs;

use cache_rs::{GdsfCache, LfuCache, LfudaCache, LruCache, SlruCache};
use core::num::NonZeroUsize;

fn main() {
    println!("Cache Implementation Comparison");
    println!("===============================");
    println!("Each cache has capacity of 3 items.");
    println!("We'll add 4 items to see eviction behavior, then access 'apple' multiple times.");
    println!(
        "Finally, we'll add 'elderberry' to see how different algorithms choose what to evict.\n"
    );

    // Create all cache types with capacity 3
    let cap = NonZeroUsize::new(3).unwrap();
    let protected_cap = NonZeroUsize::new(2).unwrap();

    // Using constructors that take capacity directly
    // The refactoring appears to be incomplete with some caches still using the old API
    let mut lru_cache = LruCache::new(cap);
    let mut slru_cache = SlruCache::new(cap, protected_cap);
    let mut lfu_cache = LfuCache::new(cap);
    let mut lfuda_cache = LfudaCache::new(cap);
    let mut gdsf_cache = GdsfCache::new(cap);

    // Test data
    let data = vec![("apple", 1), ("banana", 2), ("cherry", 3), ("date", 4)];

    // Test data with sizes for GDSF cache
    let gdsf_data = vec![
        ("apple", 1, 10), // (key, value, size)
        ("banana", 2, 20),
        ("cherry", 3, 15),
        ("date", 4, 5),
    ];

    println!("\n1. LRU Cache (Least Recently Used):");
    for (key, value) in &data {
        if let Some(evicted) = lru_cache.put(*key, *value) {
            println!("   Evicted: {evicted:?}");
        }
        println!("   Added: {key} -> {value}");
    }

    println!("\n2. SLRU Cache (Segmented LRU):");
    for (key, value) in &data {
        if let Some(evicted) = slru_cache.put(*key, *value) {
            println!("   Evicted: {evicted:?}");
        }
        println!("   Added: {key} -> {value}");
    }

    println!("\n3. LFU Cache (Least Frequently Used):");
    for (key, value) in &data {
        if let Some(evicted) = lfu_cache.put(*key, *value) {
            println!("   Evicted: {evicted:?}");
        }
        println!("   Added: {key} -> {value}");
    }

    println!("\n4. LFUDA Cache (LFU with Dynamic Aging):");
    for (key, value) in &data {
        if let Some(evicted) = lfuda_cache.put(*key, *value) {
            println!("   Evicted: {evicted:?}");
        }
        println!("   Added: {key} -> {value}");
    }

    println!("\n5. GDSF Cache (Greedy Dual-Size Frequency):");
    println!(
        "   GDSF considers both frequency and size. Priority = (Frequency / Size) + Global_Age"
    );
    for (key, value, size) in &gdsf_data {
        if let Some(evicted) = gdsf_cache.put(*key, *value, *size) {
            println!("   Evicted: {evicted:?}");
        }
        println!(
            "   Added: {} -> {} (size: {}, priority will be 1/{} = {:.3})",
            key,
            value,
            size,
            size,
            1.0 / *size as f64
        );
    }

    println!("\nAccessing 'apple' multiple times to increase its frequency...");
    println!(
        "This should affect GDSF and frequency-based caches differently than recency-based ones."
    );

    // Access apple multiple times in each cache
    for _ in 0..3 {
        lru_cache.get(&"apple");
        slru_cache.get(&"apple");
        lfu_cache.get(&"apple");
        lfuda_cache.get(&"apple");
        gdsf_cache.get(&"apple");
    }

    println!("\nAdding 'elderberry' to see different eviction behaviors...");

    if let Some(evicted) = lru_cache.put("elderberry", 5) {
        println!("LRU evicted: {evicted:?}");
    }

    if let Some(evicted) = slru_cache.put("elderberry", 5) {
        println!("SLRU evicted: {evicted:?}");
    }

    if let Some(evicted) = lfu_cache.put("elderberry", 5) {
        println!("LFU evicted: {evicted:?}");
    }

    if let Some(evicted) = lfuda_cache.put("elderberry", 5) {
        println!("LFUDA evicted: {evicted:?}");
    }

    if let Some(evicted) = gdsf_cache.put("elderberry", 5, 8) {
        println!(
            "GDSF evicted: {evicted:?} (algorithm chose based on lowest (frequency/size) + global_age)"
        );
    } else {
        println!("GDSF: Added elderberry (no eviction needed)");
    }

    println!("\nFinal cache states:");
    println!("LRU cache size: {}", lru_cache.len());
    println!("SLRU cache size: {}", slru_cache.len());
    println!("LFU cache size: {}", lfu_cache.len());
    println!("LFUDA cache size: {}", lfuda_cache.len());
    println!(
        "GDSF cache size: {} (global age: {:.2})",
        gdsf_cache.len(),
        gdsf_cache.global_age()
    );
}
