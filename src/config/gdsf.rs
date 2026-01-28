//! Configuration for the Greedy Dual-Size Frequency (GDSF) cache.
//!
//! This module provides configuration for GDSF caches. GDSF is particularly
//! well-suited for variable-sized objects as it considers object size in
//! eviction decisions.
//!
//! # Sizing Guidelines
//!
//! ## Understanding `max_size` and `capacity`
//!
//! - **`max_size`**: The maximum total size in bytes for cached *values*. This should reflect
//!   your memory budget for the cache data itself.
//! - **`capacity`**: The maximum number of entries. Each entry has memory overhead beyond
//!   the value size (approximately 64-128 bytes per entry for keys, pointers, and metadata).
//!
//! ## For In-Memory Caches
//!
//! Set `max_size` to the amount of memory you want to allocate for cached values:
//!
//! ```text
//! Total Memory ≈ max_size + (capacity × overhead_per_entry)
//! overhead_per_entry ≈ 64-128 bytes (keys, pointers, metadata)
//! ```
//!
//! **Example**: For a 100MB cache with ~10KB average values:
//! - `max_size = 100 * 1024 * 1024` (100MB for values)
//! - `capacity = 10_000` entries
//! - Overhead ≈ 10,000 × 100 bytes = ~1MB additional
//!
//! ## For Disk-Based or External Caches
//!
//! When caching references to external storage, size based on your target cache size:
//!
//! ```text
//! capacity = target_cache_size / average_object_size
//! ```
//!
//! **Example**: For a 1GB disk cache with 50KB average objects:
//! - `max_size = 1024 * 1024 * 1024` (1GB)
//! - `capacity = 1GB / 50KB ≈ 20,000` entries
//!
//! ## GDSF-Specific Considerations
//!
//! GDSF is ideal for **variable-sized objects** because it balances frequency
//! against size. For GDSF caches, `max_size` is especially important since the
//! algorithm optimizes for keeping many small popular items over few large ones.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::GdsfCacheConfig;
//! use cache_rs::GdsfCache;
//! use core::num::NonZeroUsize;
//!
//! // In-memory cache: 50MB budget for variable-sized objects
//! let config = GdsfCacheConfig {
//!     capacity: NonZeroUsize::new(10_000).unwrap(),
//!     initial_age: 0.0,
//!     max_size: 50 * 1024 * 1024,  // 50MB
//! };
//! let cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
//!
//! // Disk cache: 1GB for web assets (images, JS, CSS)
//! let config = GdsfCacheConfig {
//!     capacity: NonZeroUsize::new(20_000).unwrap(),  // ~50KB avg
//!     initial_age: 0.0,
//!     max_size: 1024 * 1024 * 1024,  // 1GB
//! };
//! let cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for a GDSF (Greedy Dual-Size Frequency) cache.
///
/// GDSF assigns a priority to each item based on the formula:
/// `Priority = (Frequency / Size) + Global_Age`
///
/// This makes it ideal for caching variable-sized objects where you want
/// to favor keeping many small popular items over few large items.
///
/// # Fields
///
/// - `capacity`: Maximum number of entries the cache can hold. Each entry has
///   memory overhead (~64-128 bytes) for keys, pointers, and metadata.
/// - `initial_age`: Initial global age value (default: 0.0)
/// - `max_size`: Maximum total size in bytes for cached values. **Essential for GDSF**
///   since the algorithm optimizes based on object sizes. See module docs for guidance.
///
/// # Sizing Recommendations
///
/// Always set meaningful values for both fields:
///
/// - **In-memory cache**: `max_size` = memory budget for values;
///   `capacity` = `max_size` / average_value_size
/// - **Disk-based cache**: `max_size` = disk space allocation;
///   `capacity` = `max_size` / average_object_size
///
/// # Examples
///
/// ```
/// use cache_rs::config::GdsfCacheConfig;
/// use cache_rs::GdsfCache;
/// use core::num::NonZeroUsize;
///
/// // 10MB cache for variable-sized web responses (~2KB avg)
/// let config = GdsfCacheConfig {
///     capacity: NonZeroUsize::new(5_000).unwrap(),
///     initial_age: 0.0,
///     max_size: 10 * 1024 * 1024,  // 10MB
/// };
/// let cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
///
/// // 100MB image cache with ~20KB average size
/// let config = GdsfCacheConfig {
///     capacity: NonZeroUsize::new(5_000).unwrap(),
///     initial_age: 0.0,
///     max_size: 100 * 1024 * 1024,  // 100MB
/// };
/// let cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
/// ```
#[derive(Clone, Copy)]
pub struct GdsfCacheConfig {
    /// Maximum number of key-value pairs the cache can hold.
    /// Account for ~64-128 bytes overhead per entry beyond value size.
    pub capacity: NonZeroUsize,
    /// Initial global age value
    pub initial_age: f64,
    /// Maximum total size in bytes for cached values.
    /// Set based on your memory/disk budget. Avoid using `u64::MAX`.
    pub max_size: u64,
}

impl fmt::Debug for GdsfCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GdsfCacheConfig")
            .field("capacity", &self.capacity)
            .field("initial_age", &self.initial_age)
            .field("max_size", &self.max_size)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gdsf_config_creation() {
        // 10MB cache for variable-sized objects
        let config = GdsfCacheConfig {
            capacity: NonZeroUsize::new(1000).unwrap(),
            initial_age: 0.0,
            max_size: 10 * 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 1000);
        assert_eq!(config.initial_age, 0.0);
        assert_eq!(config.max_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_gdsf_config_with_initial_age() {
        // 1MB cache with initial age
        let config = GdsfCacheConfig {
            capacity: NonZeroUsize::new(500).unwrap(),
            initial_age: 10.5,
            max_size: 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 500);
        assert_eq!(config.initial_age, 10.5);
        assert_eq!(config.max_size, 1024 * 1024);
    }
}
