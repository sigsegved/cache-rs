//! LFU Cache Metrics
//!
//! Metrics specific to the LFU (Least Frequently Used) cache algorithm.

extern crate alloc;

use super::{CacheMetrics, CoreCacheMetrics};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

/// LFU-specific metrics (extends CoreCacheMetrics)
///
/// This struct contains metrics specific to the LFU (Least Frequently Used) cache algorithm.
/// LFU tracks frequency of access for each item, so these metrics focus on frequency
/// distribution and access patterns.
#[derive(Debug, Clone)]
pub struct LfuCacheMetrics {
    /// Core metrics common to all cache algorithms
    pub core: CoreCacheMetrics,

    /// Current minimum frequency in the cache
    pub min_frequency: u64,

    /// Current maximum frequency in the cache
    pub max_frequency: u64,

    /// Total number of frequency increments (every cache hit increases frequency)
    pub total_frequency_increments: u64,

    /// Number of unique frequency levels currently in use
    pub active_frequency_levels: u64,
}

impl LfuCacheMetrics {
    /// Creates a new LfuCacheMetrics instance with the specified maximum cache size
    ///
    /// # Arguments
    /// * `max_cache_size_bytes` - The maximum allowed cache size in bytes
    pub fn new(max_cache_size_bytes: u64) -> Self {
        Self {
            core: CoreCacheMetrics::new(max_cache_size_bytes),
            min_frequency: 0,
            max_frequency: 0,
            total_frequency_increments: 0,
            active_frequency_levels: 0,
        }
    }

    /// Records a frequency increment (when an item is accessed and its frequency increases)
    ///
    /// # Arguments
    /// * `old_frequency` - The previous frequency value
    /// * `new_frequency` - The new frequency value for the accessed item
    pub fn record_frequency_increment(&mut self, _old_frequency: usize, new_frequency: usize) {
        self.total_frequency_increments += 1;

        // Update min/max frequency tracking
        let new_freq_u64 = new_frequency as u64;
        if self.min_frequency == 0 || new_freq_u64 < self.min_frequency {
            self.min_frequency = new_freq_u64;
        }
        if new_freq_u64 > self.max_frequency {
            self.max_frequency = new_freq_u64;
        }
    }

    /// Records a cache hit with frequency information
    ///
    /// # Arguments
    /// * `object_size` - Size of the object that was served from cache (in bytes)
    /// * `frequency` - Current frequency of the accessed item
    pub fn record_frequency_hit(&mut self, object_size: u64, frequency: usize) {
        self.core.record_hit(object_size);

        // Update frequency bounds
        let freq_u64 = frequency as u64;
        if self.min_frequency == 0 || freq_u64 < self.min_frequency {
            self.min_frequency = freq_u64;
        }
        if freq_u64 > self.max_frequency {
            self.max_frequency = freq_u64;
        }
    }

    /// Updates the frequency levels based on current frequency lists
    ///
    /// # Arguments
    /// * `frequency_lists` - Map of frequency to list of items at that frequency
    pub fn update_frequency_levels<T>(&mut self, frequency_lists: &BTreeMap<usize, T>) {
        self.active_frequency_levels = frequency_lists.len() as u64;

        // Update min/max from the actual frequency keys
        if let (Some(&min_freq), Some(&max_freq)) =
            (frequency_lists.keys().min(), frequency_lists.keys().max())
        {
            self.min_frequency = min_freq as u64;
            self.max_frequency = max_freq as u64;
        }
    }

    /// Records a cache miss for LFU metrics
    ///
    /// # Arguments
    /// * `object_size` - Size of the object that was requested (in bytes)
    pub fn record_miss(&mut self, object_size: u64) {
        self.core.record_miss(object_size);
    }

    /// Updates the count of active frequency levels
    ///
    /// # Arguments
    /// * `levels` - The number of different frequency levels currently in use
    pub fn update_active_frequency_levels(&mut self, levels: u64) {
        self.active_frequency_levels = levels;
    }

    /// Calculates the average frequency of accesses
    ///
    /// # Returns
    /// Average frequency per item access, or 0.0 if no hits have occurred
    pub fn average_frequency(&self) -> f64 {
        if self.core.cache_hits > 0 {
            self.total_frequency_increments as f64 / self.core.cache_hits as f64
        } else {
            0.0
        }
    }

    /// Calculates the frequency range (max - min)
    ///
    /// # Returns
    /// The range of frequencies currently in the cache
    pub fn frequency_range(&self) -> u64 {
        self.max_frequency.saturating_sub(self.min_frequency)
    }

    /// Converts LFU metrics to a BTreeMap for reporting
    ///
    /// This method returns all metrics relevant to the LFU cache algorithm,
    /// including both core metrics and LFU-specific frequency metrics.
    ///
    /// Uses BTreeMap to ensure consistent, deterministic ordering of metrics.
    ///
    /// # Returns
    /// A BTreeMap containing all LFU cache metrics as key-value pairs
    pub fn to_btreemap(&self) -> BTreeMap<String, f64> {
        let mut metrics = self.core.to_btreemap();

        // LFU-specific metrics
        metrics.insert("min_frequency".to_string(), self.min_frequency as f64);
        metrics.insert("max_frequency".to_string(), self.max_frequency as f64);
        metrics.insert("frequency_range".to_string(), self.frequency_range() as f64);
        metrics.insert(
            "total_frequency_increments".to_string(),
            self.total_frequency_increments as f64,
        );
        metrics.insert(
            "active_frequency_levels".to_string(),
            self.active_frequency_levels as f64,
        );
        metrics.insert("average_frequency".to_string(), self.average_frequency());

        // Frequency efficiency metrics
        if self.core.requests > 0 {
            metrics.insert(
                "frequency_increment_rate".to_string(),
                self.total_frequency_increments as f64 / self.core.requests as f64,
            );
        }

        if self.active_frequency_levels > 0 && self.core.cache_hits > 0 {
            metrics.insert(
                "frequency_distribution_efficiency".to_string(),
                self.active_frequency_levels as f64 / self.core.cache_hits as f64,
            );
        }

        metrics
    }
}

impl CacheMetrics for LfuCacheMetrics {
    /// Returns all LFU cache metrics as key-value pairs in deterministic order
    ///
    /// # Returns
    /// A BTreeMap containing all metrics tracked by this LFU cache instance
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.to_btreemap()
    }

    /// Returns the algorithm name for this cache implementation
    ///
    /// # Returns
    /// "LFU" - identifying this as a Least Frequently Used cache
    fn algorithm_name(&self) -> &'static str {
        "LFU"
    }
}
