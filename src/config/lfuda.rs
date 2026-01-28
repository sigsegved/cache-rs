//! Configuration for the Least Frequently Used with Dynamic Aging (LFUDA) cache.
//!
//! This module provides configuration for LFUDA caches.
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::LfudaCacheConfig;
//! use cache_rs::LfudaCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple capacity-only config
//! let config = LfudaCacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     initial_age: 0,
//!     max_size: u64::MAX,
//! };
//! let cache: LfudaCache<String, i32> = LfudaCache::init(config, None);
//!
//! // With size limit and initial age
//! let config = LfudaCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     initial_age: 100,
//!     max_size: 10 * 1024 * 1024,  // 10MB
//! };
//! let cache: LfudaCache<String, Vec<u8>> = LfudaCache::init(config, None);
//! ```

use core::fmt;
use core::num::NonZeroUsize;

/// Configuration for an LFUDA (Least Frequently Used with Dynamic Aging) cache.
///
/// LFUDA enhances LFU by using a dynamic aging mechanism that prevents old
/// frequently-accessed items from permanently blocking new items.
///
/// # Fields
///
/// - `capacity`: Maximum number of entries the cache can hold
/// - `initial_age`: Initial global age value (default: 0)
/// - `max_size`: Maximum total size of cached content (use `u64::MAX` for unlimited)
///
/// # Examples
///
/// ```
/// use cache_rs::config::LfudaCacheConfig;
/// use cache_rs::LfudaCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration with just capacity
/// let config = LfudaCacheConfig {
///     capacity: NonZeroUsize::new(100).unwrap(),
///     initial_age: 0,
///     max_size: u64::MAX,
/// };
/// let cache: LfudaCache<&str, i32> = LfudaCache::init(config, None);
///
/// // Full configuration with size limit and initial age
/// let config = LfudaCacheConfig {
///     capacity: NonZeroUsize::new(1000).unwrap(),
///     initial_age: 100,
///     max_size: 50 * 1024 * 1024,  // 50MB limit
/// };
/// let cache: LfudaCache<String, Vec<u8>> = LfudaCache::init(config, None);
/// ```
#[derive(Clone, Copy)]
pub struct LfudaCacheConfig {
    /// Maximum number of key-value pairs the cache can hold
    pub capacity: NonZeroUsize,
    /// Initial global age value
    pub initial_age: usize,
    /// Maximum total size of cached content (sum of entry sizes).
    /// Use `u64::MAX` for no size limit.
    pub max_size: u64,
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
        let config = LfudaCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            initial_age: 0,
            max_size: u64::MAX,
        };
        assert_eq!(config.capacity.get(), 100);
        assert_eq!(config.initial_age, 0);
        assert_eq!(config.max_size, u64::MAX);
    }

    #[test]
    fn test_lfuda_config_with_initial_age() {
        let config = LfudaCacheConfig {
            capacity: NonZeroUsize::new(50).unwrap(),
            initial_age: 10,
            max_size: 1024 * 1024,
        };
        assert_eq!(config.capacity.get(), 50);
        assert_eq!(config.initial_age, 10);
        assert_eq!(config.max_size, 1024 * 1024);
    }
}
