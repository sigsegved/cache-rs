//! LRU Cache Metrics
//!
//! Metrics specific to the LRU (Least Recently Used) cache algorithm.

extern crate alloc;

use super::{CacheMetrics, CoreCacheMetrics};
use alloc::collections::BTreeMap;
use alloc::string::String;

/// LRU-specific metrics (extends CoreCacheMetrics)
///
/// This struct contains metrics specific to the LRU (Least Recently Used) cache algorithm.
/// Currently, LRU uses only the core metrics, but this structure allows for future
/// LRU-specific metrics to be added.
#[derive(Debug, Clone)]
pub struct LruCacheMetrics {
    /// Core metrics common to all cache algorithms
    pub core: CoreCacheMetrics,
    // LRU doesn't have algorithm-specific metrics beyond core metrics
    // But we keep this structure for consistency with other cache algorithms
    //
    // Future LRU-specific metrics could include:
    // - pub recency_distribution: BTreeMap<String, u64>,  // Age distribution of cached items
    // - pub access_pattern_stats: AccessPatternStats,     // Sequential vs random access patterns
}

impl LruCacheMetrics {
    /// Creates a new LruCacheMetrics instance with the specified maximum cache size
    ///
    /// # Arguments
    /// * `max_cache_size_bytes` - The maximum allowed cache size in bytes
    pub fn new(max_cache_size_bytes: u64) -> Self {
        Self {
            core: CoreCacheMetrics::new(max_cache_size_bytes),
        }
    }

    /// Converts LRU metrics to a BTreeMap for reporting
    ///
    /// This method returns all metrics relevant to the LRU cache algorithm.
    /// Currently, this includes only core metrics, but LRU-specific metrics
    /// could be added here in the future.
    ///
    /// Uses BTreeMap to ensure consistent, deterministic ordering of metrics.
    ///
    /// # Returns
    /// A BTreeMap containing all LRU cache metrics as key-value pairs
    pub fn to_btreemap(&self) -> BTreeMap<String, f64> {
        // LRU-specific metrics would go here
        // For now, LRU only uses core metrics
        //
        // Example future additions:
        // metrics.insert("avg_item_age".to_string(), self.calculate_avg_age());
        // metrics.insert("recency_variance".to_string(), self.calculate_recency_variance());

        self.core.to_btreemap()
    }
}

impl CacheMetrics for LruCacheMetrics {
    /// Returns all LRU cache metrics as key-value pairs in deterministic order
    ///
    /// # Returns
    /// A BTreeMap containing all metrics tracked by this LRU cache instance
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.to_btreemap()
    }

    /// Returns the algorithm name for this cache implementation
    ///
    /// # Returns
    /// "LRU" - identifying this as a Least Recently Used cache
    fn algorithm_name(&self) -> &'static str {
        "LRU"
    }
}
