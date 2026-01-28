//! Cache Configuration Module
//!
//! This module provides configuration structures for all cache algorithm implementations.
//! Each cache type has its own dedicated configuration struct that encapsulates
//! algorithm-specific parameters.
//!
//! # Design Philosophy
//!
//! Each cache is created using its configuration struct as the **single entry point**.
//! This provides several benefits:
//!
//! - **Consistent API**: All caches are created the same way: `Cache::from_config(config)`
//! - **Builder pattern**: Optional parameters use fluent builder methods
//! - **Type safety**: All required parameters must be provided at construction
//! - **Extensible**: New parameters can be added without breaking existing code
//!
//! # Single-Threaded Cache Configs
//!
//! | Config | Cache | Description |
//! |--------|-------|-------------|
//! | [`LruCacheConfig`] | [`LruCache`](crate::LruCache) | Least Recently Used |
//! | [`LfuCacheConfig`] | [`LfuCache`](crate::LfuCache) | Least Frequently Used |
//! | [`LfudaCacheConfig`] | [`LfudaCache`](crate::LfudaCache) | LFU with Dynamic Aging |
//! | [`SlruCacheConfig`] | [`SlruCache`](crate::SlruCache) | Segmented LRU |
//! | [`GdsfCacheConfig`] | [`GdsfCache`](crate::GdsfCache) | Greedy Dual-Size Frequency |
//!
//! # Concurrent Cache Configs (requires `concurrent` feature)
//!
//! | Config | Cache | Description |
//! |--------|-------|-------------|
//! | [`ConcurrentLruCacheConfig`] | `ConcurrentLruCache` | Thread-safe LRU |
//! | [`ConcurrentLfuCacheConfig`] | `ConcurrentLfuCache` | Thread-safe LFU |
//! | [`ConcurrentLfudaCacheConfig`] | `ConcurrentLfudaCache` | Thread-safe LFUDA |
//! | [`ConcurrentSlruCacheConfig`] | `ConcurrentSlruCache` | Thread-safe SLRU |
//! | [`ConcurrentGdsfCacheConfig`] | `ConcurrentGdsfCache` | Thread-safe GDSF |
//!
//! # Examples
//!
//! ```
//! use cache_rs::config::LruCacheConfig;
//! use cache_rs::LruCache;
//! use core::num::NonZeroUsize;
//!
//! // Create config with required capacity
//! let config = LruCacheConfig::new(NonZeroUsize::new(1000).unwrap());
//!
//! // Create cache from config
//! let cache: LruCache<String, i32> = LruCache::from_config(config);
//! ```

// Single-threaded cache configs
pub mod gdsf;
pub mod lfu;
pub mod lfuda;
pub mod lru;
pub mod slru;

// Concurrent cache configs (always compiled, but only useful with concurrent feature)
#[cfg(feature = "concurrent")]
pub mod concurrent_gdsf;
#[cfg(feature = "concurrent")]
pub mod concurrent_lfu;
#[cfg(feature = "concurrent")]
pub mod concurrent_lfuda;
#[cfg(feature = "concurrent")]
pub mod concurrent_lru;
#[cfg(feature = "concurrent")]
pub mod concurrent_slru;

// Re-exports for convenience - single-threaded
pub use gdsf::GdsfCacheConfig;
pub use lfu::LfuCacheConfig;
pub use lfuda::LfudaCacheConfig;
pub use lru::LruCacheConfig;
pub use slru::SlruCacheConfig;

// Re-exports for convenience - concurrent
#[cfg(feature = "concurrent")]
pub use concurrent_gdsf::ConcurrentGdsfCacheConfig;
#[cfg(feature = "concurrent")]
pub use concurrent_lfu::ConcurrentLfuCacheConfig;
#[cfg(feature = "concurrent")]
pub use concurrent_lfuda::ConcurrentLfudaCacheConfig;
#[cfg(feature = "concurrent")]
pub use concurrent_lru::ConcurrentLruCacheConfig;
#[cfg(feature = "concurrent")]
pub use concurrent_slru::ConcurrentSlruCacheConfig;
