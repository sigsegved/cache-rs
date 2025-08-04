//! Configuration for the Least Frequently Used (LFU) cache.

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LFU (Least Frequently Used) cache.
///
/// LFU tracks the frequency of access for each item and evicts
/// the least frequently used items when the cache reaches capacity.
///
/// # Examples
///
/// ```
/// use cache_rs::config::lfu::LfuCacheConfig;
/// use core::num::NonZeroUsize;
///
/// // Create a config with capacity of 100 items
/// let config = LfuCacheConfig::new(NonZeroUsize::new(100).unwrap());
///
/// assert_eq!(config.capacity(), NonZeroUsize::new(100).unwrap());
/// ```
#[derive(Clone, Copy)]
pub struct LfuCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    capacity: NonZeroUsize,
}

impl LfuCacheConfig {
    /// Creates a new configuration for an LFU cache.
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

impl fmt::Debug for LfuCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LfuCacheConfig")
            .field("capacity", &self.capacity)
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
    }
}
