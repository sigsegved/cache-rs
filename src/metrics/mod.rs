//! Cache Metrics System
//!
//! Provides a flexible metrics system for cache algorithms using BTreeMap-based
//! metrics reporting. Each cache algorithm can track its own specific metrics
//! while implementing a common CacheMetrics trait.
//!
//! # Why BTreeMap over HashMap?
//!
//! BTreeMap is used instead of HashMap for several critical reasons:
//! - **Deterministic ordering**: Metrics always appear in consistent order
//! - **Reproducible output**: Essential for testing and benchmarking comparisons
//! - **Stable serialization**: JSON/CSV exports have predictable key ordering
//! - **Better debugging**: Consistent output makes logs more readable
//!
//! The performance difference (O(log n) vs O(1)) is negligible with ~15 metric keys,
//! but the deterministic behavior is invaluable for a simulation system.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

// Re-export algorithm-specific metrics
pub mod gdsf;
pub mod lfu;
pub mod lfuda;
pub mod lru;
pub mod slru;

pub use gdsf::GdsfCacheMetrics;
pub use lfu::LfuCacheMetrics;
pub use lfuda::LfudaCacheMetrics;
pub use lru::LruCacheMetrics;
pub use slru::SlruCacheMetrics;

/// Common metrics tracked by all cache algorithms
#[derive(Debug, Default, Clone)]
pub struct CoreCacheMetrics {
    /// Total number of requests (gets) made to the cache
    pub requests: u64,

    /// Number of requests that resulted in cache hits
    pub cache_hits: u64,

    /// Total bytes of data requested from the cache (hits + misses)
    pub total_bytes_requested: u64,

    /// Total bytes served directly from cache (cache hits only)
    pub bytes_served_from_cache: u64,

    /// Total bytes written/stored into the cache
    pub bytes_written_to_cache: u64,

    /// Number of items evicted from the cache due to capacity constraints
    pub evictions: u64,

    /// Current size of data stored in the cache (in bytes)
    pub cache_size_bytes: u64,

    /// Maximum allowed cache size (in bytes) - the capacity limit
    pub max_cache_size_bytes: u64,
}

impl CoreCacheMetrics {
    /// Creates a new CoreCacheMetrics instance with the specified maximum cache size
    ///
    /// # Arguments
    /// * `max_cache_size_bytes` - The maximum allowed cache size in bytes
    pub fn new(max_cache_size_bytes: u64) -> Self {
        Self {
            max_cache_size_bytes,
            ..Default::default()
        }
    }

    /// Records a cache hit - when requested data was found in the cache
    ///
    /// This increments total requests, cache hits, total bytes requested,
    /// and bytes served from cache.
    ///
    /// # Arguments
    /// * `object_size` - Size of the object that was served from cache (in bytes)
    pub fn record_hit(&mut self, object_size: u64) {
        self.requests += 1;
        self.cache_hits += 1;
        self.total_bytes_requested += object_size;
        self.bytes_served_from_cache += object_size;
    }

    /// Records a cache miss - when requested data was not found in the cache
    ///
    /// This increments total requests and total bytes requested.
    /// Cache misses are calculated as (requests - cache_hits).
    ///
    /// # Arguments
    /// * `object_size` - Size of the object that was requested but not in cache (in bytes)
    pub fn record_miss(&mut self, object_size: u64) {
        self.requests += 1;
        self.total_bytes_requested += object_size;
        // Note: cache misses can be calculated as (requests - cache_hits)
    }

    /// Records an eviction - when an item is removed from cache due to capacity constraints
    ///
    /// This increments the eviction counter and decreases the current cache size.
    ///
    /// # Arguments
    /// * `evicted_size` - Size of the evicted object (in bytes)
    pub fn record_eviction(&mut self, evicted_size: u64) {
        self.evictions += 1;
        self.cache_size_bytes -= evicted_size;
    }

    /// Records an insertion - when new data is written to the cache
    ///
    /// This increases the current cache size and tracks bytes written to cache.
    ///
    /// # Arguments
    /// * `object_size` - Size of the object being inserted (in bytes)
    pub fn record_insertion(&mut self, object_size: u64) {
        self.cache_size_bytes += object_size;
        self.bytes_written_to_cache += object_size;
    }

    /// Records a size change for an existing cache entry
    ///
    /// This adjusts the current cache size when an existing entry's size changes.
    ///
    /// # Arguments
    /// * `old_size` - Previous size of the object (in bytes)
    /// * `new_size` - New size of the object (in bytes)
    pub fn record_size_change(&mut self, old_size: u64, new_size: u64) {
        self.cache_size_bytes = self.cache_size_bytes - old_size + new_size;
    }

    /// Calculates the cache hit rate as a percentage
    ///
    /// # Returns
    /// A value between 0.0 and 1.0 representing the hit rate, or 0.0 if no requests have been made
    pub fn hit_rate(&self) -> f64 {
        if self.requests > 0 {
            self.cache_hits as f64 / self.requests as f64
        } else {
            0.0
        }
    }

    /// Calculates the cache miss rate as a percentage
    ///
    /// # Returns
    /// A value between 0.0 and 1.0 representing the miss rate, or 0.0 if no requests have been made
    pub fn miss_rate(&self) -> f64 {
        if self.requests > 0 {
            (self.requests - self.cache_hits) as f64 / self.requests as f64
        } else {
            0.0
        }
    }

    /// Calculates the byte hit rate - ratio of bytes served from cache vs total bytes requested
    ///
    /// This metric shows how much of the requested data volume was served from cache.
    ///
    /// # Returns
    /// A value between 0.0 and 1.0 representing the byte hit rate, or 0.0 if no bytes have been requested
    pub fn byte_hit_rate(&self) -> f64 {
        if self.total_bytes_requested > 0 {
            self.bytes_served_from_cache as f64 / self.total_bytes_requested as f64
        } else {
            0.0
        }
    }

    /// Calculates cache utilization - how full the cache is relative to its maximum capacity
    ///
    /// # Returns
    /// A value between 0.0 and 1.0 representing cache utilization, or 0.0 if max capacity is 0
    pub fn cache_utilization(&self) -> f64 {
        if self.max_cache_size_bytes > 0 {
            self.cache_size_bytes as f64 / self.max_cache_size_bytes as f64
        } else {
            0.0
        }
    }

    /// Convert core metrics to BTreeMap for reporting
    ///
    /// Uses BTreeMap to ensure deterministic, consistent ordering of metrics
    /// which is critical for reproducible testing and comparison results.
    ///
    /// # Returns
    /// A BTreeMap containing all core metrics with consistent key ordering
    pub fn to_btreemap(&self) -> BTreeMap<String, f64> {
        let mut metrics = BTreeMap::new();

        // Basic counters (alphabetical order for consistency)
        metrics.insert("cache_hits".to_string(), self.cache_hits as f64);
        metrics.insert("evictions".to_string(), self.evictions as f64);
        metrics.insert("requests".to_string(), self.requests as f64);

        // Calculated metrics
        metrics.insert(
            "cache_misses".to_string(),
            (self.requests - self.cache_hits) as f64,
        );

        // Rates (0.0 to 1.0)
        metrics.insert("hit_rate".to_string(), self.hit_rate());
        metrics.insert("miss_rate".to_string(), self.miss_rate());
        metrics.insert("byte_hit_rate".to_string(), self.byte_hit_rate());

        // Bytes
        metrics.insert(
            "bytes_served_from_cache".to_string(),
            self.bytes_served_from_cache as f64,
        );
        metrics.insert(
            "bytes_written_to_cache".to_string(),
            self.bytes_written_to_cache as f64,
        );
        metrics.insert(
            "total_bytes_requested".to_string(),
            self.total_bytes_requested as f64,
        );

        // Size and utilization
        metrics.insert("cache_size_bytes".to_string(), self.cache_size_bytes as f64);
        metrics.insert(
            "max_cache_size_bytes".to_string(),
            self.max_cache_size_bytes as f64,
        );
        metrics.insert("cache_utilization".to_string(), self.cache_utilization());

        // Derived metrics
        if self.requests > 0 {
            metrics.insert(
                "avg_object_size".to_string(),
                self.total_bytes_requested as f64 / self.requests as f64,
            );
            metrics.insert(
                "eviction_rate".to_string(),
                self.evictions as f64 / self.requests as f64,
            );
        }

        metrics
    }
}

/// Trait that all cache algorithms must implement for metrics reporting
///
/// This trait provides a uniform interface for retrieving metrics from any cache implementation.
/// It allows the simulation system to collect and compare metrics across different cache algorithms.
///
/// The trait uses BTreeMap to ensure deterministic ordering of metrics, which is essential
/// for reproducible benchmarks and consistent test results.
pub trait CacheMetrics {
    /// Returns all metrics as key-value pairs in deterministic order
    ///
    /// The returned BTreeMap contains all relevant metrics for the cache algorithm,
    /// including both core metrics and any algorithm-specific metrics.
    /// Keys are sorted alphabetically for consistent output.
    ///
    /// # Returns
    /// A BTreeMap where keys are metric names and values are metric values as f64
    fn metrics(&self) -> BTreeMap<String, f64>;

    /// Algorithm name for identification
    ///
    /// # Returns
    /// A static string identifying the cache algorithm (e.g., "LRU", "LFU", "SLRU")
    fn algorithm_name(&self) -> &'static str;
}
