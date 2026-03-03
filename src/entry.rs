//! Unified Cache Entry Type
//!
//! This module provides a unified `CacheEntry<K, V, M>` structure that can be used
//! across all cache algorithm implementations. The generic `M` parameter allows
//! each algorithm to store its own metadata without affecting the core entry structure.
//!
//! # Design Philosophy
//!
//! The unified entry design provides several benefits:
//! - **Consistency**: All cache algorithms use the same core entry structure
//! - **Extensibility**: Algorithm-specific metadata via the `M` generic parameter
//! - **Timestamps**: Built-in creation and last-accessed timestamps for TTL and monitoring
//! - **Size-awareness**: Explicit size field for dual-limit capacity management
//!
//! # Memory Layout
//!
//! Each entry consists of:
//! - `key: K` - User's key type
//! - `value: V` - User's value type  
//! - `metadata: CacheMetadata<M>` - Contains size, timestamps, and algorithm-specific data
//!
//! `CacheMetadata<M>` contains:
//! - `size: u64` - 8 bytes (content size tracking)
//! - `last_accessed: u64` - 8 bytes (timestamps for monitoring)
//! - `create_time: u64` - 8 bytes (timestamps for TTL)
//! - `algorithm: M` - Algorithm-specific metadata (0-16 bytes depending on algorithm)
//!
//! # Usage Examples
//!
//! ```ignore
//! use cache_rs::entry::{CacheEntry, CacheMetadata};
//!
//! // Simple entry without algorithm-specific metadata (e.g., for LRU)
//! let entry: CacheEntry<String, Vec<u8>> = CacheEntry::new(
//!     "image.png".to_string(),
//!     vec![0u8; 1024],
//!     1024, // size in bytes
//! );
//!
//! // Entry with frequency metadata (e.g., for LFU)
//! use cache_rs::lfu::LfuMeta;
//! let entry = CacheEntry::with_algorithm_metadata(
//!     "key".to_string(),
//!     "value".to_string(),
//!     5, // size
//!     LfuMeta { frequency: 0 },
//! );
//! ```
//!
//! # Thread Safety
//!
//! The single-threaded cache implementations use plain `u64` for timestamps.
//! For concurrent access, wrap the cache in appropriate synchronization primitives,
//! or use the concurrent cache implementations which handle synchronization internally.

extern crate alloc;

use core::fmt;

/// Metadata associated with a cache entry.
///
/// This struct holds common cache metadata (size, timestamps) plus algorithm-specific
/// data via the `M` type parameter. Use `()` for algorithms that don't need extra
/// per-entry metadata (e.g., LRU).
///
/// # Type Parameter
///
/// - `M`: Algorithm-specific metadata type (e.g., `LfuMeta`, `GdsfMeta`)
///
/// # Examples
///
/// ```
/// use cache_rs::entry::CacheMetadata;
///
/// // Metadata without algorithm-specific data (for LRU)
/// let meta: CacheMetadata<()> = CacheMetadata::new(1024);
/// assert_eq!(meta.size, 1024);
/// ```
pub struct CacheMetadata<M = ()> {
    /// Size of content this entry represents (user-provided).
    /// For count-based caches, use 1.
    /// For size-aware caches, use actual bytes (memory, disk, etc.)
    pub size: u64,

    /// Last access timestamp (nanos since epoch or monotonic clock).
    pub last_accessed: u64,

    /// Creation timestamp (nanos since epoch or monotonic clock).
    pub create_time: u64,

    /// Algorithm-specific metadata (frequency, priority, segment, etc.)
    pub algorithm: M,
}

impl<M: Default> CacheMetadata<M> {
    /// Creates new cache metadata with the specified size.
    ///
    /// The algorithm-specific metadata is initialized to its default value.
    ///
    /// # Arguments
    ///
    /// * `size` - Size of the content this entry represents
    #[inline]
    pub fn new(size: u64) -> Self {
        let now = Self::now_nanos();
        Self {
            size,
            last_accessed: now,
            create_time: now,
            algorithm: M::default(),
        }
    }
}

impl<M> CacheMetadata<M> {
    /// Creates new cache metadata with the specified size and algorithm metadata.
    ///
    /// # Arguments
    ///
    /// * `size` - Size of the content this entry represents
    /// * `algorithm` - Algorithm-specific metadata
    #[inline]
    pub fn with_algorithm(size: u64, algorithm: M) -> Self {
        let now = Self::now_nanos();
        Self {
            size,
            last_accessed: now,
            create_time: now,
            algorithm,
        }
    }

    /// Updates the last_accessed timestamp to the current time.
    #[inline]
    pub fn touch(&mut self) {
        self.last_accessed = Self::now_nanos();
    }

    /// Gets the age of this entry in nanoseconds.
    #[inline]
    pub fn age_nanos(&self) -> u64 {
        Self::now_nanos().saturating_sub(self.create_time)
    }

    /// Gets the time since last access in nanoseconds.
    #[inline]
    pub fn idle_nanos(&self) -> u64 {
        Self::now_nanos().saturating_sub(self.last_accessed)
    }

    /// Returns the current time in nanoseconds.
    #[cfg(feature = "std")]
    #[inline]
    fn now_nanos() -> u64 {
        extern crate std;
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }

    /// Returns 0 in no_std environments where system time is not available.
    #[cfg(not(feature = "std"))]
    #[inline]
    fn now_nanos() -> u64 {
        0
    }
}

impl<M: Clone> Clone for CacheMetadata<M> {
    fn clone(&self) -> Self {
        Self {
            size: self.size,
            last_accessed: self.last_accessed,
            create_time: self.create_time,
            algorithm: self.algorithm.clone(),
        }
    }
}

impl<M: fmt::Debug> fmt::Debug for CacheMetadata<M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheMetadata")
            .field("size", &self.size)
            .field("last_accessed", &self.last_accessed)
            .field("create_time", &self.create_time)
            .field("algorithm", &self.algorithm)
            .finish()
    }
}

/// Unified cache entry holding key, value, and metadata.
///
/// The `M` parameter allows each algorithm to store its own metadata
/// without affecting the core entry structure. Use `()` for algorithms
/// that don't need extra per-entry metadata (e.g., LRU).
///
/// # Examples
///
/// ```
/// use cache_rs::entry::CacheEntry;
///
/// // Create a simple entry for count-based caching
/// let entry: CacheEntry<&str, i32> = CacheEntry::new("key", 42, 1);
/// assert_eq!(entry.key, "key");
/// assert_eq!(entry.value, 42);
/// assert_eq!(entry.metadata.size, 1);
/// ```
pub struct CacheEntry<K, V, M = ()> {
    /// The cached key
    pub key: K,

    /// The cached value (or reference/handle to external storage)
    pub value: V,

    /// Cache metadata including size, timestamps, and algorithm-specific data
    pub metadata: CacheMetadata<M>,
}

impl<K, V, M: Default> CacheEntry<K, V, M> {
    /// Creates a new cache entry without algorithm-specific metadata.
    ///
    /// Use this constructor for algorithms like LRU that don't need
    /// per-entry metadata beyond the basic key, value, and timestamps.
    ///
    /// # Arguments
    ///
    /// * `key` - The cache key
    /// * `value` - The cached value
    /// * `size` - Size of the content this entry represents (use 1 for count-based caches)
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::entry::CacheEntry;
    ///
    /// // Count-based cache entry
    /// let entry: CacheEntry<&str, String> = CacheEntry::new("user:123", "Alice".to_string(), 1);
    ///
    /// // Size-aware cache entry
    /// let data = vec![0u8; 1024];
    /// let entry: CacheEntry<&str, Vec<u8>> = CacheEntry::new("file.bin", data, 1024);
    /// ```
    #[inline]
    pub fn new(key: K, value: V, size: u64) -> Self {
        Self {
            key,
            value,
            metadata: CacheMetadata::new(size),
        }
    }
}

impl<K, V, M> CacheEntry<K, V, M> {
    /// Creates a new cache entry with algorithm-specific metadata.
    ///
    /// Use this constructor for algorithms like LFU, LFUDA, SLRU, or GDSF
    /// that need to track additional per-entry state.
    ///
    /// # Arguments
    ///
    /// * `key` - The cache key
    /// * `value` - The cached value
    /// * `size` - Size of the content this entry represents (use 1 for count-based caches)
    /// * `algorithm_meta` - Algorithm-specific metadata
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::entry::CacheEntry;
    /// use cache_rs::lfu::LfuMeta;
    ///
    /// let entry = CacheEntry::with_algorithm_metadata(
    ///     "key".to_string(),
    ///     vec![1, 2, 3],
    ///     3,
    ///     LfuMeta { frequency: 0 },
    /// );
    /// assert_eq!(entry.metadata.algorithm.frequency, 0);
    /// ```
    #[inline]
    pub fn with_algorithm_metadata(key: K, value: V, size: u64, algorithm_meta: M) -> Self {
        Self {
            key,
            value,
            metadata: CacheMetadata::with_algorithm(size, algorithm_meta),
        }
    }

    /// Updates the last_accessed timestamp to the current time.
    #[inline]
    pub fn touch(&mut self) {
        self.metadata.touch();
    }

    /// Gets the age of this entry in nanoseconds.
    #[inline]
    pub fn age_nanos(&self) -> u64 {
        self.metadata.age_nanos()
    }

    /// Gets the time since last access in nanoseconds.
    #[inline]
    pub fn idle_nanos(&self) -> u64 {
        self.metadata.idle_nanos()
    }

    /// Returns the size of the cached content.
    #[inline]
    pub fn size(&self) -> u64 {
        self.metadata.size
    }
}

impl<K: Clone, V: Clone, M: Clone> Clone for CacheEntry<K, V, M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            value: self.value.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

impl<K: fmt::Debug, V: fmt::Debug, M: fmt::Debug> fmt::Debug for CacheEntry<K, V, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheEntry")
            .field("key", &self.key)
            .field("value", &self.value)
            .field("metadata", &self.metadata)
            .finish()
    }
}

// CacheEntry is automatically Send + Sync when K, V, M are Send + Sync
// since all fields (including u64 timestamps) are Send + Sync.

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::format;
    use alloc::vec;

    #[test]
    fn test_new_entry() {
        let entry: CacheEntry<&str, i32> = CacheEntry::new("key", 42, 1);
        assert_eq!(entry.key, "key");
        assert_eq!(entry.value, 42);
        assert_eq!(entry.metadata.size, 1);
    }

    #[test]
    fn test_entry_with_algorithm_metadata() {
        #[derive(Debug, Clone, PartialEq, Default)]
        struct TestMeta {
            frequency: u64,
        }

        let entry =
            CacheEntry::with_algorithm_metadata("key", "value", 5, TestMeta { frequency: 10 });
        assert_eq!(entry.key, "key");
        assert_eq!(entry.value, "value");
        assert_eq!(entry.metadata.size, 5);
        assert_eq!(entry.metadata.algorithm.frequency, 10);
    }

    #[test]
    fn test_touch_updates_last_accessed() {
        let mut entry: CacheEntry<&str, i32> = CacheEntry::new("key", 42, 1);
        let initial = entry.metadata.last_accessed;
        entry.touch();
        let updated = entry.metadata.last_accessed;
        // In no_std mode both will be 0, in std mode updated >= initial
        assert!(updated >= initial);
    }

    #[test]
    fn test_clone_entry() {
        #[derive(Debug, Clone, PartialEq, Default)]
        struct TestMeta {
            value: u64,
        }

        let entry =
            CacheEntry::with_algorithm_metadata("key", vec![1, 2, 3], 3, TestMeta { value: 100 });
        let cloned = entry.clone();

        assert_eq!(cloned.key, entry.key);
        assert_eq!(cloned.value, entry.value);
        assert_eq!(cloned.metadata.size, entry.metadata.size);
        assert_eq!(cloned.metadata.algorithm, entry.metadata.algorithm);
    }

    #[test]
    fn test_age_and_idle() {
        let mut entry: CacheEntry<&str, i32> = CacheEntry::new("key", 42, 1);

        // In no_std mode, timestamps will be 0
        // In std mode, age/idle should be small (just created)
        let _age = entry.age_nanos();
        let _idle = entry.idle_nanos();

        // After touch, idle_nanos() should work without panicking
        entry.touch();
        let _idle_after = entry.idle_nanos();
    }

    #[test]
    fn test_metadata_size() {
        let meta: CacheMetadata<()> = CacheMetadata::new(1024);
        assert_eq!(meta.size, 1024);
    }

    #[test]
    fn test_debug_impl() {
        let entry: CacheEntry<&str, i32> = CacheEntry::new("key", 42, 1);
        let debug_str = format!("{:?}", entry);
        assert!(debug_str.contains("CacheEntry"));
        assert!(debug_str.contains("key"));
        assert!(debug_str.contains("42"));
    }
}
