//! Configuration for the concurrent Least Recently Used (LRU) cache.
//!
//! This module provides configuration for concurrent LRU caches.
//! Use `ConcurrentLruCacheConfig` as the single entry point for creating
//! thread-safe LRU caches.
//!
//! # Examples
//!
//! ```ignore
//! use cache_rs::config::ConcurrentLruCacheConfig;
//! use cache_rs::concurrent::ConcurrentLruCache;
//! use core::num::NonZeroUsize;
//!
//! // Simple capacity-only config
//! let config = ConcurrentLruCacheConfig::new(NonZeroUsize::new(10000).unwrap());
//! let cache: ConcurrentLruCache<String, i32> = ConcurrentLruCache::from_config(config);
//!
//! // With custom segment count and size limit
//! let config = ConcurrentLruCacheConfig::new(NonZeroUsize::new(10000).unwrap())
//!     .with_segments(32)
//!     .with_max_size(100 * 1024 * 1024);  // 100MB
//! let cache: ConcurrentLruCache<String, Vec<u8>> = ConcurrentLruCache::from_config(config);
//! ```

extern crate std;

use super::LruCacheConfig;
use core::fmt;
use core::num::NonZeroUsize;

/// Returns the default number of segments based on available parallelism.
fn default_segment_count() -> usize {
    // Use available parallelism, clamped to reasonable bounds
    std::thread::available_parallelism()
        .map(|p: std::num::NonZeroUsize| p.get())
        .unwrap_or(16)
        .clamp(4, 64)
}

/// Configuration for a concurrent LRU cache with segmented storage.
///
/// This is the **only** way to configure and create a concurrent LRU cache.
/// The cache uses multiple segments with independent locks for high concurrency.
///
/// # Required Parameters
///
/// - `capacity`: Total maximum number of entries across all segments (set in constructor)
///
/// # Optional Parameters (Builder Methods)
///
/// - `max_size`: Maximum total size of cached content (default: unlimited)
/// - `segments`: Number of independent segments (default: based on CPU count)
///
/// # Examples
///
/// ```ignore
/// use cache_rs::config::ConcurrentLruCacheConfig;
/// use cache_rs::concurrent::ConcurrentLruCache;
/// use core::num::NonZeroUsize;
///
/// // Basic configuration
/// let config = ConcurrentLruCacheConfig::new(NonZeroUsize::new(10000).unwrap());
/// let cache: ConcurrentLruCache<String, i32> = ConcurrentLruCache::from_config(config);
///
/// // Full configuration
/// let config = ConcurrentLruCacheConfig::new(NonZeroUsize::new(10000).unwrap())
///     .with_segments(32)       // 32 independent segments
///     .with_max_size(100 * 1024 * 1024);  // 100MB total
/// let cache: ConcurrentLruCache<String, Vec<u8>> = ConcurrentLruCache::from_config(config);
/// ```
#[derive(Clone, Copy)]
pub struct ConcurrentLruCacheConfig {
    /// Base configuration (capacity and max_size)
    base: LruCacheConfig,
    /// Number of segments for sharding
    segments: usize,
}

impl ConcurrentLruCacheConfig {
    /// Creates a new concurrent LRU cache configuration with the specified capacity.
    ///
    /// Uses the default number of segments based on available CPU parallelism.
    /// The segment count is clamped to not exceed capacity.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Total maximum number of entries across all segments
    #[must_use]
    pub fn new(capacity: NonZeroUsize) -> Self {
        let default_segments = default_segment_count();
        // Clamp segments to not exceed capacity
        let segments = default_segments.min(capacity.get());
        Self {
            base: LruCacheConfig::new(capacity),
            segments,
        }
    }

    /// Creates a new concurrent LRU cache configuration with specified capacity and segments.
    ///
    /// # Arguments
    ///
    /// * `capacity` - Total maximum number of entries across all segments
    /// * `segments` - Number of independent segments
    ///
    /// # Panics
    ///
    /// Panics if `segments` is 0 or greater than capacity.
    #[must_use]
    pub fn with_capacity_and_segments(capacity: NonZeroUsize, segments: usize) -> Self {
        assert!(segments > 0, "segments must be > 0");
        assert!(
            capacity.get() >= segments,
            "capacity must be >= segment count"
        );
        Self {
            base: LruCacheConfig::new(capacity),
            segments,
        }
    }

    /// Sets the number of segments for concurrent access.
    ///
    /// More segments = less lock contention but more memory overhead.
    /// Use a power of 2 for optimal hash distribution.
    ///
    /// # Arguments
    ///
    /// * `segments` - Number of independent segments (must be > 0)
    ///
    /// # Panics
    ///
    /// Panics if `segments` is 0 or greater than capacity.
    #[must_use]
    pub fn with_segments(mut self, segments: usize) -> Self {
        assert!(segments > 0, "segments must be > 0");
        assert!(
            self.base.capacity().get() >= segments,
            "capacity must be >= segment count"
        );
        self.segments = segments;
        self
    }

    /// Sets the maximum total size of cached content.
    ///
    /// The size is distributed across segments proportionally.
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum total size in bytes (or your chosen unit)
    #[must_use]
    pub fn with_max_size(mut self, max_size: u64) -> Self {
        self.base = self.base.with_max_size(max_size);
        self
    }

    /// Returns the total capacity across all segments.
    #[inline]
    pub fn capacity(&self) -> NonZeroUsize {
        self.base.capacity()
    }

    /// Returns the maximum total size of cached content.
    #[inline]
    pub fn max_size(&self) -> u64 {
        self.base.max_size()
    }

    /// Returns the number of segments.
    #[inline]
    pub fn segments(&self) -> usize {
        self.segments
    }

    /// Returns the base configuration.
    #[inline]
    pub fn base_config(&self) -> &LruCacheConfig {
        &self.base
    }
}

impl fmt::Debug for ConcurrentLruCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConcurrentLruCacheConfig")
            .field("capacity", &self.base.capacity())
            .field("max_size", &self.base.max_size())
            .field("segments", &self.segments)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concurrent_lru_config_creation() {
        let config = ConcurrentLruCacheConfig::new(NonZeroUsize::new(1000).unwrap());
        assert_eq!(config.capacity().get(), 1000);
        assert_eq!(config.max_size(), u64::MAX);
        assert!(config.segments() > 0);
    }

    #[test]
    fn test_concurrent_lru_config_builder() {
        let config = ConcurrentLruCacheConfig::new(NonZeroUsize::new(1000).unwrap())
            .with_segments(16)
            .with_max_size(1024 * 1024);
        assert_eq!(config.capacity().get(), 1000);
        assert_eq!(config.max_size(), 1024 * 1024);
        assert_eq!(config.segments(), 16);
    }
}
