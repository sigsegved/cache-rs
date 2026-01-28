//! Configuration for the Least Recently Used (LRU) cache.
//!
//! This module provides configuration for LRU caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::LruCacheConfig;
//! use cache_rs::LruCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple capacity-only config
//! let config = LruCacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     max_size: u64::MAX,
//! };
//! let cache: LruCache<String, i32> = LruCache::init(config, None);
//!
//! // With size limit
//! let config = LruCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     max_size: 10 * 1024 * 1024,  // 10MB
//! };
//! let cache: LruCache<String, Vec<u8>> = LruCache::init(config, None);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LRU (Least Recently Used) cache.
///
/// LRU evicts the least recently accessed items when the cache reaches capacity.
///
/// # Fields
///
/// - `capacity`: Maximum number of entries the cache can hold
/// - `max_size`: Maximum total size of cached content (use `u64::MAX` for unlimited)
///
/// # Examples
///
/// ```
/// use cache_rs::config::LruCacheConfig;
/// use cache_rs::LruCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration with just capacity
/// let config = LruCacheConfig {
///     capacity: NonZeroUsize::new(100).unwrap(),
///     max_size: u64::MAX,
/// };
/// let cache: LruCache<&str, i32> = LruCache::init(config, None);
///
/// // Full configuration with size limit
/// let config = LruCacheConfig {
///     capacity: NonZeroUsize::new(1000).unwrap(),
///     max_size: 50 * 1024 * 1024,  // 50MB limit
/// };
/// let cache: LruCache<String, Vec<u8>> = LruCache::init(config, None);
/// ```
#[derive(Clone, Copy)]
pub struct LruCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    pub capacity: NonZeroUsize,
    /// Maximum total size of cached content (sum of entry sizes).
    /// Use `u64::MAX` for no size limit.
    pub max_size: u64,
}

impl fmt::Debug for LruCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LruCacheConfig")
            .field("capacity", &self.capacity)
            .field("max_size", &self.max_size)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lru_config_creation() {
        let config = LruCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            max_size: u64::MAX,
        };
        assert_eq!(config.capacity.get(), 100);
        assert_eq!(config.max_size, u64::MAX);
    }

    #[test]
    fn test_lru_config_with_size_limit() {
        let config = LruCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            max_size: 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 100);
        assert_eq!(config.max_size, 1024 * 1024);
    }
}
