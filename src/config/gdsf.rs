//! Configuration for the Greedy Dual-Size Frequency (GDSF) cache.
//!
//! This module provides configuration for GDSF caches. Use `GdsfCacheConfig`
//! as the single entry point for creating GDSF caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::GdsfCacheConfig;
//! use cache_rs::GdsfCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple capacity-only config
//! let config = GdsfCacheConfig::new(NonZeroUsize::new(100).unwrap());
//! let cache: GdsfCache<String, Vec<u8>> = GdsfCache::from_config(config);
//!
//! // With size limit using builder pattern (recommended for GDSF)
//! let config = GdsfCacheConfig::new(NonZeroUsize::new(1000).unwrap())
//!     .with_max_size(10 * 1024 * 1024);  // 10MB
//! let cache: GdsfCache<String, Vec<u8>> = GdsfCache::from_config(config);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for a GDSF (Greedy Dual-Size Frequency) cache.
///
/// GDSF assigns a priority to each item based on the formula:
/// `Priority = (Frequency / Size) + Global_Age`
///
/// This makes it ideal for caching variable-sized objects where you want
/// to favor keeping many small popular items over few large items.
/// This is the **only** way to configure and create a GDSF cache.
///
/// # Required Parameters
///
/// - `capacity`: Maximum number of entries the cache can hold (set in constructor)
///
/// # Optional Parameters (Builder Methods)
///
/// - `max_size`: Maximum total size of cached content (default: unlimited)
/// - `initial_age`: Initial global age value (default: 0.0)
///
/// # Examples
///
/// ```
/// use cache_rs::config::GdsfCacheConfig;
/// use cache_rs::GdsfCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration with just capacity
/// let config = GdsfCacheConfig::new(NonZeroUsize::new(100).unwrap());
/// let cache: GdsfCache<String, Vec<u8>> = GdsfCache::from_config(config);
///
/// // Full configuration with size limit (recommended for GDSF)
/// let config = GdsfCacheConfig::new(NonZeroUsize::new(1000).unwrap())
///     .with_max_size(50 * 1024 * 1024)  // 50MB limit
///     .with_initial_age(0.0);
/// let cache: GdsfCache<String, Vec<u8>> = GdsfCache::from_config(config);
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
    /// Creates a new GDSF cache configuration with the specified capacity.
    ///
    /// The cache will have no size limit by default and initial age of 0.0.
    /// Use builder methods to customize.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::config::GdsfCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = GdsfCacheConfig::new(NonZeroUsize::new(100).unwrap());
    /// assert_eq!(config.capacity().get(), 100);
    /// assert_eq!(config.initial_age(), 0.0);
    /// assert_eq!(config.max_size(), u64::MAX);
    /// ```
    #[must_use]
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            initial_age: 0.0,
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
    #[must_use]
    pub fn with_max_size(mut self, max_size: u64) -> Self {
        self.max_size = max_size;
        self
    }

    /// Sets the initial global age value.
    ///
    /// This can be useful when restoring cache state or for specific
    /// algorithm tuning.
    ///
    /// # Arguments
    ///
    /// * `initial_age` - Initial global age value
    #[must_use]
    pub fn with_initial_age(mut self, initial_age: f64) -> Self {
        self.initial_age = initial_age;
        self
    }

    /// Returns the maximum number of key-value pairs the cache can hold.
    #[inline]
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Returns the initial global age value.
    #[inline]
    pub fn initial_age(&self) -> f64 {
        self.initial_age
    }

    /// Returns the maximum total size of cached content.
    #[inline]
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
    }

    #[test]
    fn test_gdsf_config_builder_pattern() {
        let config = GdsfCacheConfig::new(NonZeroUsize::new(50).unwrap())
            .with_initial_age(10.5)
            .with_max_size(1024 * 1024);
        assert_eq!(config.capacity().get(), 50);
        assert_eq!(config.initial_age(), 10.5);
        assert_eq!(config.max_size(), 1024 * 1024);
    }
}
