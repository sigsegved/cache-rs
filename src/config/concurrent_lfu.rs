//! Configuration for the concurrent Least Frequently Used (LFU) cache.

extern crate std;

use super::LfuCacheConfig;
use core::fmt;
use core::num::NonZeroUsize;

/// Returns the default number of segments based on available parallelism.
fn default_segment_count() -> usize {
    std::thread::available_parallelism()
        .map(|p: std::num::NonZeroUsize| p.get())
        .unwrap_or(16)
        .clamp(4, 64)
}

/// Configuration for a concurrent LFU cache with segmented storage.
///
/// This is the **only** way to configure and create a concurrent LFU cache.
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
#[derive(Clone, Copy)]
pub struct ConcurrentLfuCacheConfig {
    /// Base configuration (capacity and max_size)
    base: LfuCacheConfig,
    /// Number of segments for sharding
    segments: usize,
}

impl ConcurrentLfuCacheConfig {
    /// Creates a new concurrent LFU cache configuration with the specified capacity.
    /// The segment count is clamped to not exceed capacity.
    #[must_use]
    pub fn new(capacity: NonZeroUsize) -> Self {
        let default_segments = default_segment_count();
        let segments = default_segments.min(capacity.get());
        Self {
            base: LfuCacheConfig::new(capacity),
            segments,
        }
    }

    /// Sets the number of segments for concurrent access.
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
    pub fn base_config(&self) -> &LfuCacheConfig {
        &self.base
    }
}

impl fmt::Debug for ConcurrentLfuCacheConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConcurrentLfuCacheConfig")
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
    fn test_concurrent_lfu_config_creation() {
        let config = ConcurrentLfuCacheConfig::new(NonZeroUsize::new(1000).unwrap());
        assert_eq!(config.capacity().get(), 1000);
        assert_eq!(config.max_size(), u64::MAX);
        assert!(config.segments() > 0);
    }

    #[test]
    fn test_concurrent_lfu_config_builder() {
        let config = ConcurrentLfuCacheConfig::new(NonZeroUsize::new(1000).unwrap())
            .with_segments(16)
            .with_max_size(1024 * 1024);
        assert_eq!(config.capacity().get(), 1000);
        assert_eq!(config.max_size(), 1024 * 1024);
        assert_eq!(config.segments(), 16);
    }
}
