//! # Cache
//!
//! A collection of high-performance, memory-efficient cache implementations supporting various eviction policies.
//!
//! This crate provides cache implementations optimized for performance and memory usage that can be used
//! in both std and no_std environments. All cache operations (`get`, `get_mut`, `put`, and `remove`)
//! have O(1) time complexity.
//!
//! ## Available Cache Algorithms
//!
//! | Algorithm | Description | Best Use Case |
//! |-----------|-------------|---------------|
//! | [`LruCache`] | Least Recently Used | General purpose, recency-based access patterns |
//! | [`SlruCache`] | Segmented LRU | Mixed access patterns with both hot and cold items |
//! | [`LfuCache`] | Least Frequently Used | Frequency-based access patterns |
//! | [`LfudaCache`] | LFU with Dynamic Aging | Long-running caches with changing popularity |
//! | [`GdsfCache`] | Greedy Dual Size Frequency | CDNs and size-aware caching |
//!
//! ## Performance Characteristics
//!
//! | Algorithm | Space Overhead | Hit Rate for Recency | Hit Rate for Frequency | Scan Resistance |
//! |-----------|---------------|----------------------|------------------------|----------------|
//! | LRU       | Low           | High                 | Low                    | Poor           |
//! | SLRU      | Medium        | High                 | Medium                 | Good           |
//! | LFU       | Medium        | Low                  | High                   | Excellent      |
//! | LFUDA     | Medium        | Medium               | High                   | Excellent      |
//! | GDSF      | High          | Medium               | High                   | Good           |
//!
//! ## When to Use Each Algorithm
//!
//! - **LRU**: Use for general-purpose caching where recent items are likely to be accessed again.
//! - **SLRU**: Use when you have a mix of frequently and occasionally accessed items.
//! - **LFU**: Use when access frequency is more important than recency.
//! - **LFUDA**: Use for long-running caches where item popularity changes over time.
//! - **GDSF**: Use when items have different sizes and you want to optimize for both size and popularity.
//!
//! ## Feature Flags
//!
//! - `hashbrown`: Uses the [hashbrown](https://crates.io/crates/hashbrown) crate for HashMap implementation (enabled by default)
//! - `nightly`: Enables nightly-only optimizations for improved performance
//! - `std`: Enables standard library features (disabled by default to support `no_std`)
//! - `concurrent`: Enables thread-safe concurrent cache implementations using `parking_lot`
//!
//! ## No-std Support
//!
//! This crate works in `no_std` environments by default. Enable the `std` feature for additional functionality.
//!
//! ### Example with no_std
//!
//! ```rust
//! use cache_rs::LruCache;
//! use core::num::NonZeroUsize;
//!
//! // Create a cache in a no_std environment
//! let mut cache = LruCache::new(NonZeroUsize::new(100).unwrap());
//! cache.put("key", "value");
//! assert_eq!(cache.get(&"key"), Some(&"value"));
//! ```
//!
//! ## Modules
//!
//! - [`lru`]: A Least Recently Used (LRU) cache implementation
//! - [`slru`]: A Segmented LRU (SLRU) cache implementation
//! - [`lfu`]: A Least Frequently Used (LFU) cache implementation
//! - [`lfuda`]: An LFU with Dynamic Aging (LFUDA) cache implementation
//! - [`gdsf`]: A Greedy Dual Size Frequency (GDSF) cache implementation
//! - [`config`]: Configuration structures for all cache algorithm implementations
//! - [`metrics`]: Metrics collection for cache performance monitoring
//! - [`concurrent`]: Thread-safe concurrent cache implementations (requires `concurrent` feature)

#![no_std]

#[cfg(test)]
extern crate scoped_threadpool;

/// Doubly linked list implementation with in-place editing capabilities.
///
/// This module provides a memory-efficient doubly linked list that allows for
/// efficient insertion, removal, and reordering operations.
///
/// **Note**: This module is internal infrastructure and should not be used directly
/// by library consumers. It exposes unsafe raw pointer operations that require
/// careful invariant maintenance. Use the high-level cache implementations instead.
pub(crate) mod list;

/// Cache configuration structures.
///
/// Provides configuration structures for all cache algorithm implementations.
pub mod config;

/// Least Recently Used (LRU) cache implementation.
///
/// Provides a fixed-size cache that evicts the least recently used items when
/// the capacity is reached.
pub mod lru;

/// Segmented LRU (SLRU) cache implementation.
///
/// Provides a fixed-size cache that uses a segmented approach to evict
/// items based on their usage patterns. This is useful for scenarios where
/// certain items are accessed more frequently than others.
pub mod slru;

/// Least Frequently Used (LFU) cache implementation.
///
/// Provides a fixed-size cache that evicts the least frequently used items
/// when capacity is reached. Items are tracked by their access frequency.
pub mod lfu;

/// Least Frequently Used with Dynamic Aging (LFUDA) cache implementation.
///
/// An enhanced LFU cache that addresses the aging problem by dynamically
/// adjusting item priorities based on a global aging factor.
pub mod lfuda;

/// Greedy Dual-Size Frequency (GDSF) cache implementation.
///
/// A cache replacement algorithm that combines frequency, size, and aging.
/// Assigns priority based on (Frequency / Size) + Global_Age formula.
pub mod gdsf;

/// Cache metrics system.
///
/// Provides a flexible metrics collection and reporting system for all cache algorithms.
/// Each algorithm can track algorithm-specific metrics while implementing a common interface.
pub mod metrics;

/// Concurrent cache implementations.
///
/// Provides thread-safe cache implementations using segmented storage for high-performance
/// multi-threaded access. Each concurrent cache partitions the key space across multiple
/// segments, with each segment protected by its own lock.
///
/// Available when the `concurrent` feature is enabled.
#[cfg(feature = "concurrent")]
pub mod concurrent;

pub use gdsf::GdsfCache;
pub use lfu::LfuCache;
pub use lfuda::LfudaCache;
pub use lru::LruCache;
pub use slru::SlruCache;

#[cfg(feature = "concurrent")]
pub use concurrent::{
    ConcurrentGdsfCache, ConcurrentLfuCache, ConcurrentLfudaCache, ConcurrentLruCache,
    ConcurrentSlruCache,
};
