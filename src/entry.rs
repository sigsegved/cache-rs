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
//! Each entry has the following overhead:
//! - `key: K` - User's key type
//! - `value: V` - User's value type  
//! - `size: u64` - 8 bytes (content size tracking)
//! - `last_accessed: u64` - 8 bytes (timestamps for monitoring)
//! - `create_time: u64` - 8 bytes (timestamps for TTL)
//! - `metadata: Option<M>` - 0-24 bytes depending on algorithm
//!
//! Base overhead (all algorithms): ~24 bytes + key + value + optional metadata
//!
//! # Usage Examples
//!
//! ```ignore
//! use cache_rs::entry::CacheEntry;
//!
//! // Simple entry without algorithm-specific metadata (e.g., for LRU)
//! let entry: CacheEntry<String, Vec<u8>, ()> = CacheEntry::new(
//!     "image.png".to_string(),
//!     vec![0u8; 1024],
//!     1024, // size in bytes
//! );
//!
//! // Entry with frequency metadata (e.g., for LFU)
//! use cache_rs::meta::LfuMeta;
//! let entry = CacheEntry::with_metadata(
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

/// Unified cache entry holding key, value, timestamps, and algorithm-specific metadata.
///
/// The `M` parameter allows each algorithm to store its own metadata
/// without affecting the core entry structure. Use `()` for algorithms
/// that don't need extra per-entry metadata (e.g., LRU).
///
/// # Design Decisions
///
/// - `size`: User-provided size of content this entry represents. Could be
///   memory bytes, disk bytes, or any unit. Use 1 for count-based caches.
/// - `last_accessed`: Atomic for lock-free monitoring and metrics. Can be
///   updated during reads without requiring a write lock.
/// - `create_time`: Atomic for consistency. Useful for TTL, debugging, metrics.
/// - `metadata`: Optional algorithm-specific data. `None` for simple algorithms
///   like LRU that don't need extra per-entry state.
///
/// # Examples
///
/// ```
/// use cache_rs::entry::CacheEntry;
///
/// // Create a simple entry for count-based caching
/// let entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);
/// assert_eq!(entry.key, "key");
/// assert_eq!(entry.value, 42);
/// assert_eq!(entry.size, 1);
/// ```
pub struct CacheEntry<K, V, M = ()> {
    /// The cached key
    pub key: K,

    /// The cached value (or reference/handle to external storage)
    pub value: V,

    /// Size of content this entry represents (user-provided).
    /// For count-based caches, use 1.
    /// For size-aware caches, use actual bytes (memory, disk, etc.)
    pub size: u64,

    /// Last access timestamp (nanos since epoch or monotonic clock).
    last_accessed: u64,

    /// Creation timestamp (nanos since epoch or monotonic clock).
    create_time: u64,

    /// Algorithm-specific metadata (frequency, priority, segment, etc.).
    /// `None` for algorithms that don't need per-entry metadata (e.g., LRU).
    pub metadata: Option<M>,
}

impl<K, V, M> CacheEntry<K, V, M> {
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
    /// let entry: CacheEntry<&str, String, ()> = CacheEntry::new("user:123", "Alice".to_string(), 1);
    ///
    /// // Size-aware cache entry
    /// let data = vec![0u8; 1024];
    /// let entry: CacheEntry<&str, Vec<u8>, ()> = CacheEntry::new("file.bin", data, 1024);
    /// ```
    #[inline]
    pub fn new(key: K, value: V, size: u64) -> Self {
        let now = Self::now_nanos();
        Self {
            key,
            value,
            size,
            last_accessed: now,
            create_time: now,
            metadata: None,
        }
    }

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
    /// * `metadata` - Algorithm-specific metadata
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::entry::CacheEntry;
    /// use cache_rs::meta::LfuMeta;
    ///
    /// let entry = CacheEntry::with_metadata(
    ///     "key".to_string(),
    ///     vec![1, 2, 3],
    ///     3,
    ///     LfuMeta { frequency: 0 },
    /// );
    /// assert!(entry.metadata.is_some());
    /// ```
    #[inline]
    pub fn with_metadata(key: K, value: V, size: u64, metadata: M) -> Self {
        let now = Self::now_nanos();
        Self {
            key,
            value,
            size,
            last_accessed: now,
            create_time: now,
            metadata: Some(metadata),
        }
    }

    /// Updates the last_accessed timestamp to the current time.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::entry::CacheEntry;
    ///
    /// let mut entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);
    /// let old_access_time = entry.last_accessed();
    ///
    /// // Simulate some time passing (in real code, time would actually pass)
    /// entry.touch();
    ///
    /// // On systems with std, the access time would be updated
    /// ```
    #[inline]
    pub fn touch(&mut self) {
        self.last_accessed = Self::now_nanos();
    }

    /// Gets the last_accessed timestamp in nanoseconds.
    ///
    /// Returns nanoseconds since UNIX epoch when the `std` feature is enabled,
    /// or 0 in `no_std` environments.
    #[inline]
    pub fn last_accessed(&self) -> u64 {
        self.last_accessed
    }

    /// Gets the creation timestamp in nanoseconds.
    ///
    /// Returns nanoseconds since UNIX epoch when the `std` feature is enabled,
    /// or 0 in `no_std` environments.
    #[inline]
    pub fn create_time(&self) -> u64 {
        self.create_time
    }

    /// Gets the age of this entry in nanoseconds.
    ///
    /// Age is calculated as `now - create_time`. Returns 0 in `no_std`
    /// environments where time is not available.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::entry::CacheEntry;
    ///
    /// let entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);
    /// let age = entry.age_nanos();
    /// // Age should be very small since we just created the entry
    /// ```
    #[inline]
    pub fn age_nanos(&self) -> u64 {
        Self::now_nanos().saturating_sub(self.create_time())
    }

    /// Gets the time since last access in nanoseconds.
    ///
    /// Idle time is calculated as `now - last_accessed`. Returns 0 in `no_std`
    /// environments where time is not available.
    ///
    /// # Examples
    ///
    /// ```
    /// use cache_rs::entry::CacheEntry;
    ///
    /// let entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);
    /// let idle = entry.idle_nanos();
    /// // Idle time should be very small since we just created the entry
    /// ```
    #[inline]
    pub fn idle_nanos(&self) -> u64 {
        Self::now_nanos().saturating_sub(self.last_accessed())
    }

    /// Returns a mutable reference to the metadata.
    ///
    /// Returns `None` if the entry was created without metadata.
    #[inline]
    pub fn metadata_mut(&mut self) -> Option<&mut M> {
        self.metadata.as_mut()
    }

    /// Returns the current time in nanoseconds.
    ///
    /// With the `std` feature enabled, returns nanoseconds since UNIX epoch.
    /// In `no_std` environments, returns 0 (users can manually set timestamps).
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
    ///
    /// Users can manually track and set timestamps in no_std contexts by
    /// directly accessing the atomic fields if needed.
    #[cfg(not(feature = "std"))]
    #[inline]
    fn now_nanos() -> u64 {
        0 // No clock in no_std; users can call touch() with custom time
    }
}

impl<K: Clone, V: Clone, M: Clone> Clone for CacheEntry<K, V, M> {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            value: self.value.clone(),
            size: self.size,
            last_accessed: self.last_accessed,
            create_time: self.create_time,
            metadata: self.metadata.clone(),
        }
    }
}

impl<K: fmt::Debug, V: fmt::Debug, M: fmt::Debug> fmt::Debug for CacheEntry<K, V, M> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheEntry")
            .field("key", &self.key)
            .field("value", &self.value)
            .field("size", &self.size)
            .field("last_accessed", &self.last_accessed)
            .field("create_time", &self.create_time)
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
        let entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);
        assert_eq!(entry.key, "key");
        assert_eq!(entry.value, 42);
        assert_eq!(entry.size, 1);
        assert!(entry.metadata.is_none());
    }

    #[test]
    fn test_entry_with_metadata() {
        #[derive(Debug, Clone, PartialEq)]
        struct TestMeta {
            frequency: u64,
        }

        let entry = CacheEntry::with_metadata("key", "value", 5, TestMeta { frequency: 10 });
        assert_eq!(entry.key, "key");
        assert_eq!(entry.value, "value");
        assert_eq!(entry.size, 5);
        assert!(entry.metadata.is_some());
        assert_eq!(entry.metadata.unwrap().frequency, 10);
    }

    #[test]
    fn test_touch_updates_last_accessed() {
        let mut entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);
        let initial = entry.last_accessed();
        entry.touch();
        let updated = entry.last_accessed();
        // In no_std mode both will be 0, in std mode updated >= initial
        assert!(updated >= initial);
    }

    #[test]
    fn test_clone_entry() {
        #[derive(Debug, Clone, PartialEq)]
        struct TestMeta {
            value: u64,
        }

        let entry = CacheEntry::with_metadata("key", vec![1, 2, 3], 3, TestMeta { value: 100 });
        let cloned = entry.clone();

        assert_eq!(cloned.key, entry.key);
        assert_eq!(cloned.value, entry.value);
        assert_eq!(cloned.size, entry.size);
        assert_eq!(cloned.metadata, entry.metadata);
    }

    #[test]
    fn test_age_and_idle() {
        let mut entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);

        // In no_std mode, timestamps will be 0
        // In std mode, age/idle should be small (just created)
        let _age = entry.age_nanos();
        let _idle = entry.idle_nanos();

        // After touch, idle_nanos() should work without panicking
        entry.touch();
        let _idle_after = entry.idle_nanos();
    }

    #[test]
    fn test_metadata_mut() {
        #[derive(Debug, Clone)]
        struct TestMeta {
            counter: u64,
        }

        let mut entry = CacheEntry::with_metadata("key", "value", 1, TestMeta { counter: 0 });

        if let Some(meta) = entry.metadata_mut() {
            meta.counter += 1;
        }

        assert_eq!(entry.metadata.as_ref().unwrap().counter, 1);
    }

    #[test]
    fn test_entry_without_metadata_returns_none() {
        let mut entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);
        assert!(entry.metadata_mut().is_none());
    }

    #[test]
    fn test_debug_impl() {
        let entry: CacheEntry<&str, i32, ()> = CacheEntry::new("key", 42, 1);
        let debug_str = format!("{:?}", entry);
        assert!(debug_str.contains("CacheEntry"));
        assert!(debug_str.contains("key"));
        assert!(debug_str.contains("42"));
    }
}
