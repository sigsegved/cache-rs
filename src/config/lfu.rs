//! Configuration for the Least Frequently Used (LFU) cache.
//!
//! This module provides configuration for LFU caches. Use `LfuCacheConfig`
//! as the single entry point for creating LFU caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::LfuCacheConfig;
//! use cache_rs::LfuCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple capacity-only config
//! let config = LfuCacheConfig::new(NonZeroUsize::new(100).unwrap());
//! let cache: LfuCache<String, i32> = LfuCache::from_config(config);
//!
//! // With size limit using builder pattern
//! let config = LfuCacheConfig::new(NonZeroUsize::new(1000).unwrap())
//!     .with_max_size(10 * 1024 * 1024);  // 10MB
//! let cache: LfuCache<String, Vec<u8>> = LfuCache::from_config(config);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LFU (Least Frequently Used) cache.
///
/// LFU tracks the frequency of access for each item and evicts
/// the least frequently used items when the cache reaches capacity.
/// This is the **only** way to configure and create an LFU cache.
///
/// # Required Parameters
///
/// - `capacity`: Maximum number of entries the cache can hold (set in constructor)
///
/// # Optional Parameters (Builder Methods)
///
/// - `max_size`: Maximum total size of cached content (default: unlimited)
///
/// # Examples
///
/// ```
/// use cache_rs::config::LfuCacheConfig;
/// use cache_rs::LfuCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration with just capacity
/// let config = LfuCacheConfig::new(NonZeroUsize::new(100).unwrap());
/// let cache: LfuCache<&str, i32> = LfuCache::from_config(config);
///
/// // Full configuration with size limit
/// let config = LfuCacheConfig::new(NonZeroUsize::new(1000).unwrap())
///     .with_max_size(50 * 1024 * 1024);  // 50MB limit
/// let cache: LfuCache<String, Vec<u8>> = LfuCache::from_config(config);
/// ```
#[derive(Clone, Copy)]
pub struct LfuCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    capacity: NonZeroUsize,
    /// Maximum total size of cached content (sum of entry sizes)
    max_size: u64,
}

impl LfuCacheConfig {
    /// Creates a new LFU cache configuration with the specified capacity.
    ///
    /// The cache will have no size limit by default (only entry count limit).
    /// Use [`with_max_size`](Self::with_max_size) to add a size limit.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::config::LfuCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfuCacheConfig::new(NonZeroUsize::new(100).unwrap());
    /// assert_eq!(config.capacity().get(), 100);
    /// assert_eq!(config.max_size(), u64::MAX);  // No size limit
    /// ```
    #[must_use]
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            max_size: u64::MAX,
        }
    }

    /// Sets the maximum total size of cached content.
    ///
    /// When both entry count and size limits are set, the cache evicts
    /// entries when **either** limit would be exceeded.
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum total size in bytes (or your chosen unit)
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::config::LfuCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfuCacheConfig::new(NonZeroUsize::new(1000).unwrap())
    ///     .with_max_size(10 * 1024 * 1024);  // 10MB
    ///
    /// assert_eq!(config.max_size(), 10 * 1024 * 1024);
    /// ```
    #[must_use]
    pub fn with_max_size(mut self, max_size: u64) -> Self {
        self.max_size = max_size;
        self
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    #[inline]
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Returns the maximum total size of cached content.
    #[inline]
    pub fn max_size(&self) -> u64 {
        self.max_size
    }
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
        let config = LfuCacheConfig::new(NonZeroUsize::new(100).unwrap());
        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.max_size(), u64::MAX);
    }

    #[test]
    fn test_lfu_config_builder_pattern() {
        let config =
            LfuCacheConfig::new(NonZeroUsize::new(100).unwrap()).with_max_size(1024 * 1024);
        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.max_size(), 1024 * 1024);
    }
}
