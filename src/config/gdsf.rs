//! Configuration for the Greedy Dual-Size Frequency (GDSF) cache.

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for a GDSF (Greedy Dual-Size Frequency) cache.
///
/// GDSF assigns a priority to each item based on the formula:
/// Priority = (Frequency / Size) + Global_Age
///
/// # Examples
///
/// ```
/// use cache_rs::config::gdsf::GdsfCacheConfig;
/// use core::num::NonZeroUsize;
///
/// // Create a config with capacity of 100 items and initial age of 0.0
/// let config = GdsfCacheConfig::new(NonZeroUsize::new(100).unwrap());
///
/// assert_eq!(config.capacity(), NonZeroUsize::new(100).unwrap());
/// assert_eq!(config.initial_age(), 0.0);
/// ```
#[derive(Clone, Copy)]
pub struct GdsfCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    capacity: NonZeroUsize,

    /// Initial global age value
    initial_age: f64,

    /// Maximum total size of cached content
    max_size: u64,
}

impl GdsfCacheConfig {
    /// Creates a new configuration for a GDSF cache with initial age of 0.0.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            initial_age: 0.0,
            max_size: u64::MAX,
        }
    }

    /// Creates a new configuration for a GDSF cache with a specific initial age.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    /// * `initial_age` - Initial global age value
    pub fn with_initial_age(capacity: NonZeroUsize, initial_age: f64) -> Self {
        Self {
            capacity,
            initial_age,
            max_size: u64::MAX,
        }
    }

    /// Creates a new configuration for a GDSF cache with capacity and max_size limits.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    /// * `max_size` - Maximum total size of cached content
    pub fn with_max_size(capacity: NonZeroUsize, max_size: u64) -> Self {
        Self {
            capacity,
            initial_age: 0.0,
            max_size,
        }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Returns the initial global age value.
    pub fn initial_age(&self) -> f64 {
        self.initial_age
    }

    /// Returns the maximum total size of cached content.
    pub fn max_size(&self) -> u64 {
        self.max_size
    }
}

impl fmt::Debug for GdsfCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GdsfCacheConfig")
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
    fn test_gdsf_config_creation() {
        let config = GdsfCacheConfig::new(NonZeroUsize::new(100).unwrap());
        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.initial_age(), 0.0);
        assert_eq!(config.max_size(), u64::MAX);

        let config_with_age =
            GdsfCacheConfig::with_initial_age(NonZeroUsize::new(50).unwrap(), 10.5);
        assert_eq!(config_with_age.capacity().get(), 50);
        assert_eq!(config_with_age.initial_age(), 10.5);
    }

    #[test]
    fn test_gdsf_config_with_max_size() {
        let config = GdsfCacheConfig::with_max_size(
            NonZeroUsize::new(100).unwrap(),
            1024 * 1024, // 1MB
        );
        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.max_size(), 1024 * 1024);
    }
}
