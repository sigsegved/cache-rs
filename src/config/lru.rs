//! Configuration for the Least Recently Used (LRU) cache.

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LRU (Least Recently Used) cache.
///
/// LRU evicts the least recently accessed items when the cache reaches capacity.
///
/// # Examples
///
/// ```
/// use cache_rs::config::lru::LruCacheConfig;
/// use core::num::NonZeroUsize;
///
/// // Create a config with capacity of 100 items
/// let config = LruCacheConfig::new(NonZeroUsize::new(100).unwrap());
///
/// assert_eq!(config.capacity(), NonZeroUsize::new(100).unwrap());
/// assert_eq!(config.max_size(), u64::MAX); // Default: no size limit
/// ```
#[derive(Clone, Copy)]
pub struct LruCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    capacity: NonZeroUsize,
    /// Maximum total size of cached content (sum of entry sizes)
    max_size: u64,
}

impl LruCacheConfig {
    /// Creates a new configuration for an LRU cache with no size limit.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            max_size: u64::MAX,
        }
    }

    /// Creates a new configuration with both entry limit and size limit.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    /// * `max_size` - Maximum total size of cached content
    pub fn with_max_size(capacity: NonZeroUsize, max_size: u64) -> Self {
        Self { capacity, max_size }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Returns the maximum total size of cached content.
    pub fn max_size(&self) -> u64 {
        self.max_size
    }
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
        let config = LruCacheConfig::new(NonZeroUsize::new(100).unwrap());
        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.max_size(), u64::MAX);
    }

    #[test]
    fn test_lru_config_with_max_size() {
        let config = LruCacheConfig::with_max_size(NonZeroUsize::new(100).unwrap(), 1024 * 1024);
        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.max_size(), 1024 * 1024);
    }
}
