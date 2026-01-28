//! Configuration for the Least Frequently Used (LFU) cache.
//!
//! This module provides configuration for LFU caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::LfuCacheConfig;
//! use cache_rs::LfuCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple capacity-only config
//! let config = LfuCacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     max_size: u64::MAX,
//! };
//! let cache: LfuCache<String, i32> = LfuCache::init(config, None);
//!
//! // With size limit
//! let config = LfuCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     max_size: 10 * 1024 * 1024,  // 10MB
//! };
//! let cache: LfuCache<String, Vec<u8>> = LfuCache::init(config, None);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LFU (Least Frequently Used) cache.
///
/// LFU tracks the frequency of access for each item and evicts
/// the least frequently used items when the cache reaches capacity.
///
/// # Fields
///
/// - `capacity`: Maximum number of entries the cache can hold
/// - `max_size`: Maximum total size of cached content (use `u64::MAX` for unlimited)
///
/// # Examples
///
/// ```
/// use cache_rs::config::LfuCacheConfig;
/// use cache_rs::LfuCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration with just capacity
/// let config = LfuCacheConfig {
///     capacity: NonZeroUsize::new(100).unwrap(),
///     max_size: u64::MAX,
/// };
/// let cache: LfuCache<&str, i32> = LfuCache::init(config, None);
///
/// // Full configuration with size limit
/// let config = LfuCacheConfig {
///     capacity: NonZeroUsize::new(1000).unwrap(),
///     max_size: 50 * 1024 * 1024,  // 50MB limit
/// };
/// let cache: LfuCache<String, Vec<u8>> = LfuCache::init(config, None);
/// ```
#[derive(Clone, Copy)]
pub struct LfuCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    pub capacity: NonZeroUsize,
    /// Maximum total size of cached content (sum of entry sizes).
    /// Use `u64::MAX` for no size limit.
    pub max_size: u64,
}

impl fmt::Debug for LfuCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LfuCacheConfig")
            .field("capacity", &self.capacity)
            .field("max_size", &self.max_size)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lfu_config_creation() {
        let config = LfuCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            max_size: u64::MAX,
        };
        assert_eq!(config.capacity.get(), 100);
        assert_eq!(config.max_size, u64::MAX);
    }

    #[test]
    fn test_lfu_config_with_size_limit() {
        let config = LfuCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            max_size: 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 100);
        assert_eq!(config.max_size, 1024 * 1024);
    }
}
