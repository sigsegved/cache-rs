#![doc = include_str!("../README.md")]
//!
//! ---
//!
//! # Code Reference
//!
//! This section provides quick code examples and API references for each cache algorithm.
//!
//! ## Algorithm Selection Guide
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                    Which Cache Algorithm Should I Use?                       │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                              │
//! │  Is your workload primarily...                                               │
//! │                                                                              │
//! │  ┌─────────────────┐                                                         │
//! │  │ Recency-based?  │──Yes──▶ Are you worried about scans?                   │
//! │  │ (recent = hot)  │              │                                          │
//! │  └────────┬────────┘         Yes  │  No                                      │
//! │           │                   │   │                                          │
//! │          No                   ▼   ▼                                          │
//! │           │               ┌──────────┐  ┌──────────┐                         │
//! │           │               │   SLRU   │  │   LRU    │                         │
//! │           ▼               └──────────┘  └──────────┘                         │
//! │  ┌─────────────────┐                                                         │
//! │  │ Frequency-based?│──Yes──▶ Does popularity change over time?              │
//! │  │ (popular = hot) │              │                                          │
//! │  └────────┬────────┘         Yes  │  No                                      │
//! │           │                   │   │                                          │
//! │          No                   ▼   ▼                                          │
//! │           │               ┌──────────┐  ┌──────────┐                         │
//! │           │               │  LFUDA   │  │   LFU    │                         │
//! │           ▼               └──────────┘  └──────────┘                         │
//! │  ┌─────────────────┐                                                         │
//! │  │ Variable-sized  │──Yes──▶ ┌──────────┐                                   │
//! │  │    objects?     │         │   GDSF   │                                    │
//! │  └─────────────────┘         └──────────┘                                    │
//! │                                                                              │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Quick Reference
//!
//! | Algorithm | Description | Best Use Case |
//! |-----------|-------------|---------------|
//! | [`LruCache`] | Least Recently Used | General purpose, recency-based access |
//! | [`SlruCache`] | Segmented LRU | Mixed workloads with scans |
//! | [`LfuCache`] | Least Frequently Used | Stable popularity patterns |
//! | [`LfudaCache`] | LFU with Dynamic Aging | Long-running, evolving popularity |
//! | [`GdsfCache`] | Greedy Dual Size Frequency | CDNs, variable-sized objects |
//!
//! ## Performance Characteristics
//!
//! | Algorithm | Get | Put | Remove | Memory/Entry | Scan Resist | Adapts |
//! |-----------|-----|-----|--------|--------------|-------------|--------|
//! | LRU       | O(1)| O(1)| O(1)   | ~80 bytes    | Poor        | N/A    |
//! | SLRU      | O(1)| O(1)| O(1)   | ~90 bytes    | Good        | N/A    |
//! | LFU       | O(1)| O(1)| O(1)   | ~100 bytes   | Excellent   | No     |
//! | LFUDA     | O(1)| O(1)| O(1)   | ~110 bytes   | Excellent   | Yes    |
//! | GDSF      | O(1)| O(1)| O(1)   | ~120 bytes   | Good        | Yes    |
//!
//! ## Code Examples
//!
//! ### LRU (Least Recently Used)
//!
//! Evicts the item that hasn't been accessed for the longest time. Simple and effective
//! for workloads with temporal locality.
//!
//! ```rust
//! use cache_rs::LruCache;
//! use cache_rs::config::LruCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = LruCacheConfig {
//!     capacity: NonZeroUsize::new(2).unwrap(),
//!     max_size: u64::MAX,
//! };
//! let mut cache = LruCache::init(config, None);
//! cache.put("a", 1);
//! cache.put("b", 2);
//! cache.get(&"a");      // "a" becomes most recently used
//! cache.put("c", 3);    // "b" evicted (least recently used)
//! assert!(cache.get(&"b").is_none());
//! ```
//!
//! ### SLRU (Segmented LRU)
//!
//! Divides cache into probationary and protected segments. Items enter probationary
//! and are promoted to protected on second access. Excellent scan resistance.
//!
//! ```rust
//! use cache_rs::SlruCache;
//! use cache_rs::config::SlruCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = SlruCacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     protected_capacity: NonZeroUsize::new(20).unwrap(),
//!     max_size: u64::MAX,
//! };
//! let mut cache = SlruCache::init(config, None);
//!
//! cache.put("hot", 1);
//! cache.get(&"hot");  // Promoted to protected segment!
//! ```
//!
//! ### LFU (Least Frequently Used)
//!
//! Tracks access frequency and evicts the least frequently accessed item.
//! Great for workloads with stable popularity patterns.
//!
//! ```rust
//! use cache_rs::LfuCache;
//! use cache_rs::config::LfuCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = LfuCacheConfig {
//!     capacity: NonZeroUsize::new(2).unwrap(),
//!     max_size: u64::MAX,
//! };
//! let mut cache = LfuCache::init(config, None);
//! cache.put("rare", 1);
//! cache.put("popular", 2);
//!
//! // Access "popular" multiple times
//! for _ in 0..10 { cache.get(&"popular"); }
//!
//! cache.put("new", 3);  // "rare" evicted (lowest frequency)
//! assert!(cache.get(&"popular").is_some());
//! ```
//!
//! ### LFUDA (LFU with Dynamic Aging)
//!
//! Addresses LFU's "cache pollution" problem by incorporating aging. Old popular
//! items gradually lose priority, allowing new items to compete fairly.
//!
//! ```rust
//! use cache_rs::LfudaCache;
//! use cache_rs::config::LfudaCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = LfudaCacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     initial_age: 0,
//!     max_size: u64::MAX,
//! };
//! let mut cache = LfudaCache::init(config, None);
//!
//! // Old popular items will eventually age out if not accessed
//! for i in 0..100 {
//!     cache.put(i, i);
//! }
//! ```
//!
//! ### GDSF (Greedy Dual-Size Frequency)
//!
//! Combines frequency, size, and aging. Priority = (Frequency / Size) + Age.
//! Ideal for caching variable-sized objects like images or API responses.
//!
//! ```rust
//! use cache_rs::GdsfCache;
//! use cache_rs::config::GdsfCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = GdsfCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     initial_age: 0.0,
//!     max_size: 10 * 1024 * 1024,  // 10MB
//! };
//! let mut cache: GdsfCache<String, Vec<u8>> = GdsfCache::init(config, None);
//!
//! // Size-aware insertion
//! cache.put("small.txt".to_string(), vec![0u8; 100], 100);
//! cache.put("large.bin".to_string(), vec![0u8; 10000], 10000);
//! // Small items get higher priority per byte
//! ```
//!
//! ## Concurrent Caches
//!
//! Enable the `concurrent` feature for thread-safe versions:
//!
//! ```toml
//! [dependencies]
//! cache-rs = { version = "0.3", features = ["concurrent"] }
//! ```
//!
//! ```rust,ignore
//! use cache_rs::ConcurrentLruCache;
//! use std::sync::Arc;
//!
//! let cache = Arc::new(ConcurrentLruCache::new(10_000));
//!
//! // Safe to share across threads
//! let cache_clone = Arc::clone(&cache);
//! std::thread::spawn(move || {
//!     cache_clone.put("key".to_string(), 42);
//! });
//! ```
//!
//! Concurrent caches use **lock striping** for high throughput:
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────┐
//! │              ConcurrentCache (16 segments)                         │
//! │                                                                    │
//! │  ┌─────────┐ ┌─────────┐ ┌─────────┐     ┌─────────┐              │
//! │  │Segment 0│ │Segment 1│ │Segment 2│ ... │Segment15│              │
//! │  │ [Mutex] │ │ [Mutex] │ │ [Mutex] │     │ [Mutex] │              │
//! │  └─────────┘ └─────────┘ └─────────┘     └─────────┘              │
//! │       ▲           ▲           ▲               ▲                   │
//! │       │           │           │               │                   │
//! │  hash(k1)%16  hash(k2)%16  hash(k3)%16   hash(kN)%16              │
//! └────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Dual-Limit Capacity
//!
//! All caches support both entry count and size limits:
//!
//! ```rust
//! use cache_rs::LruCache;
//! use cache_rs::config::LruCacheConfig;
//! use core::num::NonZeroUsize;
//!
//! // Limit by both count (1000 entries) AND size (10MB)
//! let config = LruCacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     max_size: 10 * 1024 * 1024,
//! };
//! let mut cache: LruCache<String, Vec<u8>> = LruCache::init(config, None);
//!
//! // Track size explicitly
//! let data = vec![0u8; 1024];
//! cache.put_with_size("file.bin".to_string(), data, 1024);
//! ```
//!
//! ## Modules
//!
//! - [`lru`]: Least Recently Used cache implementation
//! - [`slru`]: Segmented LRU cache implementation
//! - [`lfu`]: Least Frequently Used cache implementation
//! - [`lfuda`]: LFU with Dynamic Aging cache implementation
//! - [`gdsf`]: Greedy Dual Size Frequency cache implementation
//! - [`config`]: Configuration structures for all cache algorithms
//! - [`metrics`]: Metrics collection for cache performance monitoring
//! - [`concurrent`]: Thread-safe concurrent cache implementations (requires `concurrent` feature)

#![no_std]

#[cfg(test)]
extern crate scoped_threadpool;

/// Unified cache entry type.
///
/// Provides a generic `CacheEntry<K, V, M>` structure that holds key, value,
/// timestamps, and algorithm-specific metadata. This is the foundation for
/// the dual-limit capacity management system.
pub mod entry;

/// Algorithm-specific metadata types.
///
/// Provides metadata structures for each cache algorithm:
/// - `LfuMeta`: Frequency counter for LFU
/// - `LfudaMeta`: Frequency for LFUDA (age is cache-global)
/// - `SlruMeta`: Segment location for SLRU
/// - `GdsfMeta`: Frequency and priority for GDSF
pub mod meta;

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

// Re-export cache types
pub use gdsf::GdsfCache;
pub use lfu::LfuCache;
pub use lfuda::LfudaCache;
pub use lru::LruCache;
pub use slru::SlruCache;

// Re-export entry type
pub use entry::CacheEntry;

// Re-export metadata types
pub use meta::{GdsfMeta, LfuMeta, LfudaMeta, SlruMeta, SlruSegment};

#[cfg(feature = "concurrent")]
pub use concurrent::{
    ConcurrentGdsfCache, ConcurrentLfuCache, ConcurrentLfudaCache, ConcurrentLruCache,
    ConcurrentSlruCache,
};
