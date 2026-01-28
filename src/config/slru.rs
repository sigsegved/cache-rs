//! Configuration for the Segmented Least Recently Used (SLRU) cache.
//!
//! This module provides configuration for SLRU caches. Use `SlruCacheConfig`
//! as the single entry point for creating SLRU caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::SlruCacheConfig;
//! use cache_rs::SlruCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple config with capacity and protected ratio
//! let config = SlruCacheConfig::new(
//!     NonZeroUsize::new(100).unwrap(),
//!     NonZeroUsize::new(20).unwrap(),
//! );
//! let cache: SlruCache<String, i32> = SlruCache::from_config(config);
//!
//! // With size limit using builder pattern
//! let config = SlruCacheConfig::new(
//!     NonZeroUsize::new(1000).unwrap(),
//!     NonZeroUsize::new(200).unwrap(),
//! ).with_max_size(10 * 1024 * 1024);  // 10MB
//! let cache: SlruCache<String, Vec<u8>> = SlruCache::from_config(config);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an SLRU (Segmented LRU) cache.
///
/// SLRU divides the cache into two segments: a probationary segment for new entries
/// and a protected segment for frequently accessed entries.
/// This is the **only** way to configure and create an SLRU cache.
///
/// # Required Parameters
///
/// - `capacity`: Total number of entries the cache can hold (set in constructor)
/// - `protected_capacity`: Size of the protected segment (set in constructor)
///
/// # Optional Parameters (Builder Methods)
///
/// - `max_size`: Maximum total size of cached content (default: unlimited)
///
/// # Examples
///
/// ```
/// use cache_rs::config::SlruCacheConfig;
/// use cache_rs::SlruCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration
/// let config = SlruCacheConfig::new(
///     NonZeroUsize::new(100).unwrap(),   // Total capacity
///     NonZeroUsize::new(20).unwrap(),    // Protected segment (20%)
/// );
/// let cache: SlruCache<&str, i32> = SlruCache::from_config(config);
///
/// // With size limit
/// let config = SlruCacheConfig::new(
///     NonZeroUsize::new(1000).unwrap(),
///     NonZeroUsize::new(200).unwrap(),
/// ).with_max_size(50 * 1024 * 1024);  // 50MB limit
/// let cache: SlruCache<String, Vec<u8>> = SlruCache::from_config(config);
/// ```
#[derive(Clone, Copy)]
pub struct SlruCacheConfig {
    /// Total capacity of the cache (protected + probationary)
    capacity: NonZeroUsize,
    /// Maximum size for the protected segment
    protected_capacity: NonZeroUsize,
    /// Maximum total size of cached content (sum of entry sizes)
    max_size: u64,
}

impl SlruCacheConfig {
    /// Creates a new SLRU cache configuration.
    ///
    /// The cache will have no size limit by default.
    /// Use [`with_max_size`](Self::with_max_size) to add a size limit.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Total number of key-value pairs the cache can hold
    /// * `protected_capacity` - Maximum size of the protected segment
    ///
    /// # Panics
    ///
    /// Panics if `protected_capacity` is greater than `capacity`.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::config::SlruCacheConfig;
    /// use core::num::NonZeroUsize;
    ///
    /// let config = SlruCacheConfig::new(
    ///     NonZeroUsize::new(100).unwrap(),
    ///     NonZeroUsize::new(20).unwrap(),
    /// );
    /// assert_eq!(config.capacity().get(), 100);
    /// assert_eq!(config.protected_capacity().get(), 20);
    /// assert_eq!(config.max_size(), u64::MAX);
    /// ```
    #[must_use]
    pub fn new(capacity: NonZeroUsize, protected_capacity: NonZeroUsize) -> Self {
        assert!(
            protected_capacity.get() <= capacity.get(),
            "Protected capacity must be less than or equal to total capacity"
        );

        Self {
            capacity,
            protected_capacity,
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

    /// Returns the maximum number of key-value pairs the cache can hold.
    #[inline]
    pub fn capacity(&self) -> NonZeroUsize {
        self.capacity
    }

    /// Returns the maximum size of the protected segment.
    #[inline]
    pub fn protected_capacity(&self) -> NonZeroUsize {
        self.protected_capacity
    }

    /// Returns the maximum total size of cached content.
    #[inline]
    pub fn max_size(&self) -> u64 {
        self.max_size
    }
}

impl fmt::Debug for SlruCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SlruCacheConfig")
            .field("capacity", &self.capacity)
            .field("protected_capacity", &self.protected_capacity)
            .field("max_size", &self.max_size)
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
        assert_eq!(config.max_size(), u64::MAX);
    }

    #[test]
    fn test_slru_config_builder_pattern() {
        let config = SlruCacheConfig::new(
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(20).unwrap(),
        )
        .with_max_size(1024 * 1024);

        assert_eq!(config.capacity().get(), 100);
        assert_eq!(config.protected_capacity().get(), 20);
        assert_eq!(config.max_size(), 1024 * 1024);
    }

    #[test]
    #[should_panic(expected = "Protected capacity must be less than or equal to total capacity")]
    fn test_invalid_protected_capacity() {
        // This should panic because protected capacity is greater than total capacity
        let _ = SlruCacheConfig::new(
            NonZeroUsize::new(5).unwrap(),
            NonZeroUsize::new(10).unwrap(),
        );
    }
}
