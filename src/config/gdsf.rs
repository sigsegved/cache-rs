//! Configuration for the Greedy Dual-Size Frequency (GDSF) cache.
//!
//! This module provides configuration for GDSF caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::GdsfCacheConfig;
//! use cache_rs::GdsfCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple capacity-only config
//! let config = GdsfCacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     initial_age: 0.0,
//!     max_size: u64::MAX,
//! };
//! let cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
//!
//! // With size limit (recommended for GDSF)
//! let config = GdsfCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     initial_age: 0.0,
//!     max_size: 10 * 1024 * 1024,  // 10MB
//! };
//! let cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
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
///
/// # Fields
///
/// - `capacity`: Maximum number of entries the cache can hold
/// - `initial_age`: Initial global age value (default: 0.0)
/// - `max_size`: Maximum total size of cached content (use `u64::MAX` for unlimited)
///
/// # Examples
///
/// ```
/// use cache_rs::config::GdsfCacheConfig;
/// use cache_rs::GdsfCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration with just capacity
/// let config = GdsfCacheConfig {
///     capacity: NonZeroUsize::new(100).unwrap(),
///     initial_age: 0.0,
///     max_size: u64::MAX,
/// };
/// let cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
///
/// // Full configuration with size limit (recommended for GDSF)
/// let config = GdsfCacheConfig {
///     capacity: NonZeroUsize::new(1000).unwrap(),
///     initial_age: 0.0,
///     max_size: 50 * 1024 * 1024,  // 50MB limit
/// };
/// let cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
/// ```
#[derive(Clone, Copy)]
pub struct GdsfCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    pub capacity: NonZeroUsize,
    /// Initial global age value
    pub initial_age: f64,
    /// Maximum total size of cached content (sum of entry sizes).
    /// Use `u64::MAX` for no size limit.
    pub max_size: u64,
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
        let config = GdsfCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            initial_age: 0.0,
            max_size: u64::MAX,
        };
        assert_eq!(config.capacity.get(), 100);
        assert_eq!(config.initial_age, 0.0);
        assert_eq!(config.max_size, u64::MAX);
    }

    #[test]
    fn test_gdsf_config_with_initial_age() {
        let config = GdsfCacheConfig {
            capacity: NonZeroUsize::new(50).unwrap(),
            initial_age: 10.5,
            max_size: 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 50);
        assert_eq!(config.initial_age, 10.5);
        assert_eq!(config.max_size, 1024 * 1024);
    }
}
