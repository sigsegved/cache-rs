//! Configuration for the Segmented Least Recently Used (SLRU) cache.
//!
//! This module provides configuration for SLRU caches.
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
//! ## SLRU-Specific Considerations
//!
//! SLRU divides the cache into **probationary** and **protected** segments.
//! A common ratio is 20% protected (hot items) and 80% probationary (new items).
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::SlruCacheConfig;
//! use cache_rs::SlruCache;
//! use core::num::NonZeroUsize;
//!
//! // In-memory cache: 50MB budget, 20% protected segment
//! let config = SlruCacheConfig {
//!     capacity: NonZeroUsize::new(10_000).unwrap(),
//!     protected_capacity: NonZeroUsize::new(2_000).unwrap(),  // 20%
//!     max_size: 50 * 1024 * 1024,  // 50MB
//! };
//! let cache: SlruCache<String, Vec<u8>> = SlruCache::init(config, None);
//!
//! // Small cache for session data
//! let config = SlruCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     protected_capacity: NonZeroUsize::new(200).unwrap(),
//!     max_size: 10 * 1024 * 1024,  // 10MB
//! };
//! let cache: SlruCache<String, i32> = SlruCache::init(config, None);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an SLRU (Segmented LRU) cache.
///
/// SLRU divides the cache into two segments: a probationary segment for new entries
/// and a protected segment for frequently accessed entries.
///
/// # Fields
///
/// - `capacity`: Total number of entries the cache can hold. Each entry has
///   memory overhead (~64-128 bytes) for keys, pointers, and metadata.
/// - `protected_capacity`: Size of the protected segment (must be < capacity).
///   Typically 20% of total capacity for hot items.
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
/// use cache_rs::config::SlruCacheConfig;
/// use cache_rs::SlruCache;
/// use core::num::NonZeroUsize;
///
/// // 10MB cache for ~1KB values, 20% protected
/// let config = SlruCacheConfig {
///     capacity: NonZeroUsize::new(10_000).unwrap(),
///     protected_capacity: NonZeroUsize::new(2_000).unwrap(),
///     max_size: 10 * 1024 * 1024,  // 10MB
/// };
/// let cache: SlruCache<String, Vec<u8>> = SlruCache::init(config, None);
///
/// // Small cache for config values, 20% protected
/// let config = SlruCacheConfig {
///     capacity: NonZeroUsize::new(500).unwrap(),
///     protected_capacity: NonZeroUsize::new(100).unwrap(),
///     max_size: 64 * 1024,  // 64KB is ample for small values
/// };
/// let cache: SlruCache<&str, i32> = SlruCache::init(config, None);
/// ```
#[derive(Clone, Copy)]
pub struct SlruCacheConfig {
    /// Total capacity of the cache (protected + probationary).
    /// Account for ~64-128 bytes overhead per entry beyond value size.
    pub capacity: NonZeroUsize,
    /// Maximum size for the protected segment (must be < capacity).
    /// Typically 20% of capacity for frequently accessed "hot" items.
    pub protected_capacity: NonZeroUsize,
    /// Maximum total size in bytes for cached values.
    /// Set based on your memory/disk budget. Avoid using `u64::MAX`.
    pub max_size: u64,
}

impl fmt::Debug for SlruCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlruCacheConfig")
            .field("capacity", &self.capacity)
            .field("protected_capacity", &self.protected_capacity)
            .field("max_size", &self.max_size)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slru_config_creation() {
        // 10MB cache with 20% protected segment
        let config = SlruCacheConfig {
            capacity: NonZeroUsize::new(1000).unwrap(),
            protected_capacity: NonZeroUsize::new(200).unwrap(),
            max_size: 10 * 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 1000);
        assert_eq!(config.protected_capacity.get(), 200);
        assert_eq!(config.max_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_slru_config_with_size_limit() {
        // 1MB cache with 20% protected segment
        let config = SlruCacheConfig {
            capacity: NonZeroUsize::new(1000).unwrap(),
            protected_capacity: NonZeroUsize::new(200).unwrap(),
            max_size: 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 1000);
        assert_eq!(config.protected_capacity.get(), 200);
        assert_eq!(config.max_size, 1024 * 1024);
    }
}
