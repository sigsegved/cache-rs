//! Algorithm-Specific Metadata Types (Re-exports)
//!
//! This module re-exports metadata types from their respective algorithm modules
//! for backward compatibility. For new code, prefer importing directly from
//! the algorithm module (e.g., `cache_rs::lfu::LfuMeta`).
//!
//! # Metadata Types
//!
//! | Algorithm | Metadata Type | Location |
//! |-----------|---------------|----------|
//! | LRU       | `()` (none)   | N/A |
//! | LFU       | `LfuMeta`     | `cache_rs::lfu` |
//! | LFUDA     | `LfudaMeta`   | `cache_rs::lfuda` |
//! | GDSF      | `GdsfMeta`    | `cache_rs::gdsf` |
//!
//! # Migration Guide
//!
//! ```ignore
//! // Old way (still works for backward compatibility)
//! use cache_rs::meta::LfuMeta;
//!
//! // New recommended way
//! use cache_rs::lfu::LfuMeta;
//! ```

// Re-export from algorithm modules for backward compatibility
pub use crate::gdsf::GdsfMeta;
pub use crate::lfu::LfuMeta;
pub use crate::lfuda::LfudaMeta;
