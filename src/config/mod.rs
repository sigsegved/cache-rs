//! Cache Configuration Module
//!
//! This module provides configuration structures for all cache algorithm implementations.
//! Each cache type has its own dedicated configuration struct with public fields.
//!
//! # Design Philosophy
//!
//! Configuration structs have all public fields for simple instantiation:
//!
//! - **Simple**: Just create the struct with all fields set
//! - **Type safety**: All parameters must be provided at construction
//! - **No boilerplate**: No constructors or builder methods needed
//!
//! # Single-Threaded Cache Configs
//!
//! | Config | Cache | Description |
//! |--------|-------|-------------|
//! | `LruCacheConfig` | [`LruCache`](crate::LruCache) | Least Recently Used |
//! | `LfuCacheConfig` | [`LfuCache`](crate::LfuCache) | Least Frequently Used |
//! | `LfudaCacheConfig` | [`LfudaCache`](crate::LfudaCache) | LFU with Dynamic Aging |
//! | `SlruCacheConfig` | [`SlruCache`](crate::SlruCache) | Segmented LRU |
//! | `GdsfCacheConfig` | [`GdsfCache`](crate::GdsfCache) | Greedy Dual-Size Frequency |
//!
//! # Concurrent Cache Configs (requires `concurrent` feature)
//!
//! Use `ConcurrentCacheConfig<C>` wrapper around any base config:
//!
//! | Type Alias | Base Config | Description |
//! |------------|-------------|-------------|
//! | `ConcurrentLruCacheConfig` | `LruCacheConfig` | Thread-safe LRU |
//! | `ConcurrentLfuCacheConfig` | `LfuCacheConfig` | Thread-safe LFU |
//! | `ConcurrentLfudaCacheConfig` | `LfudaCacheConfig` | Thread-safe LFUDA |
//! | `ConcurrentSlruCacheConfig` | `SlruCacheConfig` | Thread-safe SLRU |
//! | `ConcurrentGdsfCacheConfig` | `GdsfCacheConfig` | Thread-safe GDSF |
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::LruCacheConfig;
//! use cache_rs::LruCache;
//! use core::num::NonZeroUsize;
//!
//! // Create config with all fields
//! let config = LruCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     max_size: u64::MAX,
//! };
//!
//! // Create cache from config
//! let cache: LruCache<String, i32> = LruCache::init(config, None);
//! ```

// Single-threaded cache configs
pub mod gdsf;
pub mod lfu;
pub mod lfuda;
pub mod lru;
pub mod slru;

// Re-exports for convenience - single-threaded
pub use gdsf::GdsfCacheConfig;
pub use lfu::LfuCacheConfig;
pub use lfuda::LfudaCacheConfig;
pub use lru::LruCacheConfig;
pub use slru::SlruCacheConfig;

/// Generic configuration wrapper for concurrent caches.
///
/// Wraps any base cache configuration and adds the `segments` field
/// for controlling the number of independent segments used for sharding.
///
/// # Type Parameter
///
/// - `C`: The base cache configuration type (e.g., `LruCacheConfig`, `LfuCacheConfig`)
///
/// # Fields
///
/// - `base`: The underlying single-threaded cache configuration
/// - `segments`: Number of independent segments for sharding (more = less contention)
///
/// # Example
///
/// ```ignore
/// use cache_rs::config::{ConcurrentCacheConfig, LruCacheConfig, ConcurrentLruCacheConfig};
/// use core::num::NonZeroUsize;
///
/// // Using the type alias
/// let config: ConcurrentLruCacheConfig = ConcurrentCacheConfig {
///     base: LruCacheConfig {
///         capacity: NonZeroUsize::new(10000).unwrap(),
///         max_size: u64::MAX,
///     },
///     segments: 16,
/// };
/// ```
#[cfg(feature = "concurrent")]
#[derive(Clone, Copy)]
pub struct ConcurrentCacheConfig<C> {
    /// Base configuration for the underlying cache algorithm
    pub base: C,
    /// Number of segments for sharding (more segments = less contention)
    pub segments: usize,
}

#[cfg(feature = "concurrent")]
impl<C: core::fmt::Debug> core::fmt::Debug for ConcurrentCacheConfig<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ConcurrentCacheConfig")
            .field("base", &self.base)
            .field("segments", &self.segments)
            .finish()
    }
}

// Type aliases for concurrent cache configs
#[cfg(feature = "concurrent")]
/// Configuration for a concurrent LRU cache.
/// Type alias for `ConcurrentCacheConfig<LruCacheConfig>`.
pub type ConcurrentLruCacheConfig = ConcurrentCacheConfig<LruCacheConfig>;

#[cfg(feature = "concurrent")]
/// Configuration for a concurrent LFU cache.
/// Type alias for `ConcurrentCacheConfig<LfuCacheConfig>`.
pub type ConcurrentLfuCacheConfig = ConcurrentCacheConfig<LfuCacheConfig>;

#[cfg(feature = "concurrent")]
/// Configuration for a concurrent LFUDA cache.
/// Type alias for `ConcurrentCacheConfig<LfudaCacheConfig>`.
pub type ConcurrentLfudaCacheConfig = ConcurrentCacheConfig<LfudaCacheConfig>;

#[cfg(feature = "concurrent")]
/// Configuration for a concurrent SLRU cache.
/// Type alias for `ConcurrentCacheConfig<SlruCacheConfig>`.
pub type ConcurrentSlruCacheConfig = ConcurrentCacheConfig<SlruCacheConfig>;

#[cfg(feature = "concurrent")]
/// Configuration for a concurrent GDSF cache.
/// Type alias for `ConcurrentCacheConfig<GdsfCacheConfig>`.
pub type ConcurrentGdsfCacheConfig = ConcurrentCacheConfig<GdsfCacheConfig>;
