//! Configuration for the Segmented Least Recently Used (SLRU) cache.
//!
//! This module provides configuration for SLRU caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::SlruCacheConfig;
//! use cache_rs::SlruCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple config with capacity and protected ratio
//! let config = SlruCacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     protected_capacity: NonZeroUsize::new(20).unwrap(),
//!     max_size: u64::MAX,
//! };
//! let cache: SlruCache<String, i32> = SlruCache::init(config, None);
//!
//! // With size limit
//! let config = SlruCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     protected_capacity: NonZeroUsize::new(200).unwrap(),
//!     max_size: 10 * 1024 * 1024,  // 10MB
//! };
//! let cache: SlruCache<String, Vec<u8>> = SlruCache::init(config, None);
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
/// - `capacity`: Total number of entries the cache can hold
/// - `protected_capacity`: Size of the protected segment (must be <= capacity)
/// - `max_size`: Maximum total size of cached content (use `u64::MAX` for unlimited)
///
/// # Examples
///
/// ```
/// use cache_rs::config::SlruCacheConfig;
/// use cache_rs::SlruCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration
/// let config = SlruCacheConfig {
///     capacity: NonZeroUsize::new(100).unwrap(),   // Total capacity
///     protected_capacity: NonZeroUsize::new(20).unwrap(),    // Protected segment (20%)
///     max_size: u64::MAX,
/// };
/// let cache: SlruCache<&str, i32> = SlruCache::init(config, None);
///
/// // With size limit
/// let config = SlruCacheConfig {
///     capacity: NonZeroUsize::new(1000).unwrap(),
///     protected_capacity: NonZeroUsize::new(200).unwrap(),
///     max_size: 50 * 1024 * 1024,  // 50MB limit
/// };
/// let cache: SlruCache<String, Vec<u8>> = SlruCache::init(config, None);
/// ```
#[derive(Clone, Copy)]
pub struct SlruCacheConfig {
    /// Total capacity of the cache (protected + probationary)
    pub capacity: NonZeroUsize,
    /// Maximum size for the protected segment (must be <= capacity)
    pub protected_capacity: NonZeroUsize,
    /// Maximum total size of cached content (sum of entry sizes).
    /// Use `u64::MAX` for no size limit.
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
        let config = SlruCacheConfig {
            capacity: NonZeroUsize::new(10).unwrap(),
            protected_capacity: NonZeroUsize::new(5).unwrap(),
            max_size: u64::MAX,
        };
        assert_eq!(config.capacity.get(), 10);
        assert_eq!(config.protected_capacity.get(), 5);
        assert_eq!(config.max_size, u64::MAX);
    }

    #[test]
    fn test_slru_config_with_size_limit() {
        let config = SlruCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            protected_capacity: NonZeroUsize::new(20).unwrap(),
            max_size: 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 100);
        assert_eq!(config.protected_capacity.get(), 20);
        assert_eq!(config.max_size, 1024 * 1024);
    }
}
