# Unified `put` API Specification

## Overview

**Work Type**: ALGORITHM ENHANCEMENT  
**Algorithms Affected**: All 5 (LRU, LFU, LFUDA, SLRU, GDSF) + all concurrent variants  
**Breaking Change**: Yes - removes `put_with_size()` method

### Problem Statement

The current API has inconsistent signatures:
- LRU, LFU, LFUDA, SLRU: `put(key, value)` + `put_with_size(key, value, size)`
- GDSF: `put(key, value, size)` only (size required)

This creates API inconsistency and forces users to remember different signatures per algorithm.

### Solution

Unify all algorithms to use a single signature with **required** size parameter:
```rust
pub fn put(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>
```

Provide `SIZE_UNIT` constant for entry-count mode (where size tracking doesn't matter).

> **Note**: The initial implementation used `Option<u64>` with `None` defaulting to `1`.
> After analysis (see "Size Parameter Analysis" section below), this was found to be 
> problematic. The recommended approach is to make `size` required.

---

## API Design

### Final Unified Signature

**Single-threaded caches:**
```rust
impl<K, V, S> [Algorithm]Cache<K, V, S> {
    /// Inserts a key-value pair into the cache.
    ///
    /// # Arguments
    /// * `key` - The key to insert
    /// * `value` - The value to cache  
    /// * `size` - Size of this entry for capacity tracking. Use `SIZE_UNIT` (1) for count-based caching.
    ///
    /// # Returns
    /// Returns `Some((evicted_key, evicted_value))` if an entry was evicted or replaced,
    /// `None` if inserted without eviction.
    pub fn put(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>
}
```

**Concurrent caches:**
```rust
impl<K, V, S> Concurrent[Algorithm]<K, V, S> {
    pub fn put(&self, key: K, value: V, size: u64) -> Option<(K, V)>
}
```

**SIZE_UNIT Constant:**
```rust
/// Size value for entry-count mode where actual size doesn't matter.
/// Use this when you only want to limit the number of entries, not total size.
pub const SIZE_UNIT: u64 = 1;
```

### Return Type Consideration

**Current inconsistency:**
- Most algorithms return `Option<(K, V)>` from `put()`
- GDSF returns `Option<V>` from `put()` (no key returned)

**Decision**: Keep GDSF's return type as `Option<V>` for backward compatibility within this change. A separate enhancement could unify return types later if desired.

### Migration Path

```rust
// Before (LRU, LFU, LFUDA, SLRU)
cache.put("key", value);                    // count-based
cache.put_with_size("key", value, 1000);    // size-based

// After (all algorithms)
cache.put("key", value, SIZE_UNIT);         // count-based (explicit)
cache.put("key", value, 1000);              // size-based (no wrapping)

// Before (GDSF only)
cache.put("key", value, 1000);              // size required

// After (GDSF) - no change needed for size-based calls
cache.put("key", value, SIZE_UNIT);         // count-based (new capability)
cache.put("key", value, 1000);              // size-based (unchanged)
```

---

## Implementation Strategy

### Phase 1: Single-Threaded Caches

Modify these files:

| File | Current Methods | New Signature |
|------|-----------------|---------------|
| `src/lru.rs` | `put()`, `put_with_size()` | `put(k, v, size: u64)` |
| `src/lfu.rs` | `put()`, `put_with_size()` | `put(k, v, size: u64)` |
| `src/lfuda.rs` | `put()`, `put_with_size()` | `put(k, v, size: u64)` |
| `src/slru.rs` | `put()`, `put_with_size()` | `put(k, v, size: u64)` |
| `src/gdsf.rs` | `put(k, v, size)` | `put(k, v, size: u64)` |

**Implementation pattern for LRU/LFU/LFUDA/SLRU:**
```rust
// Segment level
pub(crate) fn put(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>
where
    K: Clone + Hash + Eq,
{
    // Size is now required - use directly
    // ... existing put logic with size ...
}

// Cache wrapper level
pub fn put(&mut self, key: K, value: V, size: u64) -> Option<(K, V)> {
    self.segment.put(key, value, size)
}
```

**Implementation pattern for GDSF:**
```rust
// Segment level
pub(crate) fn put(&mut self, key: K, val: V, size: u64) -> Option<V>
where
    K: Clone,
{
    // Size is now required - use directly
    // ... existing put logic with size ...
}
```

### Phase 2: Concurrent Caches

Modify these files:

| File | Current Methods | New Signature |
|------|-----------------|---------------|
| `src/concurrent/lru.rs` | `put()`, `put_with_size()` | `put(k, v, size: u64)` |
| `src/concurrent/lfu.rs` | `put()`, `put_with_size()` | `put(k, v, size: u64)` |
| `src/concurrent/lfuda.rs` | `put()`, `put_with_size()` | `put(k, v, size: u64)` |
| `src/concurrent/slru.rs` | `put()`, `put_with_size()` | `put(k, v, size: u64)` |
| `src/concurrent/gdsf.rs` | `put(k, v, size)` | `put(k, v, size: u64)` |

### Phase 3: Update Tests

Files requiring updates:

- `tests/correctness_tests.rs` - Update all `put()` calls
- `tests/concurrent_correctness_tests.rs` - Update concurrent cache tests
- `tests/concurrent_stress_tests.rs` - Update stress tests
- `tests/no_std_tests.rs` - Update no_std compatibility tests

**Migration pattern:**
```rust
// Before
cache.put("key", value);
cache.put_with_size("key", value, 100);

// After
cache.put("key", value, None);
cache.put("key", value, Some(100));
```

### Phase 4: Update Examples

Files requiring updates:

- `examples/cache_comparison.rs`
- `examples/concurrent_usage.rs`
- `examples/metrics_demo.rs`

### Phase 5: Update Documentation

Files requiring updates:

- `README.md` - Update API table and usage examples
- Doc comments in all modified source files

### Phase 6: Update Cache Simulator

The `cache-simulator/` tracks `put_with_size` operations separately in latency stats. This needs updating:

- `cache-simulator/src/stats.rs` - Remove separate `put_with_size_stats` tracking (merge into `put_stats`)
- `cache-simulator/src/runner.rs` - Update cache operation calls

---

## Removed Methods

The following methods will be DELETED:

```rust
// Single-threaded (public API)
LruCache::put_with_size()
LfuCache::put_with_size()
LfudaCache::put_with_size()
SlruCache::put_with_size()

// Concurrent (public API)
ConcurrentLru::put_with_size()
ConcurrentLfu::put_with_size()
ConcurrentLfuda::put_with_size()
ConcurrentSlru::put_with_size()

// Segment level (internal)
LruSegment::put_with_size()
LfuSegment::put_with_size()
LfudaSegment::put_with_size()
SlruSegment::put_with_size()
```

Also remove: `estimate_object_size()` methods become unnecessary (can be deleted or kept internal).

---

## Size Parameter Analysis: Why Not Default to Value Size?

### The Problem with `None → 1`

The current implementation defaults `size` to `1` when `None` is passed. This is **problematic** for several reasons:

1. **Memory Budget Violation**: If `max_size = 10MB` and entries are actually 1MB each:
   - Cache thinks each entry is 1 byte
   - Allows 10 million entries before hitting `max_size`
   - Actual memory: 10TB → OOM

2. **Size-Aware Algorithm Defeat**: GDSF and LFUDA use size for eviction priority:
   ```
   GDSF priority = frequency / size
   ```
   With `size=1`, all entries have equal size weight, defeating the algorithm's purpose.

3. **Silent Failure Mode**: No warning when using `None` with size-constrained caches.

### Why Can't We Use `size_of::<V>()` as Default?

Rust's type system cannot determine the "true" memory footprint of heap-allocated types:

```rust
// std::mem::size_of::<V>() only measures STACK size, not heap
size_of::<String>()     // → 24 bytes (3 pointers), regardless of string length
size_of::<Vec<u8>>()    // → 24 bytes, regardless of capacity
size_of::<Box<[u8]>>()  // → 16 bytes, regardless of slice length

// A 1MB String still reports 24 bytes
let s = "x".repeat(1_000_000);
size_of_val(&s)  // → 24 bytes (WRONG for cache sizing!)
```

### Alternative Approaches Considered

| Approach | Pros | Cons |
|----------|------|------|
| **A. Make size required** | Forces explicit decision; no silent failures | More verbose for count-based use |
| **B. `Sizable` trait** | Auto-compute for implementing types | Adds trait bound; breaks generic V; no_std complexity |
| **C. `size_of::<V>()` default** | Automatic, no trait needed | Wrong for heap types; misleading |
| **D. Weigher function** (like Moka) | Consistent per-cache sizing | Major API redesign; closure complexity |
| **E. Keep `None → 1`** | Simple; backward compatible | Silent failures; misleading semantics |

### Recommended Solution: Make Size Required

**Change the API to require size explicitly:**

```rust
// NEW: Size is required (u64, not Option<u64>)
pub fn put(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>

// Provide a convenience constant for entry-count mode
pub const SIZE_UNIT: u64 = 1;
```

**Usage patterns:**

```rust
// Entry-count mode (just limit number of entries)
cache.put("key", value, SIZE_UNIT);

// Size-aware mode (track actual memory/disk usage)
cache.put("key", data, data.len() as u64);
cache.put("key", response, response.body.len() as u64);

// For structs, user computes size
cache.put("key", user, size_of::<User>() as u64 + user.name.len() as u64);
```

### Rationale

1. **Explicit is better than implicit**: Users MUST think about what size means for their workload.

2. **No silent failures**: Can't accidentally use wrong size with `None`.

3. **Cache semantics documented**: Using `SIZE_UNIT` explicitly signals "I want count-based caching."

4. **Real-world alignment**: 
   - In-memory caches: User knows actual heap allocation
   - Disk/external caches: User knows external resource size
   - Neither case can be inferred from `V`'s type

5. **No_std compatible**: No traits or closures required.

### Migration from `Option<u64>` to `u64`

```rust
// Before (problematic)
cache.put("key", value, None);           // Silent size=1, may cause OOM
cache.put("key", value, Some(1000));     // Explicit size

// After (explicit)
cache.put("key", value, SIZE_UNIT);      // Clearly indicates count-based
cache.put("key", value, 1000);           // Size-based, no wrapping needed
```

### Implementation Impact

If we proceed with required size:

1. **API Signature**: `put(key, value, size: u64)` instead of `put(key, value, size: Option<u64>)`
2. **Add constant**: `pub const SIZE_UNIT: u64 = 1;` in lib.rs or dedicated module
3. **Update all call sites**: Replace `None` with `SIZE_UNIT`, unwrap `Some(x)` to `x`
4. **Documentation**: Clearly explain when to use `SIZE_UNIT` vs actual size

---

## Performance Impact

**No performance impact expected:**
- The unified `put()` simply uses `unwrap_or(1)` which is a trivial operation
- Existing `put()` already called `put_with_size()` internally
- No additional allocations or computations

**Complexity remains O(1)** for all operations.

---

## Breaking Change Documentation

Add to CHANGELOG.md:

```markdown
## [Unreleased] - Breaking Changes

### Changed
- **BREAKING**: Unified `put()` API across all cache algorithms
  - Signature changed from `put(key, value)` to `put(key, value, size: Option<u64>)`
  - `put_with_size(key, value, size)` method removed - use `put(key, value, Some(size))` instead
  - GDSF now accepts `None` for size (defaults to 1) instead of requiring size parameter

### Migration Guide
```rust
// Count-based caching (before)
cache.put("key", value);
// Count-based caching (after)  
cache.put("key", value, None);

// Size-based caching (before)
cache.put_with_size("key", value, 1000);
// Size-based caching (after)
cache.put("key", value, Some(1000));
```
```

---

## Validation Checklist

After implementation, verify:

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo clippy --all-targets --features concurrent -- -D warnings` passes
- [ ] `cargo clippy --all-targets --features std,concurrent -- -D warnings` passes
- [ ] `cargo test --features "std,concurrent"` passes
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --features concurrent` passes
- [ ] All examples compile and run correctly
- [ ] Cache simulator compiles and runs correctly

---

## Files to Modify Summary

| Category | Files |
|----------|-------|
| **Single-threaded caches** | `src/lru.rs`, `src/lfu.rs`, `src/lfuda.rs`, `src/slru.rs`, `src/gdsf.rs` |
| **Concurrent caches** | `src/concurrent/lru.rs`, `src/concurrent/lfu.rs`, `src/concurrent/lfuda.rs`, `src/concurrent/slru.rs`, `src/concurrent/gdsf.rs` |
| **Tests** | `tests/correctness_tests.rs`, `tests/concurrent_correctness_tests.rs`, `tests/concurrent_stress_tests.rs`, `tests/no_std_tests.rs` |
| **Examples** | `examples/cache_comparison.rs`, `examples/concurrent_usage.rs`, `examples/metrics_demo.rs` |
| **Documentation** | `README.md`, `CHANGELOG.md` |
| **Simulator** | `cache-simulator/src/stats.rs`, `cache-simulator/src/runner.rs` |

**Estimated scope**: ~15 files, primarily mechanical search-and-replace with signature changes.
