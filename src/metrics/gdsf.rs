//! GDSF Cache Metrics
//!
//! Metrics specific to the GDSF (Greedy Dual-Size Frequency) cache algorithm.

extern crate alloc;

use super::{CacheMetrics, CoreCacheMetrics};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

/// GDSF-specific metrics (extends CoreCacheMetrics)
///
/// This struct contains metrics specific to the GDSF (Greedy Dual-Size Frequency)
/// cache algorithm. GDSF combines frequency, size, and aging using the formula:
/// Priority = (Frequency / Size) + Global_Age
#[derive(Debug, Clone)]
pub struct GdsfCacheMetrics {
    /// Core metrics common to all cache algorithms
    pub core: CoreCacheMetrics,

    /// Current global age value
    pub global_age: f64,

    /// Total number of aging events (when global age is increased)
    pub total_aging_events: u64,

    /// Current minimum priority in the cache
    pub min_priority: f64,

    /// Current maximum priority in the cache
    pub max_priority: f64,

    /// Total frequency of all items (cumulative)
    pub total_frequency: u64,

    /// Total size of all items ever processed (for size-frequency analysis)
    pub total_item_size_processed: u64,

    /// Number of small items (below average size) that were cached
    pub small_items_cached: u64,

    /// Number of large items (above average size) that were cached
    pub large_items_cached: u64,

    /// Total number of size-based evictions (items evicted due to poor size/frequency ratio)
    pub size_based_evictions: u64,

    /// Sum of all frequency/size ratios for efficiency analysis
    pub total_frequency_size_ratio: f64,
}

impl GdsfCacheMetrics {
    /// Creates a new GdsfCacheMetrics instance with the specified maximum cache size
    ///
    /// # Arguments
    /// * `max_cache_size_bytes` - The maximum allowed cache size in bytes
    pub fn new(max_cache_size_bytes: u64) -> Self {
        Self {
            core: CoreCacheMetrics::new(max_cache_size_bytes),
            global_age: 0.0,
            total_aging_events: 0,
            min_priority: 0.0,
            max_priority: 0.0,
            total_frequency: 0,
            total_item_size_processed: 0,
            small_items_cached: 0,
            large_items_cached: 0,
            size_based_evictions: 0,
            total_frequency_size_ratio: 0.0,
        }
    }

    /// Records an aging event (when global age is increased due to eviction)
    ///
    /// # Arguments
    /// * `new_global_age` - The new global age value
    pub fn record_aging_event(&mut self, new_global_age: f64) {
        self.total_aging_events += 1;
        self.global_age = new_global_age;
    }

    /// Records processing of an item (frequency increment and size tracking)
    ///
    /// # Arguments
    /// * `frequency` - Current frequency of the item
    /// * `size` - Size of the item in bytes
    /// * `priority` - Calculated priority of the item
    pub fn record_item_access(&mut self, frequency: u64, size: u64, priority: f64) {
        self.total_frequency += frequency;
        self.total_item_size_processed += size;

        if size > 0 {
            let freq_size_ratio = frequency as f64 / size as f64;
            self.total_frequency_size_ratio += freq_size_ratio;
        }

        // Update min/max priority tracking
        if self.min_priority == 0.0 || priority < self.min_priority {
            self.min_priority = priority;
        }
        if priority > self.max_priority {
            self.max_priority = priority;
        }
    }

    /// Records caching of an item with size classification
    ///
    /// # Arguments
    /// * `size` - Size of the cached item in bytes
    /// * `average_size` - Current average item size for classification
    pub fn record_item_cached(&mut self, size: u64, average_size: f64) {
        if size as f64 <= average_size {
            self.small_items_cached += 1;
        } else {
            self.large_items_cached += 1;
        }
    }

    /// Records a size-based eviction
    pub fn record_size_based_eviction(&mut self) {
        self.size_based_evictions += 1;
    }

    /// Calculates the average frequency across all processed items
    ///
    /// # Returns
    /// Average frequency per item access, or 0.0 if no items processed
    pub fn average_frequency(&self) -> f64 {
        if self.core.requests > 0 {
            self.total_frequency as f64 / self.core.requests as f64
        } else {
            0.0
        }
    }

    /// Calculates the average size of processed items
    ///
    /// # Returns
    /// Average item size in bytes, or 0.0 if no items processed
    pub fn average_item_size(&self) -> f64 {
        if self.core.requests > 0 {
            self.total_item_size_processed as f64 / self.core.requests as f64
        } else {
            0.0
        }
    }

    /// Calculates the average frequency-to-size ratio
    ///
    /// # Returns
    /// Average frequency/size ratio, or 0.0 if no items processed
    pub fn average_frequency_size_ratio(&self) -> f64 {
        if self.core.requests > 0 {
            self.total_frequency_size_ratio / self.core.requests as f64
        } else {
            0.0
        }
    }

    /// Calculates the priority range (max - min)
    ///
    /// # Returns
    /// The range of priorities currently in the cache
    pub fn priority_range(&self) -> f64 {
        if self.max_priority >= self.min_priority {
            self.max_priority - self.min_priority
        } else {
            0.0
        }
    }

    /// Calculates size distribution balance (small vs large items)
    ///
    /// # Returns
    /// Ratio of small items to total cached items, or 0.0 if no items cached
    pub fn size_distribution_balance(&self) -> f64 {
        let total_cached = self.small_items_cached + self.large_items_cached;
        if total_cached > 0 {
            self.small_items_cached as f64 / total_cached as f64
        } else {
            0.0
        }
    }

    /// Calculates size-based eviction efficiency
    ///
    /// # Returns
    /// Ratio of size-based evictions to total evictions
    pub fn size_eviction_efficiency(&self) -> f64 {
        if self.core.evictions > 0 {
            self.size_based_evictions as f64 / self.core.evictions as f64
        } else {
            0.0
        }
    }

    /// Converts GDSF metrics to a BTreeMap for reporting
    ///
    /// This method returns all metrics relevant to the GDSF cache algorithm,
    /// including both core metrics and GDSF-specific size-frequency metrics.
    ///
    /// Uses BTreeMap to ensure consistent, deterministic ordering of metrics.
    ///
    /// # Returns
    /// A BTreeMap containing all GDSF cache metrics as key-value pairs
    pub fn to_btreemap(&self) -> BTreeMap<String, f64> {
        let mut metrics = self.core.to_btreemap();

        // GDSF-specific aging metrics
        metrics.insert("global_age".to_string(), self.global_age);
        metrics.insert(
            "total_aging_events".to_string(),
            self.total_aging_events as f64,
        );

        // Priority metrics
        metrics.insert("min_priority".to_string(), self.min_priority);
        metrics.insert("max_priority".to_string(), self.max_priority);
        metrics.insert("priority_range".to_string(), self.priority_range());

        // Frequency metrics
        metrics.insert("total_frequency".to_string(), self.total_frequency as f64);
        metrics.insert("average_frequency".to_string(), self.average_frequency());

        // Size metrics
        metrics.insert(
            "total_item_size_processed".to_string(),
            self.total_item_size_processed as f64,
        );
        metrics.insert("average_item_size".to_string(), self.average_item_size());
        metrics.insert(
            "small_items_cached".to_string(),
            self.small_items_cached as f64,
        );
        metrics.insert(
            "large_items_cached".to_string(),
            self.large_items_cached as f64,
        );
        metrics.insert(
            "size_distribution_balance".to_string(),
            self.size_distribution_balance(),
        );

        // Size-frequency efficiency metrics
        metrics.insert(
            "total_frequency_size_ratio".to_string(),
            self.total_frequency_size_ratio,
        );
        metrics.insert(
            "average_frequency_size_ratio".to_string(),
            self.average_frequency_size_ratio(),
        );
        metrics.insert(
            "size_based_evictions".to_string(),
            self.size_based_evictions as f64,
        );
        metrics.insert(
            "size_eviction_efficiency".to_string(),
            self.size_eviction_efficiency(),
        );

        // Rate metrics
        if self.core.requests > 0 {
            metrics.insert(
                "aging_event_rate".to_string(),
                self.total_aging_events as f64 / self.core.requests as f64,
            );
            metrics.insert(
                "size_based_eviction_rate".to_string(),
                self.size_based_evictions as f64 / self.core.requests as f64,
            );
        }

        metrics
    }
}

impl CacheMetrics for GdsfCacheMetrics {
    /// Returns all GDSF cache metrics as key-value pairs in deterministic order
    ///
    /// # Returns
    /// A BTreeMap containing all metrics tracked by this GDSF cache instance
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.to_btreemap()
    }

    /// Returns the algorithm name for this cache implementation
    ///
    /// # Returns
    /// "GDSF" - identifying this as a Greedy Dual-Size Frequency cache
    fn algorithm_name(&self) -> &'static str {
        "GDSF"
    }
}
