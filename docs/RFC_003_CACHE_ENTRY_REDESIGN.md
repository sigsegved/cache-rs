# RFC cache-rs-003: Unified CacheEntry Design

**Status**: Draft  
**Created**: January 25, 2026  
**Supersedes**: RFC cache-rs-002 (size_aware_cache.md)

---

## Summary

Replace algorithm-specific entry structures with a unified `CacheEntry<K, V, M>` design where `M` is algorithm-specific metadata. Add dual-limit capacity management (`max_entries` + `max_size`) with explicit size parameter on `put()`.

## Motivation

The current codebase has:
- Each algorithm implements its own entry handling
- No unified way to add cross-cutting features (TTL, timestamps, size)
- Size-aware caching requires breaking API changes (GDSF's `put(k, v, size)`)
- No separation between cache-rs memory usage and content storage limits

This RFC provides:
- **Simplicity**: One entry type, extensible via generics
- **Explicit sizing**: `put(key, value, size)` - no magic, no traits
- **Dual limits**: Separate bounds for cache-rs memory and content storage
- **Production-ready**: Built-in timestamps for TTL, metrics, debugging
- **Generalized model**: Works for in-memory, on-disk, arena, or any storage

---

## Storage Model

cache-rs manages **cache entries** (metadata). The actual **content** can be:
- **Inline**: `V` is the actual data (in-memory cache)
- **Reference**: `V` is `Arc<T>`, `Box<T>` (shared/heap allocation)
- **Handle**: `V` is `PathBuf`, `FileHandle` (on-disk storage)
- **Index**: `V` is `SlabIndex`, `ArenaHandle` (external allocator)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         cache-rs                                        │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │  Cache Entries (bounded by max_entries)                           │  │
│  │  Memory: O(max_entries) × ~150 bytes per entry                    │  │
│  │                                                                   │  │
│  │  Entry { key, value, size, timestamps, metadata }                 │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                                                         │
│  Tracks: current_size (sum of entry.size, bounded by max_size)          │
└─────────────────────────────────────────────────────────────────────────┘
                               │
                               ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                    Content Storage (User's Responsibility)              │
│                                                                         │
│  - Inline data, heap allocations, disk files, arena slots, etc.         │
│  - Bounded by: max_size (sum of entry.size values)                      │
│  - Cleanup on eviction: User handles via returned evicted value         │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Design

### 1. Core CacheEntry

```rust
use core::sync::atomic::{AtomicU64, Ordering};

/// Unified cache entry holding key, value, timestamps, and algorithm-specific metadata.
/// 
/// The `M` parameter allows each algorithm to store its own metadata
/// without affecting the core entry structure. Use `Option<M>` to allow
/// algorithms that don't need extra metadata to avoid allocation.
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
#[derive(Debug)]
pub struct CacheEntry<K, V, M = ()> {
    /// The cached key
    pub key: K,
    
    /// The cached value (or reference/handle to external storage)
    pub value: V,
    
    /// Size of content this entry represents (user-provided)
    /// For count-based caches, use 1
    /// For size-aware caches, use actual bytes (memory, disk, etc.)
    pub size: u64,
    
    /// Last access timestamp (nanos since epoch or monotonic clock)
    /// Atomic to allow lock-free updates during get()
    pub last_accessed: AtomicU64,
    
    /// Creation timestamp (nanos since epoch or monotonic clock)
    /// Atomic for consistency with last_accessed
    pub create_time: AtomicU64,
    
    /// Algorithm-specific metadata (frequency, priority, etc.)
    /// None for algorithms that don't need per-entry metadata (e.g., LRU)
    pub metadata: Option<M>,
}

impl<K, V, M> CacheEntry<K, V, M> {
    /// Create a new cache entry without metadata
    #[inline]
    pub fn new(key: K, value: V, size: u64) -> Self {
        let now = Self::now_nanos();
        Self {
            key,
            value,
            size,
            last_accessed: AtomicU64::new(now),
            create_time: AtomicU64::new(now),
            metadata: None,
        }
    }
    
    /// Create a new cache entry with algorithm-specific metadata
    #[inline]
    pub fn with_metadata(key: K, value: V, size: u64, metadata: M) -> Self {
        let now = Self::now_nanos();
        Self {
            key,
            value,
            size,
            last_accessed: AtomicU64::new(now),
            create_time: AtomicU64::new(now),
            metadata: Some(metadata),
        }
    }
    
    /// Update last_accessed timestamp (lock-free)
    #[inline]
    pub fn touch(&self) {
        self.last_accessed.store(Self::now_nanos(), Ordering::Relaxed);
    }
    
    /// Get last_accessed timestamp
    #[inline]
    pub fn last_accessed(&self) -> u64 {
        self.last_accessed.load(Ordering::Relaxed)
    }
    
    /// Get creation timestamp
    #[inline]
    pub fn create_time(&self) -> u64 {
        self.create_time.load(Ordering::Relaxed)
    }
    
    /// Get age in nanoseconds
    #[inline]
    pub fn age_nanos(&self) -> u64 {
        Self::now_nanos().saturating_sub(self.create_time())
    }
    
    /// Get time since last access in nanoseconds
    #[inline]
    pub fn idle_nanos(&self) -> u64 {
        Self::now_nanos().saturating_sub(self.last_accessed())
    }
    
    #[cfg(feature = "std")]
    #[inline]
    fn now_nanos() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
    
    #[cfg(not(feature = "std"))]
    #[inline]
    fn now_nanos() -> u64 {
        0  // No clock in no_std; users can call touch() with custom time
    }
}
```

### 2. Algorithm-Specific Metadata

```rust
// ============================================================================
// LRU: No additional metadata needed
// ============================================================================
// LRU uses `metadata: None` - position in list is implicit

// ============================================================================
// LFU: Frequency counter
// ============================================================================
#[derive(Debug, Clone, Default)]
pub struct LfuMeta {
    /// Access frequency count
    pub frequency: u64,
}

// ============================================================================
// LFUDA: Frequency (age factor is cache-global, not per-entry)
// ============================================================================
#[derive(Debug, Clone, Default)]
pub struct LfudaMeta {
    /// Access frequency count
    pub frequency: u64,
}

// ============================================================================
// SLRU: Segment location
// ============================================================================
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlruSegment {
    #[default]
    Probationary,
    Protected,
}

#[derive(Debug, Clone, Default)]
pub struct SlruMeta {
    /// Which segment this entry is in
    pub segment: SlruSegment,
}

// ============================================================================
// GDSF: Priority and frequency for size-aware caching
// ============================================================================
#[derive(Debug, Clone, Default)]
pub struct GdsfMeta {
    /// Access frequency count
    pub frequency: u64,
    /// Calculated priority: (frequency / size) + clock
    pub priority: f64,
}
```

### 3. Dual-Limit Capacity Management

cache-rs enforces **two independent limits**:

1. **`max_entries`**: Bounds cache-rs memory usage (~150 bytes per entry)
2. **`max_size`**: Bounds content storage (sum of `entry.size` values)

Eviction occurs when **either** limit would be exceeded.

```rust
impl<K, V, S> Lru<K, V, S> {
    fn needs_eviction(&self, new_size: u64) -> bool {
        // Evict if EITHER constraint would be violated
        self.len() as u64 >= self.max_entries 
            || self.current_size + new_size > self.max_size
    }
}
```

**Configuration responsibility:**

| Scenario | Result | Whose responsibility? |
|----------|--------|----------------------|
| `max_entries=1M`, `max_size=100GB` | Will hit `max_size` first | ✅ Good config |
| `max_entries=1K`, `max_size=100GB` | Will always hit `max_entries` | ⚠️ User misconfiguration |

The library honors both limits correctly. Choosing sensible values is the user's job.

### 4. Cache Type Signatures (Simplified)

**No Sizer trait** - size is passed explicitly to `put()`.

```rust

// ============================================================================
// Single-threaded caches
// ============================================================================

/// LRU cache - simple, count-based or size-based
/// No per-entry metadata needed (position in list is implicit)
pub struct Lru<K, V, S = DefaultHashBuilder> {
    map: HashMap<K, *mut Entry<CacheEntry<K, V, ()>>, S>,
    list: List<CacheEntry<K, V, ()>>,
    
    /// Bounds cache-rs memory usage
    max_entries: u64,
    
    /// Bounds content storage (sum of entry.size)
    current_size: u64,
    max_size: u64,
}

/// LFU cache - frequency-based eviction
pub struct Lfu<K, V, S = DefaultHashBuilder> {
    map: HashMap<K, *mut Entry<CacheEntry<K, V, LfuMeta>>, S>,
    frequency_lists: BTreeMap<u64, List<CacheEntry<K, V, LfuMeta>>>,
    max_entries: u64,
    current_size: u64,
    max_size: u64,
    min_frequency: u64,
}

/// SLRU cache - two-segment LRU with promotion
pub struct Slru<K, V, S = DefaultHashBuilder> {
    map: HashMap<K, *mut Entry<CacheEntry<K, V, SlruMeta>>, S>,
    probationary: List<CacheEntry<K, V, SlruMeta>>,
    protected: List<CacheEntry<K, V, SlruMeta>>,
    max_entries: u64,
    current_size: u64,
    max_size: u64,
    protected_ratio: f32,
}

/// GDSF cache - size-aware frequency caching
pub struct Gdsf<K, V, S = DefaultHashBuilder> {
    map: HashMap<K, *mut Entry<CacheEntry<K, V, GdsfMeta>>, S>,
    priority_lists: BTreeMap<u64, List<CacheEntry<K, V, GdsfMeta>>>,
    max_entries: u64,
    current_size: u64,
    max_size: u64,
    clock: f64,
}

// ============================================================================
// Type aliases for convenience
// ============================================================================

// Concurrent versions (sharded)
pub type ConcurrentLru<K, V> = ShardedCache<Lru<K, V>>;
pub type ConcurrentGdsf<K, V> = ShardedCache<Gdsf<K, V>>;
```

### 5. API Design

```rust
impl<K, V, S> Lru<K, V, S>
where
    K: Hash + Eq + Clone,
    S: BuildHasher,
{
    // ========================================================================
    // Constructors
    // ========================================================================
    
    /// Create a count-based LRU cache (max entries only).
    /// 
    /// Equivalent to `with_limits(max_entries, u64::MAX)`.
    /// Each `put()` should use `size=1`.
    /// 
    /// # Example
    /// ```
    /// let mut cache: Lru<String, User> = Lru::new(1000);
    /// cache.put("alice".into(), user, 1);
    /// ```
    pub fn new(max_entries: u64) -> Lru<K, V, DefaultHashBuilder> {
        Lru::with_limits(max_entries, u64::MAX)
    }
    
    /// Create a size-based LRU cache (max size only).
    /// 
    /// Equivalent to `with_limits(u64::MAX, max_size)`.
    /// Useful for in-memory caches bounded by total memory.
    /// 
    /// # Example
    /// ```
    /// let mut cache: Lru<String, Vec<u8>> = Lru::with_max_size(10 * 1024 * 1024);
    /// cache.put("image.png".into(), bytes.clone(), bytes.len() as u64);
    /// ```
    pub fn with_max_size(max_size: u64) -> Lru<K, V, DefaultHashBuilder> {
        Lru::with_limits(u64::MAX, max_size)
    }
    
    /// Create a dual-limit LRU cache.
    /// 
    /// Evicts when EITHER limit would be exceeded:
    /// - `max_entries`: bounds cache-rs memory (~150 bytes per entry)
    /// - `max_size`: bounds content storage (sum of `size` params)
    /// 
    /// # Example (disk cache index)
    /// ```
    /// // 5M entries (~750MB RAM for index), 100GB disk
    /// let mut cache = Lru::with_limits(5_000_000, 100 * 1024 * 1024 * 1024);
    /// cache.put(url, FileMeta { path, etag }, file_size_on_disk);
    /// ```
    pub fn with_limits(max_entries: u64, max_size: u64) -> Lru<K, V, DefaultHashBuilder> {
        Lru::with_limits_and_hasher(max_entries, max_size, DefaultHashBuilder::default())
    }
    
    pub fn with_limits_and_hasher(max_entries: u64, max_size: u64, hasher: S) -> Self {
        Self {
            map: HashMap::with_hasher(hasher),
            list: List::new(),
            max_entries,
            current_size: 0,
            max_size,
        }
    }
    
    // ========================================================================
    // Core operations
    // ========================================================================
    
    /// Get a reference to a cached value.
    /// 
    /// Updates the entry's position in the LRU list (most recently used).
    /// Also updates `last_accessed` timestamp atomically.
    pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let node = *self.map.get(key)?;
        
        // SAFETY: node is valid pointer from our map
        unsafe {
            // Update timestamp (atomic, no lock needed for this)
            (*node).value.touch();
            
            // Move to front (requires &mut self)
            self.list.move_to_front(node);
            Some(&(*node).value.value)
        }
    }
    
    /// Insert a key-value pair into the cache with explicit size.
    /// 
    /// The `size` parameter specifies how much of `max_size` this entry consumes.
    /// Use `size=1` for count-based caches.
    /// 
    /// Returns the old value if the key was already present.
    /// May evict entries to make room for the new entry.
    pub fn put(&mut self, key: K, value: V, size: u64) -> Option<V> {
        // If key exists, update in place
        if let Some(&node) = self.map.get(&key) {
            unsafe {
                let entry = &mut (*node).value;
                let old_size = entry.size;
                let old_value = core::mem::replace(&mut entry.value, value);
                entry.size = size;
                entry.touch();  // Update last_accessed
                
                self.current_size = self.current_size - old_size + size;
                self.list.move_to_front(node);
                
                return Some(old_value);
            }
        }
        
        // Evict until we have room (check BOTH limits)
        while self.needs_eviction(size) && !self.is_empty() {
            self.evict_lru();
        }
        
        // Insert new entry
        let entry = CacheEntry::new(key.clone(), value, size);
        let node = self.list.push_front(entry);
        self.map.insert(key, node);
        self.current_size += size;
        
        None
    }
    
    /// Remove a key from the cache.
    /// 
    /// Returns the removed value if present.
    /// For external storage, caller is responsible for cleanup.
    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq + ?Sized,
    {
        let node = self.map.remove(key)?;
        
        unsafe {
            let entry = self.list.remove(node)?;
            self.current_size -= entry.value.size;
            Some(entry.value.value)
        }
    }
    
    // ========================================================================
    // Capacity management
    // ========================================================================
    
    /// Check if eviction is needed to insert an entry of given size.
    #[inline]
    fn needs_eviction(&self, new_size: u64) -> bool {
        self.len() as u64 >= self.max_entries 
            || self.current_size + new_size > self.max_size
    }
    
    /// Returns the current total size of cached content.
    #[inline]
    pub fn size(&self) -> u64 {
        self.current_size
    }
    
    /// Returns the maximum content size the cache can hold.
    #[inline]
    pub fn max_size(&self) -> u64 {
        self.max_size
    }
    
    /// Returns the maximum number of entries the cache can hold.
    #[inline]
    pub fn max_entries(&self) -> u64 {
        self.max_entries
    }
    
    /// Returns the number of entries in the cache.
    #[inline]
    pub fn len(&self) -> usize {
        self.map.len()
    }
    
    /// Returns true if the cache is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
    
    // ========================================================================
    // Timestamp-based queries (enabled by AtomicU64 timestamps)
    // ========================================================================
    
    /// Returns entries that haven't been accessed in the given duration.
    /// Useful for TTL-based eviction or monitoring.
    #[cfg(feature = "std")]
    pub fn idle_entries(&self, idle_threshold_nanos: u64) -> impl Iterator<Item = &K> {
        self.map.keys().filter(move |k| {
            if let Some(&node) = self.map.get(*k) {
                unsafe { (*node).value.idle_nanos() > idle_threshold_nanos }
            } else {
                false
            }
        })
    }
    
    // ========================================================================
    // Internal
    // ========================================================================
    
    fn evict_lru(&mut self) {
        if let Some(entry) = self.list.pop_back() {
            self.map.remove(&entry.value.key);
            self.current_size -= entry.value.size;
            // Note: For external storage cleanup, caller should use
            // put() return value or implement EvictionListener (future phase)
        }
    }
}
```

---

## Migration Guide

### From Current LRU

```rust
// Before (v0.3)
let mut cache = Lru::new(NonZeroUsize::new(1000).unwrap());
cache.put("key", value);

// After (v0.4) - count-based with explicit size
let mut cache: Lru<&str, Value> = Lru::new(1000);
cache.put("key", value, 1);  // size=1 for count-based
```

### From Current GDSF

```rust
// Before (v0.3) - explicit size parameter
let mut cache = Gdsf::new(NonZeroUsize::new(10_000_000).unwrap());
cache.put("key", value, 1024);

// After (v0.4) - same API, just new constructor
let mut cache: Gdsf<&str, Value> = Gdsf::with_max_size(10_000_000);
cache.put("key", value, 1024);  // Size still explicit
```

### New Dual-Limit API

```rust
// Disk cache index: 5M entries, 100GB tracked
let mut cache = Lru::with_limits(5_000_000, 100 * 1024 * 1024 * 1024);
cache.put(url, file_meta, file_size_on_disk);
```

---

## Memory Overhead Analysis

### Per-Entry Overhead

| Field | Size | Notes |
|-------|------|-------|
| `key: K` | varies | User's key type |
| `value: V` | varies | User's value type |
| `size: u64` | 8 bytes | Always present |
| `last_accessed: AtomicU64` | 8 bytes | Atomic for lock-free reads |
| `create_time: AtomicU64` | 8 bytes | Atomic for consistency |
| `metadata: Option<M>` | 0-24 bytes | Algorithm-specific |

**Base overhead (all algorithms): 24 bytes**

### Algorithm-Specific Metadata

| Algorithm | Metadata Type | Metadata Size | Total Entry Overhead |
|-----------|---------------|---------------|----------------------|
| LRU | `None` (no metadata) | 0 bytes | **24 bytes** |
| LFU | `Option<LfuMeta>` | 16 bytes | **40 bytes** |
| LFUDA | `Option<LfudaMeta>` | 16 bytes | **40 bytes** |
| SLRU | `Option<SlruMeta>` | 8 bytes | **32 bytes** |
| GDSF | `Option<GdsfMeta>` | 24 bytes | **48 bytes** |

### Cache-rs Memory Budget

For planning `max_entries`, estimate ~150 bytes per entry:
- Entry overhead: 24-48 bytes (depends on algorithm)
- HashMap entry: ~48 bytes
- List node: ~48 bytes
- Key/Value: varies

**Example**: 1M entries ≈ 150MB of cache-rs memory

---

## Implementation Plan

### Phase 1: Core Infrastructure (Week 1-2)

1. Add `CacheEntry<K, V, M>` to `src/entry.rs`
2. Add algorithm metadata types to `src/meta.rs`
3. Add dual-limit fields to all cache structs

### Phase 2: Migrate Algorithms (Week 3-4)

1. Update LRU to use `CacheEntry<K, V, ()>` with `metadata: None`
2. Update LFU to use `CacheEntry<K, V, LfuMeta>`
3. Update LFUDA to use `CacheEntry<K, V, LfudaMeta>`
4. Update SLRU to use `CacheEntry<K, V, SlruMeta>`
5. Update GDSF to use `CacheEntry<K, V, GdsfMeta>`

### Phase 3: API Changes (Week 5)

1. Change `put(key, value)` → `put(key, value, size)` on all caches
2. Add `new()`, `with_max_size()`, `with_limits()` constructors
3. Update all tests

### Phase 4: Testing & Documentation (Week 6)

1. Update all tests for new API
2. Add migration guide
3. Update documentation
4. Benchmark comparison

---

## Future Extensions

This design enables future features without breaking changes:

### Phase 2: Eviction Listener (Future RFC)

For external storage cleanup (disk files, arena slots, etc.):

```rust
/// Callback for external storage cleanup
pub trait EvictionListener<K, V> {
    fn on_evict(&self, key: &K, value: &V, size: u64);
}

pub struct Lru<K, V, L = NoOpListener, S = DefaultHashBuilder> {
    // ... existing fields ...
    listener: L,
}

// Usage
let mut cache = Lru::with_listener(
    5_000_000, 
    100_000_000_000,
    |key: &String, meta: &FileMeta, _size| {
        std::fs::remove_file(&meta.path).ok();
    }
);
```

**Deferred to Phase 2** because:
- Core API can work without it (use returned evicted value)
- Adds generic parameter complexity
- Most use cases can poll/cleanup externally

### Phase 3: TTL Support

Built-in with `create_time`:

```rust
impl<K, V, M> CacheEntry<K, V, M> {
    pub fn is_expired(&self, ttl_nanos: u64) -> bool {
        self.age_nanos() > ttl_nanos
    }
}

impl<K, V, S> Lru<K, V, S> {
    pub fn evict_expired(&mut self, ttl_nanos: u64) {
        // Use create_time to find expired entries
    }
}
```

### Phase 4: Frequency-Based Admission (SLRU Enhancement)

```rust
pub struct SlruWithAdmission<K, V> {
    inner: Slru<K, V>,
    frequency_sketch: FrequencySketch<K>,
    admission_threshold: u8,
}
```

---

## Rejected Alternatives

### Sizer Trait

Rejected in favor of explicit `size` parameter on `put()`:
- Simpler API - no generic `Z` parameter on cache types
- More explicit - size visible at call site
- No closure storage overhead
- Fits naturally with external storage (size already known)

### V: Sizer Trait Bound

Rejected due to orphan rule issues:
```rust
// Can't implement Sizer for std types
impl Sizer for String { ... }  // Orphan rule violation!
```

### Full Atomic Metadata (Moka-style)

Rejected for simplicity. Only timestamps need to be atomic:
- `size` is immutable after insertion
- `metadata` changes require structural updates (need lock anyway)
- `last_accessed` and `create_time` are read-only after set

### Unified 48-byte Metadata (RFC-002)

Rejected due to memory overhead for simple LRU caches.

### No Timestamps

Rejected because timestamps enable valuable features:
- TTL-based eviction
- Monitoring and metrics
- Debugging (how old is this entry?)
- Idle entry detection

### Single Limit (max_entries OR max_size)

Rejected because real-world use cases need both:
- In-memory: bound total memory
- Disk index: bound both RAM (entries) and disk (size)
- Arena allocator: bound slots and total bytes

---

## References

- [Moka Cache](https://github.com/moka-rs/moka) - Rust cache with similar patterns
- [Caffeine](https://github.com/ben-manes/caffeine) - Java cache, inspiration for TinyLFU
- [cache-rs RFC-002](size_aware_cache.md) - Previous design (superseded)
- [Architecture Review](SIZE_AWARE_CACHE_FINAL_SPEC.md) - Detailed analysis

---

## Appendix A: File Structure

```
src/
├── lib.rs
├── entry.rs          # NEW: CacheEntry<K, V, M>
├── meta.rs           # NEW: Algorithm metadata types (LfuMeta, SlruMeta, GdsfMeta)
├── list.rs           # Existing: doubly-linked list
├── lru.rs            # Updated: use CacheEntry<K, V, ()>, dual limits
├── lfu.rs            # Updated: use CacheEntry<K, V, LfuMeta>
├── lfuda.rs          # Updated: use CacheEntry<K, V, LfudaMeta>
├── slru.rs           # Updated: use CacheEntry<K, V, SlruMeta>
├── gdsf.rs           # Updated: use CacheEntry<K, V, GdsfMeta>
└── concurrent/
    ├── mod.rs
    ├── lru.rs        # ShardedCache<Lru<...>>
    └── ...
```

---

## Appendix B: Complete Type Reference

```rust
// =============================================================================
// Core Entry Type
// =============================================================================
pub struct CacheEntry<K, V, M = ()> {
    pub key: K,
    pub value: V,
    pub size: u64,
    pub last_accessed: AtomicU64,
    pub create_time: AtomicU64,
    pub metadata: Option<M>,
}

// =============================================================================
// Algorithm Metadata
// =============================================================================
pub struct LfuMeta { pub frequency: u64 }
pub struct LfudaMeta { pub frequency: u64 }
pub struct SlruMeta { pub segment: SlruSegment }
pub struct GdsfMeta { pub frequency: u64, pub priority: f64 }

// =============================================================================
// Cache Types (no Sizer generic!)
// =============================================================================
pub struct Lru<K, V, S = DefaultHashBuilder>;
pub struct Lfu<K, V, S = DefaultHashBuilder>;
pub struct Lfuda<K, V, S = DefaultHashBuilder>;
pub struct Slru<K, V, S = DefaultHashBuilder>;
pub struct Gdsf<K, V, S = DefaultHashBuilder>;

// =============================================================================
// Concurrent Versions
// =============================================================================
pub type ConcurrentLru<K, V> = ShardedCache<Lru<K, V>>;
pub type ConcurrentGdsf<K, V> = ShardedCache<Gdsf<K, V>>;
```

---

## Appendix C: Usage Examples

```rust
// =============================================================================
// 1. Simple count-based LRU (most common)
// =============================================================================
let mut cache: Lru<String, User> = Lru::new(1000);  // Max 1000 entries
cache.put("alice".into(), user, 1);  // size=1 for counting
if let Some(user) = cache.get("alice") {
    println!("Found: {:?}", user);
}

// =============================================================================
// 2. Size-aware LRU for byte buffers
// =============================================================================
let mut cache: Lru<String, Vec<u8>> = Lru::with_max_size(10 * 1024 * 1024);
cache.put("image.png".into(), image_bytes.clone(), image_bytes.len() as u64);

// =============================================================================
// 3. Dual-limit disk cache index (nginx/Squid style)
// =============================================================================
struct FileMeta {
    path: PathBuf,
    etag: String,
}

// 5M entries (~750MB RAM), tracking 100GB disk
let mut cache = Gdsf::with_limits(5_000_000, 100 * 1024 * 1024 * 1024);
cache.put(url, FileMeta { path, etag }, file_size_on_disk);

// =============================================================================
// 4. Arena allocator index
// =============================================================================
let arena = SlabAllocator::new(1_000_000_000);  // 1GB arena
let mut cache = Lru::with_limits(
    50_000,                  // max slots
    1_000_000_000            // 1GB total
);
let handle = arena.alloc(texture_data);
cache.put(texture_id, handle, texture_size);

// =============================================================================
// 5. TTL-based eviction using built-in timestamps
// =============================================================================
let mut cache: Lru<String, Data> = Lru::new(10000);
cache.put("key".into(), data, 1);

// Check age
if let Some(entry) = cache.peek("key") {
    if entry.age_nanos() > Duration::from_secs(3600).as_nanos() as u64 {
        cache.remove("key");
    }
}

// =============================================================================
// 6. Monitoring idle entries
// =============================================================================
let idle_threshold = Duration::from_secs(300).as_nanos() as u64;
for key in cache.idle_entries(idle_threshold) {
    println!("Entry {} hasn't been accessed in 5 minutes", key);
}
```
