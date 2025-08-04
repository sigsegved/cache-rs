//! LFUDA Cache Metrics
//!
//! Metrics specific to the LFUDA (Least Frequently Used with Dynamic Aging) cache algorithm.

extern crate alloc;

use super::{CacheMetrics, CoreCacheMetrics};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

/// LFUDA-specific metrics (extends CoreCacheMetrics)
///
/// This struct contains metrics specific to the LFUDA (Least Frequently Used with Dynamic Aging)
/// cache algorithm. LFUDA combines frequency tracking with aging to prevent old frequently-used
/// items from blocking new items indefinitely.
#[derive(Debug, Clone)]
pub struct LfudaCacheMetrics {
    /// Core metrics common to all cache algorithms
    pub core: CoreCacheMetrics,

    /// Current global age value
    pub global_age: u64,

    /// Total number of aging events (when global age is increased)
    pub total_aging_events: u64,

    /// Current minimum priority in the cache
    pub min_priority: u64,

    /// Current maximum priority in the cache
    pub max_priority: u64,

    /// Total number of frequency increments (every cache hit increases frequency)
    pub total_frequency_increments: u64,

    /// Number of items that benefited from aging (had their effective priority boosted)
    pub items_benefited_from_aging: u64,

    /// Total age value distributed to items at insertion time
    pub total_age_distributed: u64,
}

impl LfudaCacheMetrics {
    /// Creates a new LfudaCacheMetrics instance with the specified maximum cache size
    ///
    /// # Arguments
    /// * `max_cache_size_bytes` - The maximum allowed cache size in bytes
    pub fn new(max_cache_size_bytes: u64) -> Self {
        Self {
            core: CoreCacheMetrics::new(max_cache_size_bytes),
            global_age: 0,
            total_aging_events: 0,
            min_priority: 0,
            max_priority: 0,
            total_frequency_increments: 0,
            items_benefited_from_aging: 0,
            total_age_distributed: 0,
        }
    }

    /// Records an aging event (when global age is increased due to eviction)
    ///
    /// # Arguments
    /// * `new_global_age` - The new global age value
    pub fn record_aging_event(&mut self, new_global_age: u64) {
        self.total_aging_events += 1;
        self.global_age = new_global_age;
    }

    /// Records a frequency increment (when an item is accessed and its frequency increases)
    ///
    /// # Arguments
    /// * `new_priority` - The new priority value for the accessed item
    pub fn record_frequency_increment(&mut self, new_priority: u64) {
        self.total_frequency_increments += 1;

        // Update min/max priority tracking
        if self.min_priority == 0 || new_priority < self.min_priority {
            self.min_priority = new_priority;
        }
        if new_priority > self.max_priority {
            self.max_priority = new_priority;
        }
    }

    /// Records when an item benefits from aging (gets age boost at insertion)
    ///
    /// # Arguments
    /// * `age_benefit` - The age value added to the item's priority
    pub fn record_aging_benefit(&mut self, age_benefit: u64) {
        self.items_benefited_from_aging += 1;
        self.total_age_distributed += age_benefit;
    }

    /// Calculates the average aging benefit per item
    ///
    /// # Returns
    /// Average age benefit per item that received aging boost, or 0.0 if none
    pub fn average_aging_benefit(&self) -> f64 {
        if self.items_benefited_from_aging > 0 {
            self.total_age_distributed as f64 / self.items_benefited_from_aging as f64
        } else {
            0.0
        }
    }

    /// Calculates the priority range (max - min)
    ///
    /// # Returns
    /// The range of priorities currently in the cache
    pub fn priority_range(&self) -> u64 {
        self.max_priority.saturating_sub(self.min_priority)
    }

    /// Calculates the aging effectiveness (aging events / evictions)
    ///
    /// # Returns
    /// How often evictions trigger aging events, or 0.0 if no evictions
    pub fn aging_effectiveness(&self) -> f64 {
        if self.core.evictions > 0 {
            self.total_aging_events as f64 / self.core.evictions as f64
        } else {
            0.0
        }
    }

    /// Calculates the frequency advantage due to aging
    ///
    /// # Returns
    /// How much of the total priority comes from aging vs raw frequency
    pub fn aging_contribution_ratio(&self) -> f64 {
        if self.total_frequency_increments > 0 && self.total_age_distributed > 0 {
            self.total_age_distributed as f64
                / (self.total_frequency_increments + self.total_age_distributed) as f64
        } else {
            0.0
        }
    }

    /// Converts LFUDA metrics to a BTreeMap for reporting
    ///
    /// This method returns all metrics relevant to the LFUDA cache algorithm,
    /// including both core metrics and LFUDA-specific aging and priority metrics.
    ///
    /// Uses BTreeMap to ensure consistent, deterministic ordering of metrics.
    ///
    /// # Returns
    /// A BTreeMap containing all LFUDA cache metrics as key-value pairs
    pub fn to_btreemap(&self) -> BTreeMap<String, f64> {
        let mut metrics = self.core.to_btreemap();

        // LFUDA-specific aging metrics
        metrics.insert("global_age".to_string(), self.global_age as f64);
        metrics.insert(
            "total_aging_events".to_string(),
            self.total_aging_events as f64,
        );
        metrics.insert(
            "aging_effectiveness".to_string(),
            self.aging_effectiveness(),
        );

        // Priority metrics
        metrics.insert("min_priority".to_string(), self.min_priority as f64);
        metrics.insert("max_priority".to_string(), self.max_priority as f64);
        metrics.insert("priority_range".to_string(), self.priority_range() as f64);

        // Frequency metrics
        metrics.insert(
            "total_frequency_increments".to_string(),
            self.total_frequency_increments as f64,
        );

        // Aging benefit metrics
        metrics.insert(
            "items_benefited_from_aging".to_string(),
            self.items_benefited_from_aging as f64,
        );
        metrics.insert(
            "total_age_distributed".to_string(),
            self.total_age_distributed as f64,
        );
        metrics.insert(
            "average_aging_benefit".to_string(),
            self.average_aging_benefit(),
        );
        metrics.insert(
            "aging_contribution_ratio".to_string(),
            self.aging_contribution_ratio(),
        );

        // Rate metrics
        if self.core.requests > 0 {
            metrics.insert(
                "aging_event_rate".to_string(),
                self.total_aging_events as f64 / self.core.requests as f64,
            );
            metrics.insert(
                "frequency_increment_rate".to_string(),
                self.total_frequency_increments as f64 / self.core.requests as f64,
            );
            metrics.insert(
                "aging_benefit_rate".to_string(),
                self.items_benefited_from_aging as f64 / self.core.requests as f64,
            );
        }

        metrics
    }
}

impl CacheMetrics for LfudaCacheMetrics {
    /// Returns all LFUDA cache metrics as key-value pairs in deterministic order
    ///
    /// # Returns
    /// A BTreeMap containing all metrics tracked by this LFUDA cache instance
    fn metrics(&self) -> BTreeMap<String, f64> {
        self.to_btreemap()
    }

    /// Returns the algorithm name for this cache implementation
    ///
    /// # Returns
    /// "LFUDA" - identifying this as a Least Frequently Used with Dynamic Aging cache
    fn algorithm_name(&self) -> &'static str {
        "LFUDA"
    }
}
