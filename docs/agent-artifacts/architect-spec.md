# Remove `pop()` and Return All Evicted Entries from `put()`

## Overview

**Work Type**: INFRASTRUCTURE CHANGE  
**Algorithms Affected**: All 10 cache implementations (5 single-threaded + 5 concurrent)  
**Breaking Change**: Yes  
**Version Target**: 0.3.0

### Decisions

| Decision | Resolution | Rationale |
|----------|------------|-----------|
| Remove `pop()` | **Yes, from ALL caches** | Broken in concurrent (segment mismatch); removing from both ensures API consistency across sync/concurrent |
| Remove `pop_r()` | **Yes, from ALL caches** | Internal only (`pub(crate)`), used nowhere externally, same flaws as `pop()` |
| `put()` return type | **`Option<Vec<(K, V)>>`** | Captures all evicted entries; `None` = zero-alloc happy path; consumer hands off vec to async system |
| Eviction handlers | **Not needed** | `put()` returning all evicted items lets consumers hand off to async processing without cache knowing about async |

---

## Problem Statement

### Problem 1: `pop()` is fundamentally broken in concurrent caches

The segmented architecture creates an inherent mismatch between `pop()` (global eviction candidate search) and `put()` (local segment routing by key hash):

```
init(2 segments, capacity 2 each)
put(1) → segment-0     put(3) → segment-1
put(2) → segment-0     put(4) → segment-1

pop()  → removes key=1 from segment-0  (global LRU)
put(5) → routes to segment-1 (still full!) → evicts key=3 anyway

Result: key=1 removed unnecessarily. pop() created space in the wrong segment.
```

Additionally, concurrent `pop()` has a TOCTOU race (two-phase scan then re-lock) and requires O(segments) lock acquisitions.

### Problem 2: `put()` silently drops evicted entries

When size-based eviction triggers multiple evictions, only the **last** evicted entry is returned:

```rust
// Current: only last eviction survives
while needs_eviction {
    if let Some(entry) = self.pop() {
        evicted = Some(entry); // overwrites previous!
    }
}
```

This means write-back patterns, resource cleanup, and async eviction processing all lose data.

### Problem 3: API inconsistency

- `pop()` works correctly in single-threaded caches but is broken in concurrent
- Keeping it in single-threaded but not concurrent creates migration traps
- No production cache library (moka, caffeine, guava) exposes a global `pop()` equivalent

---

## Current State

### Methods Being Removed

| File | Method | Visibility | Status |
|------|--------|-----------|--------|
| `src/lru.rs` | `pop()` | `pub` | **Remove** |
| `src/lru.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/lfu.rs` | `pop()` | `pub` | **Remove** |
| `src/lfu.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/lfuda.rs` | `pop()` | `pub` | **Remove** |
| `src/lfuda.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/slru.rs` | `pop()` | `pub` | **Remove** |
| `src/slru.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/gdsf.rs` | `pop()` | `pub` | **Remove** |
| `src/gdsf.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/concurrent/lru.rs` | `pop()` | `pub` | **Remove** |
| `src/concurrent/lru.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/concurrent/lfu.rs` | `pop()` | `pub` | **Remove** |
| `src/concurrent/lfu.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/concurrent/lfuda.rs` | `pop()` | `pub` | **Remove** |
| `src/concurrent/lfuda.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/concurrent/slru.rs` | `pop()` | `pub` | **Remove** |
| `src/concurrent/slru.rs` | `pop_r()` | `pub(crate)` | **Remove** |
| `src/concurrent/gdsf.rs` | `pop()` | `pub` | **Remove** |
| `src/concurrent/gdsf.rs` | `pop_r()` | `pub(crate)` | **Remove** |

### Internal `pop()` Usage

The segment-level `pop()` is called **internally** by `put()` for eviction:

```rust
// Current: put() calls self.pop() internally for eviction
while self.map.len() >= self.cap().get()
    || (self.current_size + size > self.config.max_size && !self.map.is_empty())
{
    if let Some(entry) = self.pop() {  // <-- internal eviction
        self.metrics.core.evictions += 1;
        evicted = Some(entry);
    }
}
```

**Solution**: Replace `pop()` with a private `evict()` method. Same logic, but not exposed in public API.

### Helper Methods Used Only by `pop()`/`pop_r()` in Concurrent Caches

These methods exist solely to support the global `pop()` scan and should also be removed:

| File | Method | Purpose |
|------|--------|---------|
| `src/lru.rs` | `peek_lru_timestamp()` | Scan for global LRU candidate |
| `src/lru.rs` | `peek_mru_timestamp()` | Scan for global MRU candidate |
| `src/lfu.rs` | `peek_min_frequency()` | Scan for global min-frequency candidate |
| `src/lfu.rs` | `peek_max_frequency()` | Scan for global max-frequency candidate |
| `src/lfuda.rs` | `peek_min_priority()` | Scan for global min-priority candidate |
| `src/lfuda.rs` | `peek_max_priority()` | Scan for global max-priority candidate |
| `src/slru.rs` | `peek_lru_timestamp()` | Scan for global LRU candidate |
| `src/slru.rs` | `peek_mru_timestamp()` | Scan for global MRU candidate |
| `src/gdsf.rs` | `peek_min_priority()` | Scan for global min-priority candidate |
| `src/gdsf.rs` | `peek_max_priority()` | Scan for global max-priority candidate |

**Note**: Verify these are not used by any other code path before removing.

---

## API Design

### `put()` Return Type Change

**Before:**
```rust
pub fn put(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>
```

**After:**
```rust
pub fn put(&mut self, key: K, value: V, size: u64) -> Option<Vec<(K, V)>>
```

**Semantics:**
- `None` — Inserted without any eviction or replacement (zero allocation)
- `Some(vec![(k, v)])` — Single entry: either a replaced old entry OR single eviction
- `Some(vec![(k1, v1), (k2, v2), ...])` — Multiple evictions (size-based eviction chain)

### Open Design Question: Should Replacement and Eviction Be Distinguished?

Currently, replacement (key exists → update value) and eviction (cache full → remove LRU) are **mutually exclusive** in all implementations — the code early-returns on replacement before reaching the eviction loop.

**Option A: Mix them in the same `Vec` (simpler)**
```rust
/// Returns evicted and/or replaced entries, or `None` if no entries were displaced.
pub fn put(&mut self, key: K, value: V, size: u64) -> Option<Vec<(K, V)>>
```
- Pro: Simpler API, consumer rarely cares about the distinction
- Pro: If async handoff is the pattern, all displaced entries get same treatment
- Con: Consumer can't tell if first entry is a replacement or eviction

**Option B: Structured return type (more precise)**
```rust
pub struct PutResult<K, V> {
    /// The old entry if the key already existed (replacement)
    pub replaced: Option<(K, V)>,
    /// Entries evicted to make room (0 or more)
    pub evicted: Vec<(K, V)>,
}

pub fn put(&mut self, key: K, value: V, size: u64) -> PutResult<K, V>
```
- Pro: Unambiguous semantics
- Con: More complex API, always allocates the struct
- Con: `replaced` and `evicted` are mutually exclusive today, so struct has wasted field

**Recommendation**: **Option A** — Use `Option<Vec<(K, V)>>`. The distinction between replacement and eviction is an internal concern. Consumers processing evicted items (write-back, cleanup, async handoff) treat all displaced entries the same way.

### GDSF Return Type Alignment

GDSF currently returns `Option<V>` (no key) from `put()`. This change should also align GDSF to return `Option<Vec<(K, V)>>` for consistency.

**Current GDSF behavior**: `put()` returns `Some(old_value)` on replacement, `None` on eviction (evicted entries are silently dropped by `evict_one()`). This must change so evicted entries are collected.

### Consumer Usage Pattern

```rust
// Simple: ignore evictions
cache.put("key", value, 1024);

// Process evictions synchronously
if let Some(evicted) = cache.put("key", value, 1024) {
    for (k, v) in &evicted {
        log::info!("Displaced: {:?}", k);
    }
}

// Hand off to async system (the motivating use case)
if let Some(evicted) = cache.put("key", value, 1024) {
    eviction_tx.send(evicted).unwrap(); // Send vec to async worker
}

// Write-back cache pattern
if let Some(evicted) = cache.put("key", value, 1024) {
    for (k, v) in evicted {
        db.write(k, v); // Persist all evicted entries
    }
}
```

---

## Implementation Strategy

### Phase 1: Remove Public `pop()` and `pop_r()` Functions

Remove all **public** `pop()` and `pop_r()` methods from cache wrappers and concurrent caches.

**Remove from cache wrappers (public API):**
- `LruCache::pop()`, `LruCache::pop_r()`
- `LfuCache::pop()`, `LfuCache::pop_r()`
- `LfudaCache::pop()`, `LfudaCache::pop_r()`
- `SlruCache::pop()`, `SlruCache::pop_r()`
- `GdsfCache::pop()`, `GdsfCache::pop_r()`

**Remove from concurrent wrappers (public API):**
- `ConcurrentLruCache::pop()`, `ConcurrentLruCache::pop_r()`
- `ConcurrentLfuCache::pop()`, `ConcurrentLfuCache::pop_r()`
- `ConcurrentLfudaCache::pop()`, `ConcurrentLfudaCache::pop_r()`
- `ConcurrentSlruCache::pop()`, `ConcurrentSlruCache::pop_r()`
- `ConcurrentGdsfCache::pop()`, `ConcurrentGdsfCache::pop_r()`

### Phase 2: Convert Segment `pop()` to Private `evict()`

Rename segment-level `pop()` to `evict()` and make it private (`fn` not `pub(crate) fn`).

**In each segment implementation:**
```rust
// Before:
pub(crate) fn pop(&mut self) -> Option<(K, V)> { ... }
pub(crate) fn pop_r(&mut self) -> Option<(K, V)> { ... }

// After:
fn evict(&mut self) -> Option<(K, V)> { ... }
// pop_r() removed entirely (only used by concurrent pop_r which is being removed)
```

**Remove these segment methods entirely:**
- `pop_r()` from all segments (only used by removed concurrent `pop_r()`)
- `pop_eviction_candidate()` from GDSF (rename to `evict()`)
- `evict_one()` from GDSF (consolidate into `evict()`)

### Phase 3: Update `put()` to Return `Option<Vec<(K, V)>>`

**Segment-level `put()`:**
```rust
// Cache Segment
fn put(&mut self, key: K, value: V, size: u64) -> Option<Vec<(K, V)>> {
    // Handle key replacement (early return)
    if let Some(&node) = self.map.get(&key) {
        // ... update value in place ...
        return Some(vec![(old_key, old_value)]);
    }

    // Evict until there's space
    let mut evicted = Vec::new();
    while self.needs_eviction(size) {
        if let Some(entry) = self.evict() {
            evicted.push(entry);
        } else {
            break;
        }
    }

    // Insert the new item
    self.insert(key, value, size);

    // Return evicted items (None if empty)
    if evicted.is_empty() { None } else { Some(evicted) }
}
```

**Concurrent cache `put()`:**
```rust
// Concurrent Cache
pub fn put(&self, key: K, value: V, size: u64) -> Option<Vec<(K, V)>> {
    let idx = self.segment_index(&key);
    self.segments[idx].lock().put(key, value, size)
}
```

**Key optimization**: `Vec::new()` does not allocate until first `push()`. So when there are no evictions, we only pay for the stack cost of an empty Vec, then return `None`.

### Phase 4: Remove Helper Methods

Remove segment helper methods that were only used by concurrent `pop()` scans:
- `peek_lru_timestamp()`, `peek_mru_timestamp()` (LRU, SLRU)
- `peek_min_frequency()`, `peek_max_frequency()` (LFU)
- `peek_min_priority()`, `peek_max_priority()` (LFUDA)
- `peek_min_priority_key()`, `peek_max_priority_key()` (GDSF)

### Phase 5: Update Tests

**Tests to remove:**
- All `test_*_pop_*` tests
- All `test_*_pop_r_*` tests

**Tests to update:**
- Any test using `pop()` for draining → use `clear()` or `remove()`
- Any test checking `put()` return type → update to `Option<Vec<(K, V)>>`
- Any test verifying single eviction → update pattern matching

**New tests to add:**
```rust
#[test]
fn test_put_returns_none_when_no_eviction() {
    let mut cache = make_cache(10);
    assert!(cache.put("a", 1, 1).is_none());
}

#[test]
fn test_put_returns_single_eviction() {
    let mut cache = make_cache(2);
    cache.put("a", 1, 1);
    cache.put("b", 2, 1);
    let result = cache.put("c", 3, 1);
    assert_eq!(result, Some(vec![("a", 1)]));
}

#[test]
fn test_put_returns_multiple_evictions_size_based() {
    let mut cache = make_cache_with_size(10, 100); // 10 entries, 100 bytes max
    // Fill with small entries
    for i in 0..10 {
        cache.put(i, i, 10); // 10 bytes each, total = 100
    }
    // Insert large entry that requires evicting multiple
    let result = cache.put(99, 99, 50); // needs 50 bytes → evict 5 entries
    let evicted = result.unwrap();
    assert_eq!(evicted.len(), 5);
}

#[test]
fn test_put_returns_replaced_entry() {
    let mut cache = make_cache(10);
    cache.put("a", 1, 1);
    let result = cache.put("a", 2, 1);
    assert_eq!(result, Some(vec![("a", 1)]));
}
```

### Phase 6: Update Documentation, Examples, and Benchmarks

- Update doc comments on all `put()` methods
- Update `README.md` examples
- Update `examples/*.rs`
- Update benchmark code in `benches/*.rs`
- Update `CHANGELOG.md`

---

## Files Affected

### Single-Threaded Caches (5 files)

Each file contains both a **Segment** (internal) and a **Cache** (public wrapper).

| File | Segment Changes | Cache Wrapper Changes |
|------|-----------------|----------------------|
| `src/lru.rs` | `LruSegment::pop()`→`evict()` (private), remove `pop_r()`, change `put()` return | Remove `LruCache::pop()`, `LruCache::pop_r()` |
| `src/lfu.rs` | `LfuSegment::pop()`→`evict()` (private), remove `pop_r()`, change `put()` return | Remove `LfuCache::pop()`, `LfuCache::pop_r()` |
| `src/lfuda.rs` | `LfudaSegment::pop()`→`evict()` (private), remove `pop_r()`, change `put()` return | Remove `LfudaCache::pop()`, `LfudaCache::pop_r()` |
| `src/slru.rs` | `SlruInner::pop()`→`evict()` (private), remove `pop_r()`, change `put()` return | Remove `SlruCache::pop()`, `SlruCache::pop_r()` |
| `src/gdsf.rs` | `GdsfSegment`: consolidate `pop()`/`pop_eviction_candidate()`/`evict_one()`→`evict()` (private), remove `pop_r()`, change `put()` return to `Option<Vec<(K, V)>>` | Remove `GdsfCache::pop()`, `GdsfCache::pop_r()` |

### Concurrent Caches (5 files)

These only contain the concurrent wrapper — they delegate to segment implementations above.

| File | Changes |
|------|---------|
| `src/concurrent/lru.rs` | Remove `ConcurrentLruCache::pop()`, `pop_r()`, change `put()` return |
| `src/concurrent/lfu.rs` | Remove `ConcurrentLfuCache::pop()`, `pop_r()`, change `put()` return |
| `src/concurrent/lfuda.rs` | Remove `ConcurrentLfudaCache::pop()`, `pop_r()`, change `put()` return |
| `src/concurrent/slru.rs` | Remove `ConcurrentSlruCache::pop()`, `pop_r()`, change `put()` return |
| `src/concurrent/gdsf.rs` | Remove `ConcurrentGdsfCache::pop()`, `pop_r()`, change `put()` return to `Option<Vec<(K, V)>>` |

### Helper Methods to Remove (only used by concurrent `pop()`)

| File | Methods to Remove |
|------|-------------------|
| `src/lru.rs` | `LruSegment::peek_lru_timestamp()`, `peek_mru_timestamp()` |
| `src/lfu.rs` | `LfuSegment::peek_min_frequency()`, `peek_max_frequency()` |
| `src/lfuda.rs` | `LfudaSegment::peek_min_priority()`, `peek_max_priority()` |
| `src/slru.rs` | `SlruInner::peek_lru_timestamp()`, `peek_mru_timestamp()` |
| `src/gdsf.rs` | `GdsfSegment::peek_min_priority_key()`, `peek_max_priority_key()` |

### Tests (4 files)
| File | Changes |
|------|---------|
| `tests/correctness_tests.rs` | Remove pop tests, update put assertions |
| `tests/concurrent_correctness_tests.rs` | Remove pop tests, update put assertions |
| `tests/concurrent_stress_tests.rs` | Update put usage |
| `tests/no_std_tests.rs` | Update put assertions |

### Documentation & Examples
| File | Changes |
|------|---------|
| `README.md` | Update API examples |
| `examples/cache_comparison.rs` | Update put usage |
| `examples/concurrent_usage.rs` | Update put usage |
| `examples/metrics_demo.rs` | Update put usage |
| `CHANGELOG.md` | Document breaking change |
| `src/lib.rs` | Update module-level doc examples |

### Benchmarks
| File | Changes |
|------|---------|
| `benches/cache_benchmarks.rs` | Update put usage |
| `benches/concurrent_benchmarks.rs` | Update put usage |
| `benches/criterion_benchmarks.rs` | Update put usage |

---

## Migration Guide

```rust
// ═══════════════════════════════════════════════
// put() return type: Option<(K,V)> → Option<Vec<(K,V)>>
// ═══════════════════════════════════════════════

// BEFORE: Single eviction
if let Some((k, v)) = cache.put("key", val, size) {
    handle_evicted(k, v);
}

// AFTER: All evictions
if let Some(evicted) = cache.put("key", val, size) {
    for (k, v) in evicted {
        handle_evicted(k, v);
    }
}

// AFTER: Async handoff pattern
if let Some(evicted) = cache.put("key", val, size) {
    eviction_tx.send(evicted).unwrap();
}

// ═══════════════════════════════════════════════
// pop() removal
// ═══════════════════════════════════════════════

// BEFORE: Manual eviction
let evicted = cache.pop();

// AFTER: Not available. Use put() return or remove() instead.
// For draining: use clear() or remove known keys.
```

---

## Performance Analysis

### `put()` Return Type Change: Allocation Cost

| Scenario | Before (`Option<(K,V)>`) | After (`Option<Vec<(K,V)>>`) |
|----------|-------------------------|------------------------------|
| No eviction | `None` (0 alloc) | `None` (0 alloc) |
| Single eviction | `Some((k,v))` (0 alloc) | `Some(vec![(k,v)])` (1 heap alloc: 24-byte Vec header + capacity for 1 tuple) |
| Multiple evictions | Impossible (data lost) | `Some(vec![...])` (1 heap alloc) |
| Replacement | `Some((k,v))` (0 alloc) | `Some(vec![(k,v)])` (1 heap alloc) |

**Impact**: Single-eviction `put()` calls now allocate a small Vec. This is:
- ~24 bytes for Vec metadata + pointer to heap buffer
- One `malloc` call per evicting `put()`
- Negligible compared to the cache entry allocation itself

**Mitigation**: Hot-path `put()` calls that don't trigger eviction pay zero cost (`None`).

### `pop()` Removal: Lock Contention Reduction

Removing concurrent `pop()` eliminates the O(segments) lock scan pattern, which was the most expensive operation in the concurrent API. Any code that was calling `pop()` before `put()` will now be strictly faster (one lock instead of segments+1 locks).

---

## Why Not Eviction Handlers?

With `put()` returning `Option<Vec<(K, V)>>`, eviction handlers are unnecessary:

| Feature | Eviction Handler | `put()` → `Option<Vec<(K,V)>>` |
|---------|------------------|---------------------------------|
| Capture all evictions | ✅ | ✅ |
| Async processing | Built-in (callback in hot path) | Consumer's choice (hand off vec) |
| No async in cache | ❌ (callback runs in `put()`) | ✅ (cache stays sync) |
| Resource cleanup | ✅ | ✅ |
| Write-back pattern | ✅ | ✅ |
| Complexity | High (generic callback, `Send+Sync`) | None (return value) |
| Performance | Callback overhead on every eviction | Vec allocation only when evicting |
| `no_std` compatible | ❌ (closures need `alloc`) | ✅ (`Vec` from `alloc`) |

**Key argument**: The `Option<Vec<(K, V)>>` return keeps the cache library synchronous and simple. The consumer decides how to process evicted entries — synchronously, via channel, via thread pool, etc. This is better separation of concerns than embedding eviction callbacks inside the cache.

---

## Validation Checklist

After implementation, all must pass:

```bash
# Format
cargo fmt --all -- --check

# Clippy (all feature combos)
cargo clippy --all-targets -- -D warnings
cargo clippy --all-targets --features concurrent -- -D warnings
cargo clippy --all-targets --features std,concurrent -- -D warnings

# Tests
cargo test --features "std,concurrent"

# Documentation
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --features concurrent

# No-std
cargo build --target thumbv6m-none-eabi --no-default-features --features hashbrown
```

---

## Summary

| Decision | Resolution |
|----------|------------|
| Remove `pop()` | Yes, from all 10 cache types (sync + concurrent) |
| Remove `pop_r()` | Yes, from all 10 cache types |
| `put()` return type | `Option<Vec<(K, V)>>` |
| Replacement in return vec | Yes, included alongside evictions (Option A — simple, mixed) |
| Eviction handlers | Not needed; `put()` return is sufficient |
| GDSF alignment | Change from `Option<V>` to `Option<Vec<(K, V)>>` |
| Internal eviction logic | Private `evict()` method on each segment, called by `put()` |

---

**✅ Design spec complete: docs/agent-artifacts/architect-spec.md**

Work Type: INFRASTRUCTURE CHANGE  
Algorithms Affected: All 10 implementations (5 single-threaded + 5 concurrent)  
Performance Impact: Small Vec allocation on evicting `put()` calls; net positive from removing concurrent `pop()` lock scans  
Safety Level: Safe (no unsafe code changes, only API surface changes)

Cache Theory Summary: Cache eviction is an internal concern that should be triggered by `put()`, not manually managed by consumers. Returning all evicted entries from `put()` gives consumers full control over post-eviction processing without requiring callbacks, channels, or async semantics in the cache itself.

Use the "🚀 Implement This Design" handoff to proceed to the Developer agent.
