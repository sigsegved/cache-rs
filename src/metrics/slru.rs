//! SLRU Cache Metrics
//!
//! Metrics specific to the SLRU (Segmented Least Recently Used) cache algorithm.

extern crate alloc;

use super::{CacheMetrics, CoreCacheMetrics};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

/// SLRU-specific metrics (extends CoreCacheMetrics)
///
/// This struct contains metrics specific to the SLRU (Segmented LRU) cache algorithm.
/// SLRU divides the cache into probationary and protected segments, so these metrics
/// focus on segment utilization and promotion/demotion patterns.
#[derive(Debug, Clone)]
pub struct SlruCacheMetrics {
    /// Core metrics common to all cache algorithms
    pub core: CoreCacheMetrics,

    /// Number of items currently in the probationary segment
    pub probationary_size: u64,

    /// Number of items currently in the protected segment
    pub protected_size: u64,

    /// Maximum allowed size for the protected segment
    pub protected_max_size: u64,

    /// Total number of promotions from probationary to protected segment
    pub total_promotions: u64,

    /// Total number of demotions from protected to probationary segment
    pub total_demotions: u64,

    /// Number of cache hits in the probationary segment
    pub probationary_hits: u64,

    /// Number of cache hits in the protected segment
    pub protected_hits: u64,

    /// Number of evictions from the probationary segment
    pub probationary_evictions: u64,

    /// Number of evictions from the protected segment
    pub protected_evictions: u64,
}

impl SlruCacheMetrics {
    /// Creates a new SlruCacheMetrics instance with the specified parameters
    ///
    /// # Arguments
    /// * `max_cache_size_bytes` - The maximum allowed cache size in bytes
    /// * `protected_max_size` - The maximum number of items in protected segment
    pub fn new(max_cache_size_bytes: u64, protected_max_size: u64) -> Self {
        Self {
            core: CoreCacheMetrics::new(max_cache_size_bytes),
            probationary_size: 0,
            protected_size: 0,
            protected_max_size,
            total_promotions: 0,
            total_demotions: 0,
            probationary_hits: 0,
            protected_hits: 0,
            probationary_evictions: 0,
            protected_evictions: 0,
        }
    }

    /// Records a promotion from probationary to protected segment
    pub fn record_promotion(&mut self) {
        self.total_promotions += 1;
    }

    /// Records a demotion from protected to probationary segment
    pub fn record_demotion(&mut self) {
        self.total_demotions += 1;
    }

    /// Records a cache hit in the probationary segment
    ///
    /// # Arguments
    /// * `object_size` - Size of the object that was served from cache (in bytes)
    pub fn record_probationary_hit(&mut self, object_size: u64) {
        self.core.record_hit(object_size);
        self.probationary_hits += 1;
    }

    /// Records a cache hit in the protected segment
    ///
    /// # Arguments
    /// * `object_size` - Size of the object that was served from cache (in bytes)
    pub fn record_protected_hit(&mut self, object_size: u64) {
        self.core.record_hit(object_size);
        self.protected_hits += 1;
    }

    /// Records an eviction from the probationary segment
    ///
    /// # Arguments
    /// * `evicted_size` - Size of the evicted object (in bytes)
    pub fn record_probationary_eviction(&mut self, evicted_size: u64) {
        self.core.record_eviction(evicted_size);
        self.probationary_evictions += 1;
    }

    /// Records an eviction from the protected segment
    ///
    /// # Arguments
    /// * `evicted_size` - Size of the evicted object (in bytes)
    pub fn record_protected_eviction(&mut self, evicted_size: u64) {
        self.core.record_eviction(evicted_size);
        self.protected_evictions += 1;
    }

    /// Updates the segment sizes
    ///
    /// # Arguments
    /// * `probationary_size` - Current number of items in probationary segment
    /// * `protected_size` - Current number of items in protected segment
    pub fn update_segment_sizes(&mut self, probationary_size: u64, protected_size: u64) {
        self.probationary_size = probationary_size;
        self.protected_size = protected_size;
    }

    /// Calculates the protection ratio (protected hits / total hits)
    ///
    /// # Returns
    /// Ratio of hits in protected segment vs total hits, or 0.0 if no hits
    pub fn protection_ratio(&self) -> f64 {
        if self.core.cache_hits > 0 {
            self.protected_hits as f64 / self.core.cache_hits as f64
        } else {
            0.0
        }
    }

    /// Calculates the promotion efficiency (promotions / probationary hits)
    ///
    /// # Returns
    /// How often probationary hits lead to promotions, or 0.0 if no probationary hits
    pub fn promotion_efficiency(&self) -> f64 {
        if self.probationary_hits > 0 {
            self.total_promotions as f64 / self.probationary_hits as f64
        } else {
            0.0
        }
    }

    /// Calculates protected segment utilization
    ///
    /// # Returns
    /// Ratio of current protected size to maximum protected size
    pub fn protected_utilization(&self) -> f64 {
        if self.protected_max_size > 0 {
            self.protected_size as f64 / self.protected_max_size as f64
        } else {
            0.0
        }
    }

    /// Converts SLRU metrics to a BTreeMap for reporting
    ///
    /// This method returns all metrics relevant to the SLRU cache algorithm,
    /// including both core metrics and SLRU-specific segment metrics.
    ///
    /// Uses BTreeMap to ensure consistent, deterministic ordering of metrics.
    ///
    /// # Returns
    /// A BTreeMap containing all SLRU cache metrics as key-value pairs
    pub fn to_btreemap(&self) -> BTreeMap<String, f64> {
        let mut metrics = self.core.to_btreemap();

        // SLRU-specific segment metrics
        metrics.insert(
            "probationary_size".to_string(),
            self.probationary_size as f64,
        );
        metrics.insert("protected_size".to_string(), self.protected_size as f64);
        metrics.insert(
            "protected_max_size".to_string(),
            self.protected_max_size as f64,
        );
        metrics.insert(
            "protected_utilization".to_string(),
            self.protected_utilization(),
        );

        // Movement metrics
        metrics.insert("total_promotions".to_string(), self.total_promotions as f64);
        metrics.insert("total_demotions".to_string(), self.total_demotions as f64);

        // Segment-specific hit metrics
        metrics.insert(
            "probationary_hits".to_string(),
            self.probationary_hits as f64,
        );
        metrics.insert("protected_hits".to_string(), self.protected_hits as f64);
        metrics.insert("protection_ratio".to_string(), self.protection_ratio());

        // Segment-specific eviction metrics
        metrics.insert(
            "probationary_evictions".to_string(),
            self.probationary_evictions as f64,
        );
        metrics.insert(
            "protected_evictions".to_string(),
            self.protected_evictions as f64,
        );

        // Efficiency metrics
        metrics.insert(
            "promotion_efficiency".to_string(),
            self.promotion_efficiency(),
        );

        if self.core.requests > 0 {
            metrics.insert(
                "promotion_rate".to_string(),
                self.total_promotions as f64 / self.core.requests as f64,
            );
            metrics.insert(
                "demotion_rate".to_string(),
                self.total_demotions as f64 / self.core.requests as f64,
            );
        }

        metrics
    }
}

impl CacheMetrics for SlruCacheMetrics {
    /// Returns all SLRU cache metrics as key-value pairs in deterministic order
    ///
    /// # Returns
    /// A BTreeMap containing all metrics tracked by this SLRU cache instance
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.to_btreemap()
    }

    /// Returns the algorithm name for this cache implementation
    ///
    /// # Returns
    /// "SLRU" - identifying this as a Segmented Least Recently Used cache
    fn algorithm_name(&self) -> &'static str {
        "SLRU"
    }
}
