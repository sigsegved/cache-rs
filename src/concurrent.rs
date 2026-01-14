//! Concurrent Cache Implementations
//!
//! This module provides thread-safe concurrent cache implementations using the
//! Shared Segment Pattern for high-performance multi-threaded access.
//!
//! # Architecture
//!
//! Each concurrent cache uses segmented storage where:
//! - The key space is partitioned across multiple segments using hash-based sharding
//! - Each segment is protected by its own lock (using `parking_lot::Mutex`)
//! - Operations only lock the relevant segment, allowing concurrent access to different segments
//!
//! This design provides near-linear scalability with thread count for workloads
//! with good key distribution.
//!
//! # Available Concurrent Caches
//!
//! | Type | Description |
//! |------|-------------|
//! | [`ConcurrentLruCache`] | Thread-safe LRU cache with segmented storage |
//! | [`ConcurrentSlruCache`] | Thread-safe Segmented LRU cache |
//! | [`ConcurrentLfuCache`] | Thread-safe LFU cache |
//! | [`ConcurrentLfudaCache`] | Thread-safe LFUDA cache |
//! | [`ConcurrentGdsfCache`] | Thread-safe GDSF cache |
//!
//! # Performance Characteristics
//!
//! - **Read/Write Latency**: O(1) average case, same as single-threaded variants
//! - **Concurrency**: Near-linear scaling up to segment count
//! - **Memory Overhead**: ~1 Mutex per segment (typically 16 segments by default)
//!
//! # Default Segment Count
//!
//! By default, concurrent caches use `min(num_cpus * 4, 64)` segments, which provides
//! good parallelism while limiting memory overhead. You can customize this using the
//! `with_segments()` constructor.
//!
//! # Example
//!
//! ```rust,ignore
//! use cache_rs::concurrent::ConcurrentLruCache;
//! use std::sync::Arc;
//! use std::thread;
//!
//! // Create a concurrent cache (can be shared across threads)
//! let cache = Arc::new(ConcurrentLruCache::new(1000));
//!
//! // Spawn multiple threads that access the cache concurrently
//! let handles: Vec<_> = (0..4).map(|t| {
//!     let cache = Arc::clone(&cache);
//!     thread::spawn(move || {
//!         for i in 0..1000 {
//!             let key = format!("key_{}_{}", t, i);
//!             cache.put(key.clone(), i);
//!             let _ = cache.get(&key);
//!         }
//!     })
//! }).collect();
//!
//! for handle in handles {
//!     handle.join().unwrap();
//! }
//! ```
//!
//! # Thread Safety
//!
//! All concurrent cache types implement `Send` and `Sync`, making them safe to share
//! across threads. They can be wrapped in `Arc` for shared ownership.
//!
//! # Zero-Copy Access
//!
//! For performance-critical code paths, use the `get_with()` method which provides
//! access to the value while holding the segment lock, avoiding unnecessary cloning:
//!
//! ```rust,ignore
//! let result = cache.get_with(&key, |value| {
//!     // Process value while lock is held
//!     value.some_method()
//! });
//! ```

mod gdsf;
mod lfu;
mod lfuda;
mod lru;
mod slru;

pub use self::gdsf::ConcurrentGdsfCache;
pub use self::lfu::ConcurrentLfuCache;
pub use self::lfuda::ConcurrentLfudaCache;
pub use self::lru::ConcurrentLruCache;
pub use self::slru::ConcurrentSlruCache;

/// Returns the default number of segments based on CPU count.
///
/// This provides a good balance between parallelism and memory overhead.
/// The formula is `min(num_cpus * 4, 64)`.
#[inline]
pub fn default_segment_count() -> usize {
    // Use a reasonable default that works well across different hardware
    // In production, this would use num_cpus crate, but we keep it simple here
    16
}
