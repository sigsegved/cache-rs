//! Configuration for the Least Frequently Used with Dynamic Aging (LFUDA) cache.

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LFUDA (Least Frequently Used with Dynamic Aging) cache.
///
/// LFUDA enhances LFU by using a dynamic aging mechanism that prevents old
/// frequently-accessed items from permanently blocking new items.
///
/// # Examples
///
/// ```
/// use cache_rs::config::lfuda::LfudaCacheConfig;
/// use core::num::NonZeroUsize;
///
/// // Create a config with capacity of 100 items and initial age of 0
/// let config = LfudaCacheConfig::new(NonZeroUsize::new(100).unwrap());
///
/// assert_eq!(config.capacity(), NonZeroUsize::new(100).unwrap());
/// assert_eq!(config.initial_age(), 0);
/// ```
#[derive(Clone, Copy)]
pub struct LfudaCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    capacity: NonZeroUsize,

    /// Initial global age value
    initial_age: usize,
}

impl LfudaCacheConfig {
    /// Creates a new configuration for an LFUDA cache with initial age of 0.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            initial_age: 0,
        }
    }

    /// Creates a new configuration for an LFUDA cache with a specific initial age.
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    /// * `initial_age` - Initial global age value
    pub fn with_initial_age(capacity: NonZeroUsize, initial_age: usize) -> Self {
        Self {
            capacity,
            initial_age,
        }
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Returns the initial global age value.
    pub fn initial_age(&self) -> usize {
        self.initial_age
    }
}

impl fmt::Debug for LfudaCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LfudaCacheConfig")
            .field("capacity", &self.capacity)
            .field("initial_age", &self.initial_age)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lfuda_config_creation() {
        let config = LfudaCacheConfig::new(NonZeroUsize::new(100).unwrap());
        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.initial_age(), 0);

        let config_with_age =
            LfudaCacheConfig::with_initial_age(NonZeroUsize::new(50).unwrap(), 10);
        assert_eq!(config_with_age.capacity().get(), 50);
        assert_eq!(config_with_age.initial_age(), 10);
    }
}
