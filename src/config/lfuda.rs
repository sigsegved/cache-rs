//! Configuration for the Least Frequently Used with Dynamic Aging (LFUDA) cache.
//!
//! This module provides configuration for LFUDA caches.
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
//! # Examples
//!
//! ```
//! use cache_rs::config::LfudaCacheConfig;
//! use cache_rs::LfudaCache;
//! use core::num::NonZeroUsize;
//!
//! // In-memory cache: 50MB budget for values, ~5KB average size
//! let config = LfudaCacheConfig {
//!     capacity: NonZeroUsize::new(10_000).unwrap(),
//!     initial_age: 0,
//!     max_size: 50 * 1024 * 1024,  // 50MB
//! };
//! let cache: LfudaCache<String, Vec<u8>> = LfudaCache::init(config, None);
//!
//! // Small fixed-size value cache with initial age
//! let config = LfudaCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     initial_age: 100,
//!     max_size: 1024 * 1024,  // 1MB is plenty for small values
//! };
//! let cache: LfudaCache<String, i32> = LfudaCache::init(config, None);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LFUDA (Least Frequently Used with Dynamic Aging) cache.
///
/// LFUDA enhances LFU by using a dynamic aging mechanism that prevents old
/// frequently-accessed items from permanently blocking new items.
///
/// # Fields
///
/// - `capacity`: Maximum number of entries the cache can hold. Each entry has
///   memory overhead (~64-128 bytes) for keys, pointers, and metadata.
/// - `initial_age`: Initial global age value (default: 0)
/// - `max_size`: Maximum total size in bytes for cached values. Set this based
///   on your memory budget, not to `u64::MAX`. See module docs for sizing guidance.
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
/// use cache_rs::config::LfudaCacheConfig;
/// use cache_rs::LfudaCache;
/// use core::num::NonZeroUsize;
///
/// // 10MB cache for ~1KB average values → ~10,000 entries
/// let config = LfudaCacheConfig {
///     capacity: NonZeroUsize::new(10_000).unwrap(),
///     initial_age: 0,
///     max_size: 10 * 1024 * 1024,  // 10MB
/// };
/// let cache: LfudaCache<String, Vec<u8>> = LfudaCache::init(config, None);
///
/// // Small cache for tiny values with initial age
/// let config = LfudaCacheConfig {
///     capacity: NonZeroUsize::new(500).unwrap(),
///     initial_age: 100,
///     max_size: 64 * 1024,  // 64KB is ample for small values
/// };
/// let cache: LfudaCache<&str, i32> = LfudaCache::init(config, None);
/// ```
#[derive(Clone, Copy)]
pub struct LfudaCacheConfig {
    /// Maximum number of key-value pairs the cache can hold.
    /// Account for ~64-128 bytes overhead per entry beyond value size.
    pub capacity: NonZeroUsize,
    /// Initial global age value
    pub initial_age: usize,
    /// Maximum total size in bytes for cached values.
    /// Set based on your memory/disk budget. Avoid using `u64::MAX`.
    pub max_size: u64,
}

impl fmt::Debug for LfudaCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LfudaCacheConfig")
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
    fn test_lfuda_config_creation() {
        // 10MB cache with ~10KB average values
        let config = LfudaCacheConfig {
            capacity: NonZeroUsize::new(1000).unwrap(),
            initial_age: 0,
            max_size: 10 * 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 1000);
        assert_eq!(config.initial_age, 0);
        assert_eq!(config.max_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_lfuda_config_with_initial_age() {
        // 1MB cache with initial age
        let config = LfudaCacheConfig {
            capacity: NonZeroUsize::new(500).unwrap(),
            initial_age: 10,
            max_size: 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 500);
        assert_eq!(config.initial_age, 10);
        assert_eq!(config.max_size, 1024 * 1024);
    }
}
