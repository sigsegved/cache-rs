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
/// ```
#[derive(Clone, Copy)]
pub struct LruCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    capacity: NonZeroUsize,
}

impl LruCacheConfig {
    /// Creates a new configuration for an LRU cache.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self { capacity }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }
}

impl fmt::Debug for LruCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LruCacheConfig")
            .field("capacity", &self.capacity)
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
    }
}
