//! Cache Configuration Module
//!
//! This module provides configuration structures for all cache algorithm implementations.
//! Each cache type has its own dedicated configuration struct that encapsulates
//! algorithm-specific parameters.
//!
//! Using configurations instead of individual fields provides several benefits:
//! - Cleaner API with related parameters grouped together
//! - Easier to extend with new parameters without breaking existing code
//! - Supports configuration reuse across multiple cache instances
//! - Enables factory patterns and programmatic cache configuration

pub mod gdsf;
pub mod lfu;
pub mod lfuda;
pub mod lru;
pub mod slru;

// Re-exports for convenience
pub use gdsf::GdsfCacheConfig;
pub use lfu::LfuCacheConfig;
pub use lfuda::LfudaCacheConfig;
pub use lru::LruCacheConfig;
pub use slru::SlruCacheConfig;
