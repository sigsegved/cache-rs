//! Algorithm-Specific Metadata Types
//!
//! This module provides metadata types used by different cache algorithms.
//! Each cache algorithm that needs per-entry metadata defines its own type here,
//! which can be used with `CacheEntry<K, V, M>`.
//!
//! # Metadata Types
//!
//! | Algorithm | Metadata Type | Description |
//! |-----------|---------------|-------------|
//! | LRU       | `()` (none)   | Position in list is implicit |
//! | LFU       | `LfuMeta`     | Access frequency counter |
//! | LFUDA     | `LfudaMeta`   | Access frequency (age is cache-global) |
//! | SLRU      | `SlruMeta`    | Segment location (probationary/protected) |
//! | GDSF      | `GdsfMeta`    | Frequency and calculated priority |
//!
//! # Memory Overhead
//!
//! | Metadata Type | Size |
//! |---------------|------|
//! | `()`          | 0 bytes |
//! | `LfuMeta`     | 8 bytes |
//! | `LfudaMeta`   | 8 bytes |
//! | `SlruMeta`    | 1 byte (+ padding) |
//! | `GdsfMeta`    | 16 bytes |
//!
//! # Usage
//!
//! ```
//! use cache_rs::meta::{LfuMeta, SlruSegment, SlruMeta, GdsfMeta};
//!
//! // LFU metadata with initial frequency
//! let lfu_meta = LfuMeta::default();
//! assert_eq!(lfu_meta.frequency, 0);
//!
//! // SLRU metadata for probationary segment
//! let slru_meta = SlruMeta::new(SlruSegment::Probationary);
//! assert_eq!(slru_meta.segment, SlruSegment::Probationary);
//!
//! // GDSF metadata with initial values
//! let gdsf_meta = GdsfMeta::default();
//! assert_eq!(gdsf_meta.frequency, 0);
//! assert_eq!(gdsf_meta.priority, 0.0);
//! ```

/// Metadata for LFU (Least Frequently Used) cache entries.
///
/// LFU tracks access frequency to evict the least frequently accessed items.
/// The frequency counter is incremented on each access.
///
/// # Examples
///
/// ```
/// use cache_rs::meta::LfuMeta;
///
/// let mut meta = LfuMeta::default();
/// assert_eq!(meta.frequency, 0);
///
/// // Simulate access
/// meta.frequency += 1;
/// assert_eq!(meta.frequency, 1);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LfuMeta {
    /// Access frequency count.
    /// Incremented each time the entry is accessed.
    pub frequency: u64,
}

impl LfuMeta {
    /// Creates a new LFU metadata with the specified initial frequency.
    ///
    /// # Arguments
    ///
    /// * `frequency` - Initial frequency value (usually 0 or 1)
    #[inline]
    pub fn new(frequency: u64) -> Self {
        Self { frequency }
    }

    /// Increments the frequency counter and returns the new value.
    #[inline]
    pub fn increment(&mut self) -> u64 {
        self.frequency += 1;
        self.frequency
    }
}

/// Metadata for LFUDA (LFU with Dynamic Aging) cache entries.
///
/// LFUDA is similar to LFU but addresses the "aging problem" where old
/// frequently-used items can prevent new items from being cached.
/// The age factor is maintained at the cache level, not per-entry.
///
/// # Algorithm
///
/// Entry priority = frequency + age_at_insertion
/// - When an item is evicted, global_age = evicted_item.priority
/// - New items start with current global_age as their insertion age
///
/// # Examples
///
/// ```
/// use cache_rs::meta::LfudaMeta;
///
/// let meta = LfudaMeta::new(1, 10); // frequency=1, age_at_insertion=10
/// assert_eq!(meta.frequency, 1);
/// assert_eq!(meta.age_at_insertion, 10);
/// assert_eq!(meta.priority(), 11);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LfudaMeta {
    /// Access frequency count.
    pub frequency: u64,
    /// Age value when this item was inserted (snapshot of global_age).
    pub age_at_insertion: u64,
}

impl LfudaMeta {
    /// Creates a new LFUDA metadata with the specified initial frequency and age.
    ///
    /// # Arguments
    ///
    /// * `frequency` - Initial frequency value (usually 1 for new items)
    /// * `age_at_insertion` - The global_age at the time of insertion
    #[inline]
    pub fn new(frequency: u64, age_at_insertion: u64) -> Self {
        Self {
            frequency,
            age_at_insertion,
        }
    }

    /// Increments the frequency counter and returns the new value.
    #[inline]
    pub fn increment(&mut self) -> u64 {
        self.frequency += 1;
        self.frequency
    }

    /// Calculates the effective priority (frequency + age_at_insertion).
    #[inline]
    pub fn priority(&self) -> u64 {
        self.frequency + self.age_at_insertion
    }
}

/// Segment location within an SLRU cache.
///
/// SLRU divides the cache into two segments:
/// - **Probationary**: New entries start here
/// - **Protected**: Entries promoted after multiple accesses
///
/// Items in the protected segment are shielded from one-time scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlruSegment {
    /// Entry is in the probationary segment (initial placement).
    /// Items here are candidates for eviction when the cache is full.
    #[default]
    Probationary,

    /// Entry is in the protected segment (promoted after repeated access).
    /// Items here are protected from eviction by one-time scans.
    Protected,
}

/// Metadata for SLRU (Segmented LRU) cache entries.
///
/// SLRU uses two segments to provide scan resistance:
/// - New items enter the probationary segment
/// - Accessed items in probationary are promoted to protected
/// - When protected is full, LRU items demote back to probationary
///
/// # Examples
///
/// ```
/// use cache_rs::meta::{SlruMeta, SlruSegment};
///
/// // New entry starts in probationary
/// let meta = SlruMeta::default();
/// assert_eq!(meta.segment, SlruSegment::Probationary);
///
/// // After promotion
/// let protected_meta = SlruMeta::new(SlruSegment::Protected);
/// assert_eq!(protected_meta.segment, SlruSegment::Protected);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SlruMeta {
    /// Which segment this entry is currently in.
    pub segment: SlruSegment,
}

impl SlruMeta {
    /// Creates a new SLRU metadata with the specified segment.
    ///
    /// # Arguments
    ///
    /// * `segment` - The segment where this entry should be placed
    #[inline]
    pub fn new(segment: SlruSegment) -> Self {
        Self { segment }
    }

    /// Creates metadata for a probationary segment entry.
    #[inline]
    pub fn probationary() -> Self {
        Self {
            segment: SlruSegment::Probationary,
        }
    }

    /// Creates metadata for a protected segment entry.
    #[inline]
    pub fn protected() -> Self {
        Self {
            segment: SlruSegment::Protected,
        }
    }

    /// Returns true if the entry is in the probationary segment.
    #[inline]
    pub fn is_probationary(&self) -> bool {
        self.segment == SlruSegment::Probationary
    }

    /// Returns true if the entry is in the protected segment.
    #[inline]
    pub fn is_protected(&self) -> bool {
        self.segment == SlruSegment::Protected
    }

    /// Promotes the entry to the protected segment.
    #[inline]
    pub fn promote(&mut self) {
        self.segment = SlruSegment::Protected;
    }

    /// Demotes the entry to the probationary segment.
    #[inline]
    pub fn demote(&mut self) {
        self.segment = SlruSegment::Probationary;
    }
}

/// Metadata for GDSF (Greedy Dual-Size Frequency) cache entries.
///
/// GDSF is a sophisticated algorithm that considers:
/// - **Frequency**: How often the item is accessed
/// - **Size**: Larger items have lower priority per byte
/// - **Aging**: Global clock advances when items are evicted
///
/// # Priority Calculation
///
/// ```text
/// priority = (frequency / size) + global_age
/// ```
///
/// Items with lower priority are evicted first.
///
/// # Examples
///
/// ```
/// use cache_rs::meta::GdsfMeta;
///
/// let meta = GdsfMeta::new(1, 0.5); // frequency=1, priority=0.5
/// assert_eq!(meta.frequency, 1);
/// assert_eq!(meta.priority, 0.5);
/// ```
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct GdsfMeta {
    /// Access frequency count.
    pub frequency: u64,

    /// Calculated priority: (frequency / size) + clock.
    /// Lower priority = more likely to be evicted.
    pub priority: f64,
}

impl GdsfMeta {
    /// Creates a new GDSF metadata with the specified frequency and priority.
    ///
    /// # Arguments
    ///
    /// * `frequency` - Initial frequency count
    /// * `priority` - Calculated priority value
    #[inline]
    pub fn new(frequency: u64, priority: f64) -> Self {
        Self {
            frequency,
            priority,
        }
    }

    /// Increments the frequency counter and returns the new value.
    #[inline]
    pub fn increment(&mut self) -> u64 {
        self.frequency += 1;
        self.frequency
    }

    /// Calculates and updates the priority based on frequency, size, and global age.
    ///
    /// # Arguments
    ///
    /// * `size` - Size of the cached item
    /// * `global_age` - Current global age (clock value) of the cache
    ///
    /// # Returns
    ///
    /// The newly calculated priority value.
    #[inline]
    pub fn calculate_priority(&mut self, size: u64, global_age: f64) -> f64 {
        self.priority = if size == 0 {
            f64::INFINITY
        } else {
            (self.frequency as f64 / size as f64) + global_age
        };
        self.priority
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::format;

    #[test]
    fn test_lfu_meta_default() {
        let meta = LfuMeta::default();
        assert_eq!(meta.frequency, 0);
    }

    #[test]
    fn test_lfu_meta_new() {
        let meta = LfuMeta::new(5);
        assert_eq!(meta.frequency, 5);
    }

    #[test]
    fn test_lfu_meta_increment() {
        let mut meta = LfuMeta::new(0);
        assert_eq!(meta.increment(), 1);
        assert_eq!(meta.increment(), 2);
        assert_eq!(meta.frequency, 2);
    }

    #[test]
    fn test_lfuda_meta_default() {
        let meta = LfudaMeta::default();
        assert_eq!(meta.frequency, 0);
        assert_eq!(meta.age_at_insertion, 0);
    }

    #[test]
    fn test_lfuda_meta_increment() {
        let mut meta = LfudaMeta::new(10, 5);
        assert_eq!(meta.frequency, 10);
        assert_eq!(meta.age_at_insertion, 5);
        assert_eq!(meta.priority(), 15);
        assert_eq!(meta.increment(), 11);
        assert_eq!(meta.priority(), 16);
    }

    #[test]
    fn test_slru_segment_default() {
        let segment = SlruSegment::default();
        assert_eq!(segment, SlruSegment::Probationary);
    }

    #[test]
    fn test_slru_meta_default() {
        let meta = SlruMeta::default();
        assert_eq!(meta.segment, SlruSegment::Probationary);
        assert!(meta.is_probationary());
        assert!(!meta.is_protected());
    }

    #[test]
    fn test_slru_meta_promote_demote() {
        let mut meta = SlruMeta::probationary();
        assert!(meta.is_probationary());

        meta.promote();
        assert!(meta.is_protected());

        meta.demote();
        assert!(meta.is_probationary());
    }

    #[test]
    fn test_slru_meta_constructors() {
        let prob = SlruMeta::probationary();
        assert_eq!(prob.segment, SlruSegment::Probationary);

        let prot = SlruMeta::protected();
        assert_eq!(prot.segment, SlruSegment::Protected);
    }

    #[test]
    fn test_gdsf_meta_default() {
        let meta = GdsfMeta::default();
        assert_eq!(meta.frequency, 0);
        assert_eq!(meta.priority, 0.0);
    }

    #[test]
    fn test_gdsf_meta_new() {
        let meta = GdsfMeta::new(5, 1.5);
        assert_eq!(meta.frequency, 5);
        assert_eq!(meta.priority, 1.5);
    }

    #[test]
    fn test_gdsf_meta_increment() {
        let mut meta = GdsfMeta::new(0, 0.0);
        assert_eq!(meta.increment(), 1);
        assert_eq!(meta.frequency, 1);
    }

    #[test]
    fn test_gdsf_meta_calculate_priority() {
        let mut meta = GdsfMeta::new(4, 0.0);
        let global_age = 10.0;

        // priority = frequency/size + global_age = 4/2 + 10 = 12
        let priority = meta.calculate_priority(2, global_age);
        assert_eq!(priority, 12.0);
        assert_eq!(meta.priority, 12.0);
    }

    #[test]
    fn test_gdsf_meta_calculate_priority_zero_size() {
        let mut meta = GdsfMeta::new(4, 0.0);
        let priority = meta.calculate_priority(0, 10.0);
        assert!(priority.is_infinite());
    }

    #[test]
    fn test_metadata_clone() {
        let lfu = LfuMeta::new(5);
        let cloned = lfu;
        assert_eq!(lfu, cloned);

        let gdsf = GdsfMeta::new(3, 1.5);
        let cloned = gdsf;
        assert_eq!(gdsf.frequency, cloned.frequency);
        assert_eq!(gdsf.priority, cloned.priority);
    }

    #[test]
    fn test_metadata_debug() {
        let lfu = LfuMeta::new(5);
        let debug_str = format!("{:?}", lfu);
        assert!(debug_str.contains("LfuMeta"));
        assert!(debug_str.contains("5"));

        let slru = SlruMeta::protected();
        let debug_str = format!("{:?}", slru);
        assert!(debug_str.contains("SlruMeta"));
        assert!(debug_str.contains("Protected"));
    }
}
