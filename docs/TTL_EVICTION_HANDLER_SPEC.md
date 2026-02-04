# TTL and Eviction Handler Support for cache-rs

## Design Specification

**Authors:** Team  
**Status:** Draft  
**Created:** February 2026  
**Last Updated:** February 2026

---

## Abstract

This document specifies the design for adding Time-To-Live (TTL) support and eviction handler callbacks to the cache-rs library. The design addresses two primary goals: ensuring stale content is never served (unless stale-while-revalidate is enabled), and preferring expired items for eviction before applying the underlying cache algorithm's eviction policy. The specification evaluates multiple implementation approaches, analyzes their trade-offs in the context of both single-threaded and concurrent cache implementations, and recommends a hybrid lazy-evaluation strategy that maintains the library's performance characteristics while providing robust TTL semantics.

---

## 1. Introduction

### 1.1 Problem Statement

The current cache-rs implementation provides sophisticated eviction algorithms (LRU, LFU, LFUDA, SLRU, GDSF) that operate purely on access patterns and capacity constraints. However, many caching scenarios require time-based invalidation where cached data becomes stale after a defined period. Without TTL support, users must either implement external expiration logic or accept potentially stale data.

Additionally, when eviction occurs—whether due to TTL expiration or capacity pressure—applications often need to perform cleanup actions: persisting data to disk, releasing external resources, notifying dependent systems, or collecting metrics. The current implementation discards evicted entries silently, providing no mechanism for such callbacks.

### 1.2 Requirements

The design must satisfy the following requirements:

**Functional Requirements:**
1. Support per-entry TTL specification at insertion time
2. Never return expired content from cache reads (staleness guarantee)
3. Support optional stale-while-revalidate semantics with a revalidation callback
4. Invoke registered eviction handlers when entries are removed, whether due to TTL expiration, capacity eviction, or explicit removal
5. Prefer evicting expired entries before invoking the underlying eviction algorithm

**Non-Functional Requirements:**
1. Maintain O(1) time complexity for get/put operations in the common case
2. Preserve `no_std` compatibility for the core library
3. Minimize performance impact on non-concurrent caches (no mandatory locking)
4. Support both single-threaded and concurrent cache implementations
5. Remain backward-compatible with existing API usage

### 1.3 Scope

This specification covers TTL and eviction handler support for all cache algorithms (LRU, LFU, LFUDA, SLRU, GDSF) in both their single-threaded and concurrent variants. Background reaper threads and async runtimes are addressed but presented as optional features requiring the `std` feature flag.

---

## 2. Design Analysis

This section evaluates three candidate approaches for TTL implementation, analyzing their trade-offs across dimensions relevant to cache-rs.

### 2.1 Approach A: Eager Background Expiration

In this model, a dedicated background thread periodically scans the cache and removes expired entries proactively. The thread maintains a timer or sleeps between sweeps, ensuring expired items are removed even when no cache operations occur.

**Architecture:**
```
┌─────────────────────────────────────────────────────────────┐
│                        Cache                                │
│  ┌─────────────┐    ┌──────────────┐    ┌───────────────┐  │
│  │   HashMap   │◄───│  Data List   │◄───│  Expiry Heap  │  │
│  └─────────────┘    └──────────────┘    └───────────────┘  │
│         ▲                                       ▲          │
│         │                                       │          │
│         └───────────────────────────────────────┘          │
│                    Background Thread                        │
│                  (periodic sweep/pop)                       │
└─────────────────────────────────────────────────────────────┘
```

**Pros:**
- Expired entries are removed promptly, reducing memory pressure
- Cache metrics reflect actual live entry count accurately
- Eviction handlers fire in a timely manner
- Deterministic cleanup independent of access patterns

**Cons:**
- Requires `std` feature for thread spawning, incompatible with `no_std`
- Forces synchronization primitives on single-threaded caches, adding ~15-30% overhead per operation even in uncontended paths
- Background thread consumes CPU cycles even when cache is idle
- Complex lifecycle management (thread shutdown, panic handling)
- Priority inversion risks if background thread holds locks during eviction handler execution

For cache-rs, this approach conflicts fundamentally with the library's design philosophy. The single-threaded cache implementations currently use raw pointers with no synchronization, achieving optimal performance. Introducing mandatory locking would degrade performance for all users, including those who do not require TTL support.

### 2.2 Approach B: Lazy Evaluation (Check on Access)

Lazy evaluation defers expiration checks to the moment of access. When a `get()` operation encounters an expired entry, it removes the entry and returns `None`. Expired entries remain in the cache until accessed or evicted through capacity pressure.

**Architecture:**
```
get(key) {
    if entry_exists(key) {
        if entry.is_expired() {
            remove_and_invoke_handler(entry)
            return None
        }
        return Some(entry.value)
    }
    return None
}
```

**Pros:**
- Zero overhead when TTL is not configured (check can be compile-time eliminated)
- No background threads required; fully `no_std` compatible
- No synchronization overhead for single-threaded caches
- Implementation complexity is localized to get/put paths
- Natural fit with existing reactive eviction model

**Cons:**
- Expired entries consume memory until accessed or evicted
- Eviction handlers may fire with significant delay
- Cache size metrics may overcount (include expired entries)
- "Ghost entries" problem: expired entries might be evicted by capacity pressure before TTL-expired entries that were accessed more recently

The ghost entries problem deserves elaboration. Consider an LRU cache with entries A (TTL expired) and B (not expired, but LRU). If the cache reaches capacity and B is the LRU candidate, B will be evicted while the expired entry A remains. This violates the principle that expired entries should be evicted preferentially.

### 2.3 Approach C: Hybrid Lazy with Expired-Entries Index

This approach combines lazy evaluation with an auxiliary data structure tracking potentially-expired entries. The index enables preferential eviction of expired entries during capacity pressure without requiring background threads.

**Architecture:**
```
┌───────────────────────────────────────────────────────────────────┐
│                            Cache                                  │
│  ┌─────────────┐     ┌──────────────┐     ┌────────────────────┐ │
│  │   HashMap   │────▶│  Data List   │     │   Expiry Index     │ │
│  │  K → Node*  │     │  (K,V,TTL)   │     │  (expiry_time, K)  │ │
│  └─────────────┘     └──────────────┘     └────────────────────┘ │
│                                                    │              │
│                             ┌──────────────────────┘              │
│                             ▼                                     │
│                    on_eviction_needed():                          │
│                      1. Check expiry index for expired entries    │
│                      2. If found, evict expired entry             │
│                      3. Else, use normal eviction algorithm       │
└───────────────────────────────────────────────────────────────────┘
```

The expiry index can be implemented as a min-heap ordered by expiration time, enabling O(1) peek at the next-to-expire entry. During eviction, the algorithm first checks if the top of the heap is expired; if so, it evicts that entry instead of the algorithm's normal candidate.

**Pros:**
- Expired entries are preferentially evicted during capacity pressure
- No background threads; `no_std` compatible
- No mandatory locking for single-threaded caches
- Staleness guarantee maintained (lazy check on get)
- Memory overhead is bounded (one pointer per TTL-enabled entry)

**Cons:**
- Small memory overhead for expiry index (~16 bytes per entry with TTL)
- Heap maintenance adds O(log n) overhead to put operations with TTL
- Expired entries still not proactively removed; delay depends on access/eviction patterns
- Index must be kept synchronized with main data structures

---

## 3. Recommended Approach

After evaluating the three approaches against cache-rs's design principles and requirements, we recommend **Approach C: Hybrid Lazy with Expired-Entries Index**, with optional background reaping for concurrent caches that opt into the `std` feature.

### 3.1 Rationale

The hybrid approach provides the best balance across competing concerns:

**Performance Preservation:** Single-threaded caches remain lock-free. The TTL check in `get()` is a simple integer comparison against the current timestamp, costing approximately 2-3 nanoseconds on modern hardware. For entries without TTL, the check can be eliminated entirely through conditional compilation or Option-based branching.

**Staleness Guarantee:** Lazy evaluation at access time ensures expired content is never returned. The instant an entry's TTL elapses and a read occurs, the entry is invalidated. This matches the semantics users expect from TTL-based caching.

**Preferential Expired Eviction:** The expiry index solves the ghost entries problem from pure lazy evaluation. When capacity pressure triggers eviction, expired entries are evicted first. This is a best-effort optimization—not a guarantee that all expired entries will be evicted before any valid entry—but it significantly improves behavior in practice.

**`no_std` Compatibility:** The core TTL mechanism requires only timestamp comparison and heap operations, both achievable in `no_std` with the `alloc` crate. Users in embedded contexts can provide their own clock source.

**Optional Eager Reaping:** For concurrent caches where background cleanup is desirable, we support an optional reaper thread behind the `std` feature flag. This thread is not required for correctness; it's a convenience for applications that prefer proactive cleanup.

### 3.2 Design Decisions

Several key decisions shape the implementation:

**Per-Entry TTL vs. Cache-Wide TTL:** We support per-entry TTL specified at insertion time via a new `put_with_ttl()` method. A cache-wide default TTL can be configured but is overridden by per-entry values. This flexibility supports use cases where different content types have different freshness requirements (e.g., user sessions vs. static assets).

**TTL Units:** TTL is specified as `Duration` (with `std`) or raw nanoseconds (for `no_std`). Internally, we store absolute expiration timestamps rather than relative TTL values, avoiding repeated timestamp arithmetic on access.

**Expiry Index Structure:** We use a `BTreeMap<(u64, K), ()>` keyed by `(expiry_timestamp, key)` rather than a binary heap. This design:
- Supports efficient range queries to find all entries expired before a given time
- Allows O(log n) removal by key when entries are explicitly deleted
- Integrates naturally with cache-rs's existing use of `BTreeMap` for metrics and priority queues

**Eviction Handler Signature:** The eviction handler receives a batch of evicted entries to amortize callback overhead. The signature is:

```rust
pub type EvictionHandler<K, V> = Box<dyn Fn(&[(K, V, EvictionReason)]) + Send + Sync>;

pub enum EvictionReason {
    Expired,
    Capacity,
    Explicit,  // via remove() call
    Replaced,  // via put() with existing key
}
```

Batch delivery allows efficient handling when multiple entries are evicted in a single operation (e.g., inserting a large object that requires evicting several smaller ones).

**Stale-While-Revalidate:** When enabled, expired entries return their stale value while triggering an asynchronous revalidation callback. This requires tracking "soft" vs. "hard" expiration and is implemented as an optional `StaleWhileRevalidate` configuration:

```rust
pub struct StaleWhileRevalidate<K, V> {
    /// Maximum time after hard expiry that stale content can be served
    pub max_stale: Duration,
    /// Callback invoked when stale content is served
    pub revalidate: Option<Box<dyn Fn(&K, &V) + Send + Sync>>,
}
```

---

## 4. Detailed Design

### 4.1 Configuration Changes

TTL and eviction handler configuration extends the existing config structs:

```rust
/// TTL configuration for a cache.
pub struct TtlConfig {
    /// Default TTL for entries inserted without explicit TTL.
    /// None means no default TTL (entries never expire unless specified).
    pub default_ttl: Option<Duration>,
    
    /// Stale-while-revalidate configuration.
    pub stale_while_revalidate: Option<StaleWhileRevalidate>,
}

/// Extended cache configuration with TTL and eviction handler support.
pub struct CacheConfig<BaseConfig> {
    /// Base algorithm-specific configuration
    pub base: BaseConfig,
    
    /// TTL configuration (optional)
    pub ttl: Option<TtlConfig>,
    
    /// Eviction handler called when entries are removed
    pub eviction_handler: Option<EvictionHandler<K, V>>,
}
```

For backward compatibility, existing config structs remain unchanged. Users opt into TTL support by wrapping their config or using new constructor methods.

### 4.2 Entry Modifications

The `CacheEntry` struct gains an optional expiration timestamp:

```rust
pub struct CacheEntry<K, V, M = ()> {
    pub key: K,
    pub value: V,
    pub size: u64,
    last_accessed: u64,
    create_time: u64,
    
    /// Absolute expiration timestamp in nanoseconds since epoch.
    /// None means the entry never expires.
    expiry: Option<u64>,
    
    pub metadata: Option<M>,
}

impl<K, V, M> CacheEntry<K, V, M> {
    /// Returns true if this entry has expired.
    #[inline]
    pub fn is_expired(&self) -> bool {
        match self.expiry {
            Some(exp) => Self::now_nanos() >= exp,
            None => false,
        }
    }
    
    /// Returns true if this entry has a soft expiry (stale but revalidatable).
    #[inline]
    pub fn is_soft_expired(&self, max_stale: u64) -> bool {
        match self.expiry {
            Some(exp) => {
                let now = Self::now_nanos();
                now >= exp && now < exp.saturating_add(max_stale)
            }
            None => false,
        }
    }
}
```

### 4.3 Expiry Index

Each cache segment maintains a min-heap of entry pointers ordered by expiration time. This is the simplest and most efficient structure for TTL-based eviction since we only need to find the earliest-expiring entry.

**Design Decision: Min-Heap vs. BTreeMap**

| Aspect | BTreeMap | Min-Heap |
|--------|----------|----------|
| Peek min | O(1) | O(1) |
| Insert | O(log n) | O(log n) |
| Pop min | O(log n) | O(log n) |
| Remove arbitrary | O(log n) | O(n) or lazy |
| Memory overhead | ~40 bytes/entry | ~16 bytes/entry |
| Complexity | Higher | Lower |

The min-heap wins because:
1. We only need min-access (earliest expiry), not range queries
2. Arbitrary removal is rare (only on explicit `remove()` calls)
3. Lazy deletion handles removal efficiently for the common case
4. Lower memory overhead and implementation complexity

**Lazy Deletion Strategy:**

When an entry is explicitly removed from the cache, we don't search the heap to remove it. Instead, when popping from the heap during eviction, we validate that the pointer is still valid (entry exists in the cache). Invalid entries are simply skipped.

```rust
use alloc::collections::BinaryHeap;
use core::cmp::Ordering;

/// Entry in the expiry heap: (expiry_time, pointer to cache entry).
/// Ordered by expiry time (min-heap via Reverse ordering).
#[derive(Clone, Copy)]
pub struct ExpiryEntry<P> {
    /// Expiration timestamp in nanoseconds
    pub expiry: u64,
    /// Pointer to the cache entry
    pub ptr: P,
}

impl<P> PartialEq for ExpiryEntry<P> {
    fn eq(&self, other: &Self) -> bool {
        self.expiry == other.expiry
    }
}

impl<P> Eq for ExpiryEntry<P> {}

impl<P> PartialOrd for ExpiryEntry<P> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<P> Ord for ExpiryEntry<P> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering for min-heap (BinaryHeap is a max-heap by default)
        other.expiry.cmp(&self.expiry)
    }
}

/// Min-heap of cache entry pointers ordered by expiration time.
/// 
/// Generic over pointer type `P` to work with all cache algorithms:
/// - LRU: `P = *mut ListEntry<CacheEntry<K, V>>`
/// - LFU: `P = *mut ListEntry<CacheEntry<K, V, LfuMeta>>`
/// - etc.
pub struct ExpiryHeap<P> {
    heap: BinaryHeap<ExpiryEntry<P>>,
}

impl<P: Copy> ExpiryHeap<P> {
    /// Creates an empty expiry heap.
    pub fn new() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }

    /// Creates an expiry heap with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            heap: BinaryHeap::with_capacity(capacity),
        }
    }

    /// Insert an entry with the given expiration time. O(log n).
    #[inline]
    pub fn push(&mut self, expiry: u64, ptr: P) {
        self.heap.push(ExpiryEntry { expiry, ptr });
    }

    /// Peek at the earliest expiry time without removing. O(1).
    #[inline]
    pub fn peek_expiry(&self) -> Option<u64> {
        self.heap.peek().map(|e| e.expiry)
    }

    /// Pop the entry with the earliest expiry time. O(log n).
    /// Returns None if the heap is empty.
    #[inline]
    pub fn pop(&mut self) -> Option<(u64, P)> {
        self.heap.pop().map(|e| (e.expiry, e.ptr))
    }

    /// Returns true if the earliest entry has expired.
    #[inline]
    pub fn has_expired(&self, now: u64) -> bool {
        self.peek_expiry().map(|exp| exp <= now).unwrap_or(false)
    }

    /// Returns the number of entries in the heap.
    /// Note: may include stale entries if lazy deletion is used.
    #[inline]
    pub fn len(&self) -> usize {
        self.heap.len()
    }

    /// Returns true if the heap is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }

    /// Clears all entries from the heap.
    pub fn clear(&mut self) {
        self.heap.clear();
    }
}

impl<P> Default for ExpiryHeap<P> {
    fn default() -> Self {
        Self {
            heap: BinaryHeap::new(),
        }
    }
}

// SAFETY: ExpiryHeap is Send/Sync when P is Send.
unsafe impl<P: Send> Send for ExpiryHeap<P> {}
unsafe impl<P: Send> Sync for ExpiryHeap<P> {}
```

**Integration with Cache Segments:**

Each cache segment instantiates `ExpiryHeap` with its specific pointer type:

```rust
// In LRU cache
pub(crate) struct LruSegment<K, V, S = DefaultHashBuilder> {
    config: LruCacheConfig,
    list: List<CacheEntry<K, V>>,
    map: HashMap<K, *mut ListEntry<CacheEntry<K, V>>, S>,
    metrics: LruCacheMetrics,
    current_size: u64,
    /// Expiry heap for TTL support (only populated when TTL is used)
    expiry_heap: ExpiryHeap<*mut ListEntry<CacheEntry<K, V>>>,
}

// In LFU cache
pub(crate) struct LfuSegment<K, V, S = DefaultHashBuilder> {
    // ... existing fields ...
    expiry_heap: ExpiryHeap<*mut ListEntry<CacheEntry<K, V, LfuMeta>>>,
}
```

**Lazy Deletion in Practice:**

When popping expired entries, we validate the pointer before using it:

```rust
/// Pop expired entries with lazy deletion validation.
fn pop_expired_with_validation(&mut self, now: u64) -> Option<*mut ListEntry<CacheEntry<K, V>>> {
    while let Some((expiry, ptr)) = self.expiry_heap.pop() {
        if expiry > now {
            // Not expired yet - push back and stop
            self.expiry_heap.push(expiry, ptr);
            return None;
        }
        
        // Validate pointer is still in the cache
        unsafe {
            let entry = (*ptr).get_value();
            if self.map.contains_key(&entry.key) {
                // Valid expired entry
                return Some(ptr);
            }
            // Stale pointer (entry was already removed) - skip and continue
        }
    }
    None
}
```

**Memory Overhead:**

| Component | Size (bytes) | Notes |
|-----------|--------------|-------|
| `expiry` field in CacheEntry | 16 | `Option<u64>` with alignment |
| Heap entry | 16 | `u64` expiry + 8-byte pointer |

Total additional memory for TTL-enabled entries: ~32 bytes (down from ~48 with BTreeMap).

### 4.4 Modified Cache Operations

The core cache operations are modified to incorporate TTL checking and preferential expiration:

**get() Operation:**

```rust
pub fn get<Q>(&mut self, key: &Q) -> Option<&V>
where
    K: Borrow<Q>,
    Q: ?Sized + Hash + Eq,
{
    let node = self.map.get(key).copied()?;
    
    unsafe {
        let entry = (*node).get_value();
        
        // Check for hard expiration
        if entry.is_expired() {
            // Entry has expired - remove it
            self.remove_entry_internal(key, EvictionReason::Expired);
            return None;
        }
        
        // Check for soft expiration (stale-while-revalidate)
        if let Some(ref swr) = self.config.stale_while_revalidate {
            if entry.is_soft_expired(swr.max_stale) {
                // Trigger revalidation callback (async or sync depending on config)
                if let Some(ref revalidate) = swr.revalidate {
                    revalidate(&entry.key, &entry.value);
                }
            }
        }
        
        // Normal access path
        self.list.move_to_front(node);
        (*node).get_value_mut().touch();
        self.metrics.core.record_hit(entry.size);
        Some(&entry.value)
    }
}
```

**put() Operation with Preferential Expired Eviction (using min-heap):**

```rust
pub fn put_with_size_and_ttl(
    &mut self, 
    key: K, 
    value: V, 
    size: u64,
    ttl: Option<Duration>
) -> Option<V>
where
    K: Clone + Hash + Eq,
{
    let mut evicted_entries = Vec::new();
    let now = CacheEntry::<K, V>::now_nanos();
    
    // Handle existing key update
    if let Some(&node) = self.map.get(&key) {
        unsafe {
            let entry = (*node).get_value_mut();
            
            // Note: We don't remove old expiry from heap (lazy deletion handles it)
            // Just update the entry's expiry field
            let new_expiry = ttl.map(|d| now + d.as_nanos() as u64)
                .or_else(|| self.config.default_ttl.map(|d| now + d.as_nanos() as u64));
            entry.expiry = new_expiry;
            
            // Push new expiry to heap (old entry will be skipped via lazy deletion)
            if let Some(exp) = new_expiry {
                self.expiry_heap.push(exp, node);
            }
            
            // ... existing value update logic, return old value ...
        }
    }
    
    // Eviction loop: prefer expired entries first
    while self.needs_eviction(size) {
        // First, try to evict an expired entry using lazy deletion validation
        if let Some(ptr) = self.pop_expired_validated(now) {
            unsafe {
                let entry = (*ptr).get_value();
                let evicted_key = entry.key.clone();
                let evicted_value = entry.value.clone();
                let evicted_size = entry.size;
                
                // Remove from map and list
                self.map.remove(&evicted_key);
                self.list.remove(ptr);
                self.current_size = self.current_size.saturating_sub(evicted_size);
                
                evicted_entries.push((evicted_key, evicted_value, EvictionReason::Expired));
                continue;
            }
        }
        
        // No expired entries; use normal eviction algorithm
        // Note: We don't remove evicted entries from heap (lazy deletion)
        if let Some((evicted_key, evicted_value, _node)) = self.evict_by_algorithm() {
            evicted_entries.push((evicted_key, evicted_value, EvictionReason::Capacity));
        } else {
            break; // Cache is empty
        }
    }
    
    // Insert new entry
    let expiry = ttl.map(|d| now + d.as_nanos() as u64)
        .or_else(|| self.config.default_ttl.map(|d| now + d.as_nanos() as u64));
    
    let cache_entry = CacheEntry::new_with_expiry(key.clone(), value, size, expiry);
    
    // Add to list and map
    if let Some(node) = self.list.add(cache_entry) {
        self.map.insert(key, node);
        self.current_size += size;
        
        // Add to expiry heap if TTL is set
        if let Some(exp) = expiry {
            self.expiry_heap.push(exp, node);
        }
        
        self.metrics.core.record_insertion(size);
    }
    
    // Invoke eviction handler
    if !evicted_entries.is_empty() {
        if let Some(ref handler) = self.eviction_handler {
            handler(&evicted_entries);
        }
    }
    
    // ... return value ...
}
    
    // Insert new entry
    let expiry = ttl.map(|d| now + d.as_nanos() as u64)
        .or_else(|| self.config.default_ttl.map(|d| now + d.as_nanos() as u64));
    
    let cache_entry = CacheEntry::new_with_expiry(key.clone(), value, size, expiry);
    
    // Add to list and map
    if let Some(node) = self.list.add(cache_entry) {
        self.map.insert(key, node);
        self.current_size += size;
        
        // Register in expiry index if TTL is set
        if let Some(exp) = expiry {
            self.expiry_index.insert(exp, node);
        }
        
        self.metrics.core.record_insertion(size);
    }
    
    // Invoke eviction handler
    if !evicted_entries.is_empty() {
        if let Some(ref handler) = self.eviction_handler {
            handler(&evicted_entries);
        }
    }
    
    // ... return value ...
}
```

### 4.5 Concurrent Cache Considerations

For concurrent caches, the design extends naturally with some considerations:

**Lock Scope:** TTL checks and expiry index operations occur within the existing per-segment locks. No additional synchronization is required.

**Cross-Segment Expiry:** Each segment maintains its own expiry index. This means expired entries in one segment might persist while another segment evicts valid entries under capacity pressure. This is consistent with the existing per-segment LRU semantics and acceptable for most use cases.

**Optional Background Reaper:** An optional reaper thread can be enabled for concurrent caches:

```rust
impl<K, V, S> ConcurrentLruCache<K, V, S> {
    /// Enables background expiration with the specified scan interval.
    /// Requires the `std` feature.
    #[cfg(feature = "std")]
    pub fn enable_background_expiration(&self, interval: Duration) -> ReaperHandle {
        let cache = Arc::clone(&self.inner);
        let handle = std::thread::spawn(move || {
            loop {
                std::thread::sleep(interval);
                // Scan each segment and remove expired entries
                for segment in cache.segments.iter() {
                    let mut guard = segment.lock();
                    guard.expire_entries();
                }
            }
        });
        ReaperHandle { handle: Some(handle) }
    }
}
```

The reaper is non-blocking: it acquires segment locks briefly to scan and evict, then releases. Applications sensitive to latency can tune the interval or disable the reaper entirely.

### 4.6 Eviction Handler Execution

Eviction handlers execute synchronously within the cache operation that triggers eviction. This design choice has important implications:

**Pros:**
- Simple mental model: handler completion guarantees eviction is processed
- No additional synchronization for handler invocation
- Handler can safely access external state without races

**Cons:**
- Long-running handlers block cache operations
- Handler panics can leave cache in inconsistent state

To mitigate handler risks, we provide guidance in documentation and support an async execution mode for concurrent caches:

```rust
/// Eviction handler configuration
pub struct EvictionHandlerConfig<K, V> {
    pub handler: EvictionHandler<K, V>,
    
    /// If true, handler is invoked asynchronously on a separate thread.
    /// Only available with `std` feature for concurrent caches.
    #[cfg(feature = "std")]
    pub async_execution: bool,
}
```

For async execution, evicted entries are sent through a channel to a handler thread, decoupling handler execution from cache operations.

### 4.7 Eviction Handler Batching Semantics

The eviction handler receives entries in batches for efficiency. The batching behavior is deterministic:

**Single Operation Batch:** When a `put()` operation triggers one or more evictions (either expired entries or capacity-based), all evicted entries are collected into a single batch and delivered to the handler once, after the operation completes.

```rust
// Example: put() triggering multiple evictions
cache.put("large_key", large_value); // Requires evicting 3 entries
// Handler called ONCE with [(k1, v1, Capacity), (k2, v2, Expired), (k3, v3, Capacity)]
```

**Ordering Guarantees:** Within a batch, expired entries appear before capacity-evicted entries. This ordering reflects the actual eviction sequence and allows handlers to distinguish patterns.

**No Cross-Operation Batching:** Each cache operation that causes eviction triggers its own handler invocation. There is no buffering across operations:

```rust
cache.put("a", v1); // Evicts x → handler([x])
cache.put("b", v2); // Evicts y → handler([y])
// NOT: handler([x, y])
```

**Empty Batch Guarantee:** The handler is only invoked when at least one entry is evicted. Cache operations that complete without eviction do not invoke the handler.

**Handler Execution Timing:** For synchronous handlers, the handler completes before `put()` returns. For async handlers (concurrent caches only), the handler is scheduled but `put()` returns immediately.

### 4.8 Memory Layout Impact

The min-heap approach provides excellent memory efficiency:

| Component | Size (bytes) | Notes |
|-----------|--------------|-------|
| `expiry` field in CacheEntry | 16 | `Option<u64>` with alignment |
| Heap entry (`ExpiryEntry<P>`) | 16 | `u64` expiry + 8-byte pointer |

**Comparison of approaches:**

| Approach | Memory per TTL entry | Arbitrary removal |
|----------|---------------------|-------------------|
| Key-based `BTreeMap<(u64, K), ()>` | ~48 bytes + key size | O(log n) |
| Pointer BTreeMap + Vec | ~24-32 bytes | O(log n) + O(k) |
| **Min-Heap (chosen)** | ~32 bytes | Lazy deletion |

For entries without TTL, the overhead is only the `Option<u64>` discriminant in `CacheEntry` (optimized to 8 bytes when expiry is `None`). The heap has zero entries for non-TTL keys.

**Lazy deletion trade-off:** The heap may temporarily contain stale entries (pointers to evicted or updated entries), but:
- Stale entries are cleaned up incrementally during eviction
- Memory overhead is bounded (at most O(updates) extra entries)
- Trade-off is worthwhile vs. O(n) heap search for eager deletion

### 4.9 API Surface

New public API additions:

```rust
// New methods on all cache types
impl<K, V, S> LruCache<K, V, S> {
    /// Insert with explicit TTL.
    pub fn put_with_ttl(&mut self, key: K, value: V, ttl: Duration) -> Option<V>;
    
    /// Insert with explicit size and TTL.
    pub fn put_with_size_and_ttl(&mut self, key: K, value: V, size: u64, ttl: Duration) -> Option<V>;
    
    /// Manually expire all entries past their TTL.
    /// Returns the number of expired entries removed.
    pub fn expire(&mut self) -> usize;
    
    /// Set the eviction handler.
    pub fn set_eviction_handler(&mut self, handler: EvictionHandler<K, V>);
}

// Configuration types
pub struct TtlConfig { /* ... */ }
pub struct StaleWhileRevalidate { /* ... */ }
pub enum EvictionReason { Expired, Capacity, Explicit, Replaced }
pub type EvictionHandler<K, V> = Box<dyn Fn(&[(K, V, EvictionReason)]) + Send + Sync>;
```

---

## 5. Implementation Plan

### Phase 1: Core TTL Infrastructure
1. Add `expiry` field to `CacheEntry`
2. Implement `ExpiryIndex` data structure
3. Modify `get()` to check expiration
4. Add `put_with_ttl()` methods
5. Add manual `expire()` method

### Phase 2: Preferential Expiration
1. Integrate `ExpiryIndex` into eviction paths
2. Modify eviction loops to check expired entries first
3. Update all cache algorithms (LRU, LFU, LFUDA, SLRU, GDSF)

### Phase 3: Eviction Handlers
1. Define `EvictionHandler` type and `EvictionReason` enum
2. Add handler invocation to all eviction paths
3. Implement batch collection and delivery
4. Support handler configuration

### Phase 4: Stale-While-Revalidate
1. Add soft expiry checking
2. Implement revalidation callback mechanism
3. Document usage patterns

### Phase 5: Concurrent Cache Enhancements
1. Extend concurrent caches with TTL support
2. Implement optional background reaper
3. Add async handler execution option

### Phase 6: Testing and Documentation
1. Comprehensive unit tests for all TTL scenarios
2. Integration tests for eviction handler behavior
3. Benchmark TTL overhead
4. Update documentation and examples

---

## 6. Alternatives Considered

### 6.1 Hierarchical Timing Wheels

Timing wheels provide O(1) insertion and expiration for time-based events. However, they require periodic "ticking" (background thread) and add significant complexity. For cache-rs, where TTL is an optional feature and `no_std` support is required, the complexity outweighs the performance benefit.

### 6.2 External TTL Management

Delegating TTL to users (e.g., wrapping values in a `TimedValue<V>` struct) avoids library complexity but pushes burden to users and prevents preferential expiration. This defeats the purpose of integrated TTL support.

### 6.3 Probabilistic Expiration

Expired entries could be sampled probabilistically on each operation, amortizing cleanup cost. Redis uses a similar approach for its volatile keys. However, this provides weaker guarantees and complicates reasoning about cache behavior. The hybrid approach achieves similar amortization without sacrificing determinism.

---

## 7. Testing Strategy

### 7.1 Unit Tests

- Entry expiration detection at boundary conditions
- Expiry index insert/remove/pop operations
- TTL inheritance from default configuration
- Eviction handler invocation with correct reasons
- Stale-while-revalidate callback triggering

### 7.2 Integration Tests

- Mixed TTL and non-TTL entries
- Preferential expired eviction under capacity pressure
- Cross-algorithm consistency (same TTL behavior across LRU, LFU, etc.)
- Concurrent cache TTL correctness under contention

### 7.3 Performance Tests

- Baseline comparison: cache performance with/without TTL configured
- TTL overhead measurement: additional latency per operation
- Memory overhead validation: actual vs. expected per-entry cost
- Scalability: TTL operations under high entry counts

---

## 8. Resolved Design Questions

This section documents design questions that arose during specification and their resolutions.

### 8.1 Clock Source for `no_std`

**Question:** Should we provide a trait for custom clock sources, or require users to manually set expiration timestamps?

**Resolution:** Provide a `Clock` trait that users can implement for `no_std` environments. The default implementation uses `std::time::SystemTime` when the `std` feature is enabled. For `no_std`, users must provide a clock implementation at cache construction time.

```rust
/// Trait for providing the current time in nanoseconds.
pub trait Clock: Send + Sync {
    /// Returns the current time in nanoseconds since an arbitrary epoch.
    fn now_nanos(&self) -> u64;
}

/// Default clock using system time (requires `std` feature).
#[cfg(feature = "std")]
#[derive(Clone, Copy, Default)]
pub struct SystemClock;

#[cfg(feature = "std")]
impl Clock for SystemClock {
    fn now_nanos(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
    }
}
```

### 8.2 Eviction Handler Error Handling

**Question:** Should handlers return `Result`, and if so, how should errors be propagated?

**Resolution:** Eviction handlers are fire-and-forget by design. They should not return `Result` because:
- Eviction has already occurred; the handler cannot prevent it
- Error handling at the cache level would complicate the API for minimal benefit
- Applications needing error handling should use async handlers with internal error channels

Handlers that need to report errors should use internal mechanisms (logging, metrics, channels) rather than return values.

### 8.3 Metrics Integration

**Question:** Should TTL-related metrics be added to the existing `CacheMetrics` trait?

**Resolution:** Yes. Add the following metrics to `CacheMetrics`:

```rust
// Additional metrics for TTL support
pub struct TtlMetrics {
    /// Number of entries expired (via lazy check or preferential eviction)
    pub entries_expired: u64,
    /// Number of stale-while-revalidate serves
    pub stale_serves: u64,
    /// Number of revalidation callbacks triggered
    pub revalidations_triggered: u64,
}
```

These metrics are tracked separately from core metrics and only populated when TTL is configured.

### 8.4 Zero or Negative TTL

**Question:** Should TTL of 0 or negative values be allowed?

**Resolution:** TTL of zero is allowed and means "expire immediately on next access" (useful for invalidation patterns). The entry is still inserted but will be removed on the first `get()` call. Negative TTL values are not applicable since `Duration` is always non-negative.

### 8.5 Backward Compatibility Guarantees

**Question:** How do we ensure existing code continues to work unchanged?

**Resolution:** Full backward compatibility through:
- All existing config structs and methods remain unchanged
- New TTL methods are additions, not modifications
- Entries without TTL have `None` for expiry (zero overhead in checks)
- Eviction handler is optional; omitting it preserves current behavior
- New config types use composition, not inheritance

---

## 9. Appendix: Benchmark Projections

Based on the current implementation's performance characteristics and the min-heap expiry design:

| Operation | Current (ns) | Projected with TTL (ns) | Overhead |
|-----------|--------------|-------------------------|----------|
| LRU get() | ~50 | ~52 | +4% |
| LRU put() | ~150 | ~165 | +10% |
| LFU get() | ~80 | ~83 | +4% |
| LFU put() | ~250 | ~280 | +12% |

**Analysis:** The min-heap approach improves put() overhead compared to BTreeMap (~10-12% vs. ~13-16%) due to simpler heap operations and lazy deletion eliminating remove operations. The get() overhead is minimal since expiry checking is a simple integer comparison.

**Lazy deletion amortization:** Stale heap entries are cleaned up incrementally during normal eviction operations. The overhead is amortized across multiple operations rather than concentrated in explicit removes.

For workloads where TTL is not configured, the overhead should be negligible as the expiry heap remains empty and checks short-circuit.

---

## 10. Architectural Review Notes

*This section contains self-review feedback and resolutions.*

### Review Finding 1: Handler Panic Safety

**Issue:** If an eviction handler panics, the cache may be left in an inconsistent state (entry removed from map but handler not completed).

**Resolution:** Wrap handler invocation in `std::panic::catch_unwind` (when `std` is available) and log the panic. In `no_std`, document that handlers must not panic. Consider adding a `#[must_use]` attribute to handler configuration to encourage users to handle this case.

### Review Finding 2: Timestamp Overflow

**Issue:** Using `u64` nanoseconds since epoch will overflow in approximately 584 years. While not a practical concern, saturating arithmetic should be used consistently.

**Resolution:** All timestamp arithmetic already uses `saturating_add` and `saturating_sub`. No changes required, but add a note in documentation about the theoretical limit.

### Review Finding 3: Entry Size Estimation with TTL

**Issue:** The `estimate_object_size()` method does not account for expiry heap overhead, potentially underestimating memory usage.

**Resolution:** Update size estimation to include expiry heap overhead when TTL is configured. With the min-heap approach: `TTL_OVERHEAD_BYTES = 32` (16 bytes for `Option<u64>` in entry + 16 bytes for heap entry).

### Review Finding 4: Stale-While-Revalidate Thread Safety

**Issue:** The revalidation callback signature `Fn(&K, &V)` requires `Send + Sync`, which may be overly restrictive for single-threaded caches.

**Resolution:** Use separate callback types for single-threaded (`Fn`) and concurrent (`Fn + Send + Sync`) caches. This adds API surface but preserves flexibility.

### Review Finding 5: Lazy Deletion Heap Growth

**Issue:** With lazy deletion, the heap can grow unbounded if entries are frequently updated (each update adds a new heap entry without removing the old one).

**Resolution:** This is bounded in practice because:
1. Stale entries are cleaned up whenever we pop from the heap during eviction
2. Each cache entry can have at most a small number of stale heap entries (proportional to update frequency)
3. For pathological cases (very frequent updates to same keys), add an optional `compact()` method that rebuilds the heap

**Mitigation for pathological cases:**
```rust
impl<P: Copy> ExpiryHeap<P> {
    /// Rebuild heap keeping only valid entries. O(n log n).
    /// Call periodically if heap.len() >> cache.len().
    pub fn compact<F>(&mut self, is_valid: F)
    where
        F: Fn(P) -> bool,
    {
        let valid: Vec<_> = self.heap.drain()
            .filter(|e| is_valid(e.ptr))
            .collect();
        self.heap = BinaryHeap::from(valid);
    }
}
```

### Review Finding 6: Pointer Validation Cost in Lazy Deletion

**Issue:** Each pop from the heap requires validating the pointer, which involves a HashMap lookup to verify the entry still exists.

**Resolution:** This is acceptable because:
1. The validation is O(1) HashMap lookup
2. It only happens during eviction (not on every get/put)
3. Alternative (eager deletion) would require O(n) heap search or O(log n) auxiliary index
4. In practice, most popped entries are valid; stale entries are the exception

### Review Finding 7: Heap Entry Equality

**Issue:** Multiple heap entries can exist for the same cache entry (after updates). Need to ensure we don't double-evict.

**Resolution:** The validation check inherently handles this: after evicting an entry via its pointer, subsequent pops of stale entries for the same key will fail validation (key no longer in map) and be skipped.

---

## 11. Migration Guide for Existing Users

Existing cache-rs users can adopt TTL and eviction handler support incrementally. The design preserves full backward compatibility.

### 11.1 No Changes Required

All existing code continues to work without modification:

```rust
// Existing code - unchanged
let config = LruCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    max_size: u64::MAX,
};
let mut cache = LruCache::init(config, None);
cache.put("key", "value");  // Works exactly as before
```

### 11.2 Adding TTL to Existing Caches

Enable TTL with the new `put_with_ttl()` method:

```rust
use std::time::Duration;

let mut cache = LruCache::init(config, None);

// Entries without TTL (never expire)
cache.put("static", "data");

// Entries with TTL (expire after 5 minutes)
cache.put_with_ttl("session", session_data, Duration::from_secs(300));
```

### 11.3 Adding Eviction Handlers

Register an eviction handler using the builder pattern or setter method:

```rust
let mut cache = LruCache::init(config, None);

cache.set_eviction_handler(Box::new(|evicted| {
    for (key, value, reason) in evicted {
        println!("Evicted {:?} due to {:?}", key, reason);
    }
}));
```

### 11.4 Configuring Default TTL

For caches where most entries should have the same TTL, configure a default:

```rust
let config = LruCacheConfigWithTtl {
    base: LruCacheConfig {
        capacity: NonZeroUsize::new(1000).unwrap(),
        max_size: u64::MAX,
    },
    ttl: Some(TtlConfig {
        default_ttl: Some(Duration::from_secs(3600)),  // 1 hour default
        stale_while_revalidate: None,
    }),
    eviction_handler: None,
};

let mut cache = LruCache::init_with_ttl(config, None);

// This entry expires in 1 hour (uses default)
cache.put("key", "value");

// This entry expires in 30 seconds (overrides default)
cache.put_with_ttl("key2", "value2", Duration::from_secs(30));
```

---

## 12. Conclusion

The hybrid lazy evaluation approach with an expired-entries index provides robust TTL support for cache-rs while preserving the library's core design principles: O(1) operations, `no_std` compatibility, and lock-free single-threaded performance. The design integrates naturally with the existing eviction algorithms and extends cleanly to concurrent caches.

The eviction handler mechanism completes the TTL story by enabling applications to respond to entry removal, whether due to expiration, capacity pressure, or explicit deletion. The batch delivery model amortizes callback overhead, and the configurable async execution mode accommodates latency-sensitive applications.

Implementation should proceed incrementally through the six phases outlined, with comprehensive testing at each stage to validate correctness and performance characteristics.
