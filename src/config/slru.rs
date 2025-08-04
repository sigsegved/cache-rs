//! Configuration for the Segmented Least Recently Used (SLRU) cache.

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an SLRU (Segmented LRU) cache.
///
/// SLRU divides the cache into two segments: a probationary segment for new entries
/// and a protected segment for frequently accessed entries.
///
/// # Examples
///
/// ```
/// use cache_rs::config::slru::SlruCacheConfig;
/// use core::num::NonZeroUsize;
///
/// // Create a config with total capacity of 4 items and protected capacity of 2 items
/// let config = SlruCacheConfig::new(
///     NonZeroUsize::new(4).unwrap(),
///     NonZeroUsize::new(2).unwrap()
/// );
///
/// assert_eq!(config.capacity(), NonZeroUsize::new(4).unwrap());
/// assert_eq!(config.protected_capacity(), NonZeroUsize::new(2).unwrap());
/// ```
#[derive(Clone, Copy)]
pub struct SlruCacheConfig {
    /// Total capacity of the cache (protected + probationary)
    capacity: NonZeroUsize,

    /// Maximum size for the protected segment
    protected_capacity: NonZeroUsize,
}

impl SlruCacheConfig {
    /// Creates a new configuration for an SLRU cache.
    ///
    /// # Arguments
    /// * `capacity` - Total number of key-value pairs the cache can hold
    /// * `protected_capacity` - Maximum size of the protected segment
    ///
    /// # Panics
    /// Panics if `protected_capacity` is greater than `capacity`
    pub fn new(capacity: NonZeroUsize, protected_capacity: NonZeroUsize) -> Self {
        assert!(
            protected_capacity.get() <= capacity.get(),
            "Protected capacity must be less than or equal to total capacity"
        );

        Self {
            capacity,
            protected_capacity,
        }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Returns the maximum size of the protected segment.
    pub fn protected_capacity(&self) -> NonZeroUsize {
        self.protected_capacity
    }
}

impl fmt::Debug for SlruCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlruCacheConfig")
            .field("capacity", &self.capacity)
            .field("protected_capacity", &self.protected_capacity)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slru_config_creation() {
        let config = SlruCacheConfig::new(
            NonZeroUsize::new(10).unwrap(),
            NonZeroUsize::new(5).unwrap(),
        );

        assert_eq!(config.capacity().get(), 10);
        assert_eq!(config.protected_capacity().get(), 5);
    }

    #[test]
    #[should_panic(expected = "Protected capacity must be less than or equal to total capacity")]
    fn test_invalid_protected_capacity() {
        // This should panic because protected capacity is greater than total capacity
        SlruCacheConfig::new(
            NonZeroUsize::new(5).unwrap(),
            NonZeroUsize::new(10).unwrap(),
        );
    }
}
