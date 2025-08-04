// filepath: examples/complete_metrics_demo.rs
//! Comprehensive Cache Metrics Demonstration
//!
//! This example demonstrates the complete metrics system implemented across all
//! cache algorithms (LRU, LFU, SLRU, LFUDA, GDSF), showing both core metrics
//! and algorithm-specific metrics for performance analysis and comparison.

use cache_rs::{
    config::{
        gdsf::GdsfCacheConfig, lfu::LfuCacheConfig, lfuda::LfudaCacheConfig, lru::LruCacheConfig,
        slru::SlruCacheConfig,
    },
    gdsf::GdsfCache,
    lfu::LfuCache,
    lfuda::LfudaCache,
    lru::LruCache,
    metrics::CacheMetrics,
    slru::SlruCache,
};
use core::num::NonZeroUsize;
use std::collections::BTreeMap;

fn main() {
    println!("🚀 Cache Metrics System - Complete Demonstration");
    println!("==================================================\n");

    // Test with a small capacity to force evictions and see interesting metrics
    let capacity = NonZeroUsize::new(3).unwrap();

    println!("📊 Comparing cache algorithms with identical workloads:");
    println!("   • Capacity: {} items", capacity.get());
    println!("   • Operations: Insert 5 items → Access patterns → Insert 2 more");
    println!("   • This will trigger evictions and show algorithm differences\n");

    // Initialize and test all cache types
    let caches: Vec<(String, Box<dyn CacheMetrics>)> = vec![
        ("LRU".to_string(), Box::new(test_lru_cache(capacity))),
        ("LFU".to_string(), Box::new(test_lfu_cache(capacity))),
        ("SLRU".to_string(), Box::new(test_slru_cache(capacity))),
        ("LFUDA".to_string(), Box::new(test_lfuda_cache(capacity))),
        ("GDSF".to_string(), Box::new(test_gdsf_cache(capacity))),
    ];

    // Display metrics comparison
    display_metrics_comparison(&caches);

    // Show deterministic ordering demonstration
    demonstrate_deterministic_ordering(&*caches[0].1);
}

/// Test LRU cache with standard workload
fn test_lru_cache(capacity: NonZeroUsize) -> LruCache<&'static str, i32> {
    println!("🔄 Testing LRU Cache...");
    // Create the config but use capacity directly with the constructor
    let _config = LruCacheConfig::new(capacity);
    let mut cache = LruCache::new(capacity);

    // Standard workload
    cache.put("apple", 1);
    cache.put("banana", 2);
    cache.put("cherry", 3);

    // Access patterns (apple becomes most recently used)
    cache.get(&"apple");
    cache.get(&"apple");
    cache.get(&"banana");

    // Simulate cache misses
    cache.record_miss(64);
    cache.record_miss(32);

    // More insertions (will trigger evictions)
    cache.put("date", 4);
    cache.put("elderberry", 5);

    println!("   ✅ LRU test completed");
    cache
}

/// Test LFU cache with standard workload  
fn test_lfu_cache(capacity: NonZeroUsize) -> LfuCache<&'static str, i32> {
    println!("📊 Testing LFU Cache...");
    // Create the config but use capacity directly with the constructor
    let _config = LfuCacheConfig::new(capacity);
    let mut cache = LfuCache::new(capacity);

    // Standard workload
    cache.put("apple", 1);
    cache.put("banana", 2);
    cache.put("cherry", 3);

    // Access patterns (apple becomes most frequently used)
    cache.get(&"apple");
    cache.get(&"apple");
    cache.get(&"banana");

    // Simulate cache misses
    cache.record_miss(64);
    cache.record_miss(32);

    // More insertions
    cache.put("date", 4);
    cache.put("elderberry", 5);

    println!("   ✅ LFU test completed");
    cache
}

/// Test SLRU cache with standard workload
fn test_slru_cache(capacity: NonZeroUsize) -> SlruCache<&'static str, i32> {
    println!("🏟️  Testing SLRU Cache...");
    let protected_capacity = NonZeroUsize::new(2).unwrap();
    let _config = SlruCacheConfig::new(capacity, protected_capacity);
    let mut cache = SlruCache::new(capacity, protected_capacity);

    // Standard workload
    cache.put("apple", 1);
    cache.put("banana", 2);
    cache.put("cherry", 3);

    // Access patterns (will cause promotions)
    cache.get(&"apple");
    cache.get(&"apple");
    cache.get(&"banana");

    // Simulate cache misses
    cache.record_miss(64);
    cache.record_miss(32);

    // More insertions
    cache.put("date", 4);
    cache.put("elderberry", 5);

    println!("   ✅ SLRU test completed");
    cache
}

/// Test LFUDA cache with standard workload
fn test_lfuda_cache(capacity: NonZeroUsize) -> LfudaCache<&'static str, i32> {
    println!("⏳ Testing LFUDA Cache...");
    // Create the config but use capacity directly with the constructor
    let _config = LfudaCacheConfig::new(capacity);
    let mut cache = LfudaCache::new(capacity);

    // Standard workload
    cache.put("apple", 1);
    cache.put("banana", 2);
    cache.put("cherry", 3);

    // Access patterns
    cache.get(&"apple");
    cache.get(&"apple");
    cache.get(&"banana");

    // Simulate cache misses
    cache.record_miss(64);
    cache.record_miss(32);

    // More insertions (will trigger aging)
    cache.put("date", 4);
    cache.put("elderberry", 5);

    println!("   ✅ LFUDA test completed");
    cache
}

/// Test GDSF cache with standard workload
fn test_gdsf_cache(capacity: NonZeroUsize) -> GdsfCache<&'static str, i32> {
    println!("⚖️  Testing GDSF Cache...");
    // Create the config but use capacity directly with the constructor
    let _config = GdsfCacheConfig::new(capacity);
    let mut cache = GdsfCache::new(capacity);

    // Standard workload (with different sizes)
    cache.put("apple", 1, 10); // Small item
    cache.put("banana", 2, 50); // Large item
    cache.put("cherry", 3, 25); // Medium item

    // Access patterns
    cache.get(&"apple");
    cache.get(&"apple");
    cache.get(&"banana");

    // Simulate cache misses
    cache.record_miss(64);
    cache.record_miss(32);

    // More insertions
    cache.put("date", 4, 15);
    cache.put("elderberry", 5, 40);

    println!("   ✅ GDSF test completed");
    cache
}

/// Display comprehensive metrics comparison across all algorithms
fn display_metrics_comparison(caches: &[(String, Box<dyn CacheMetrics>)]) {
    println!("\n📈 COMPREHENSIVE METRICS COMPARISON");
    println!("====================================\n");

    // Core metrics table
    println!("📊 Core Performance Metrics:");
    println!(
        "{:<10} {:<8} {:<8} {:<10} {:<12} {:<8}",
        "Algorithm", "Hits", "Misses", "Evictions", "Hit Rate %", "Requests"
    );
    println!("{}", "-".repeat(70));

    for (name, cache) in caches {
        let metrics = cache.metrics();
        let hits = metrics.get("cache_hits").unwrap_or(&0.0);
        let requests = metrics.get("requests").unwrap_or(&0.0);
        let evictions = metrics.get("evictions").unwrap_or(&0.0);
        let hit_rate = metrics.get("hit_rate").unwrap_or(&0.0) * 100.0;
        let misses = requests - hits;

        println!(
            "{name:<10} {hits:<8.0} {misses:<8.0} {evictions:<10.0} {hit_rate:<12.1} {requests:<8.0}"
        );
    }

    // Algorithm-specific metrics
    println!("\n🔍 Algorithm-Specific Metrics:\n");

    for (name, cache) in caches {
        let metrics = cache.metrics();
        println!("{}️ {} Cache Metrics:", get_algorithm_emoji(name), name);

        match name.as_str() {
            "LRU" => print_lru_metrics(&metrics),
            "LFU" => print_lfu_metrics(&metrics),
            "SLRU" => print_slru_metrics(&metrics),
            "LFUDA" => print_lfuda_metrics(&metrics),
            "GDSF" => print_gdsf_metrics(&metrics),
            _ => {}
        }
        println!();
    }
}

/// Display algorithm-specific metrics for LRU
fn print_lru_metrics(metrics: &BTreeMap<String, f64>) {
    if let Some(updates) = metrics.get("recency_updates") {
        println!("  • Recency Updates: {updates:.0}");
    }
    if let Some(rate) = metrics.get("cache_utilization") {
        println!("  • Cache Utilization: {:.1}%", rate * 100.0);
    }
}

/// Display algorithm-specific metrics for LFU
fn print_lfu_metrics(metrics: &BTreeMap<String, f64>) {
    let keys = [
        "min_frequency",
        "max_frequency",
        "frequency_range",
        "average_frequency",
    ];
    for key in &keys {
        if let Some(value) = metrics.get(*key) {
            println!(
                "  • {}: {:.2}",
                key.replace('_', " ").to_title_case(),
                value
            );
        }
    }
}

/// Display algorithm-specific metrics for SLRU
fn print_slru_metrics(metrics: &BTreeMap<String, f64>) {
    let keys = [
        "probationary_hits",
        "protected_hits",
        "promotions",
        "probationary_evictions",
    ];
    for key in &keys {
        if let Some(value) = metrics.get(*key) {
            println!(
                "  • {}: {:.0}",
                key.replace('_', " ").to_title_case(),
                value
            );
        }
    }
}

/// Display algorithm-specific metrics for LFUDA
fn print_lfuda_metrics(metrics: &BTreeMap<String, f64>) {
    let keys = [
        "global_age",
        "total_aging_events",
        "aging_effectiveness",
        "items_benefited_from_aging",
    ];
    for key in &keys {
        if let Some(value) = metrics.get(*key) {
            println!(
                "  • {}: {:.2}",
                key.replace('_', " ").to_title_case(),
                value
            );
        }
    }
}

/// Display algorithm-specific metrics for GDSF
fn print_gdsf_metrics(metrics: &BTreeMap<String, f64>) {
    let keys = [
        "average_item_size",
        "size_based_evictions",
        "priority_range",
    ];
    for key in &keys {
        if let Some(value) = metrics.get(*key) {
            println!(
                "  • {}: {:.2}",
                key.replace('_', " ").to_title_case(),
                value
            );
        }
    }
}

/// Get emoji for algorithm display
fn get_algorithm_emoji(name: &str) -> &str {
    match name {
        "LRU" => "🔄",
        "LFU" => "📊",
        "SLRU" => "🏟",
        "LFUDA" => "⏳",
        "GDSF" => "⚖",
        _ => "📈",
    }
}

/// Helper trait to convert string to title case
trait ToTitleCase {
    fn to_title_case(&self) -> String;
}

impl ToTitleCase for str {
    fn to_title_case(&self) -> String {
        self.split(' ')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => {
                        first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                    }
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Demonstrate the deterministic ordering of BTreeMap metrics
fn demonstrate_deterministic_ordering(cache: &dyn CacheMetrics) {
    println!("\n🔢 Deterministic Metrics Ordering (BTreeMap):");
    println!("==============================================");
    println!("All metrics use BTreeMap for consistent, reproducible ordering across runs.\n");

    let metrics = cache.metrics();
    println!("Sample metrics keys (showing deterministic alphabetical ordering):");
    for (i, key) in metrics.keys().take(8).enumerate() {
        println!("  {}. {}", i + 1, key);
    }

    println!("\n✅ Metrics Integration Complete!");
    println!(
        "🎯 All {} cache algorithms integrated with comprehensive metrics tracking!",
        5
    );
    println!("📊 Ready for cache simulation and performance comparison!");
}
