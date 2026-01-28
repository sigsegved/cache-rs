//! Configuration for the Least Frequently Used with Dynamic Aging (LFUDA) cache.
//!
//! This module provides configuration for LFUDA caches. Use `LfudaCacheConfig`
//! as the single entry point for creating LFUDA caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::LfudaCacheConfig;
//! use cache_rs::LfudaCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple capacity-only config
//! let config = LfudaCacheConfig::new(NonZeroUsize::new(100).unwrap());
//! let cache: LfudaCache<String, i32> = LfudaCache::from_config(config);
//!
//! // With size limit using builder pattern
//! let config = LfudaCacheConfig::new(NonZeroUsize::new(1000).unwrap())
//!     .with_max_size(10 * 1024 * 1024);  // 10MB
//! let cache: LfudaCache<String, Vec<u8>> = LfudaCache::from_config(config);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LFUDA (Least Frequently Used with Dynamic Aging) cache.
///
/// LFUDA enhances LFU by using a dynamic aging mechanism that prevents old
/// frequently-accessed items from permanently blocking new items.
/// This is the **only** way to configure and create an LFUDA cache.
///
/// # Required Parameters
///
/// - `capacity`: Maximum number of entries the cache can hold (set in constructor)
///
/// # Optional Parameters (Builder Methods)
///
/// - `max_size`: Maximum total size of cached content (default: unlimited)
/// - `initial_age`: Initial global age value (default: 0)
///
/// # Examples
///
/// ```
/// use cache_rs::config::LfudaCacheConfig;
/// use cache_rs::LfudaCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration with just capacity
/// let config = LfudaCacheConfig::new(NonZeroUsize::new(100).unwrap());
/// let cache: LfudaCache<&str, i32> = LfudaCache::from_config(config);
///
/// // Full configuration with size limit and initial age
/// let config = LfudaCacheConfig::new(NonZeroUsize::new(1000).unwrap())
///     .with_max_size(50 * 1024 * 1024)  // 50MB limit
///     .with_initial_age(100);
/// let cache: LfudaCache<String, Vec<u8>> = LfudaCache::from_config(config);
/// ```
#[derive(Clone, Copy)]
pub struct LfudaCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    capacity: NonZeroUsize,
    /// Initial global age value
    initial_age: usize,
    /// Maximum total size of cached content (sum of entry sizes)
    max_size: u64,
}

impl LfudaCacheConfig {
    /// Creates a new LFUDA cache configuration with the specified capacity.
    ///
    /// The cache will have no size limit by default and initial age of 0.
    /// Use builder methods to customize.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Maximum number of key-value pairs the cache can hold
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::config::LfudaCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = LfudaCacheConfig::new(NonZeroUsize::new(100).unwrap());
    /// assert_eq!(config.capacity().get(), 100);
    /// assert_eq!(config.max_size(), u64::MAX);
    /// assert_eq!(config.initial_age(), 0);
    /// ```
    #[must_use]
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            initial_age: 0,
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
    pub fn with_initial_age(mut self, initial_age: usize) -> Self {
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
    pub fn initial_age(&self) -> usize {
        self.initial_age
    }

    /// Returns the maximum total size of cached content.
    #[inline]
    pub fn max_size(&self) -> u64 {
        self.max_size
    }
}

impl fmt::Debug for LfudaCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LfudaCacheConfig")
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
    fn test_lfuda_config_creation() {
        let config = LfudaCacheConfig::new(NonZeroUsize::new(100).unwrap());
        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.initial_age(), 0);
        assert_eq!(config.max_size(), u64::MAX);
    }

    #[test]
    fn test_lfuda_config_builder_pattern() {
        let config = LfudaCacheConfig::new(NonZeroUsize::new(50).unwrap())
            .with_initial_age(10)
            .with_max_size(1024 * 1024);
        assert_eq!(config.capacity().get(), 50);
        assert_eq!(config.initial_age(), 10);
        assert_eq!(config.max_size(), 1024 * 1024);
    }
}
