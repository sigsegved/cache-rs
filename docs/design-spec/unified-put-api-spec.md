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

Unify all algorithms to use a single signature:
```rust
pub fn put(&mut self, key: K, value: V, size: Option<u64>) -> Option<(K, V)>
```

When `size` is `None`, default to `1` (count-based caching).

---

## API Design

### New Unified Signature

**Single-threaded caches:**
```rust
impl<K, V, S> [Algorithm]Cache<K, V, S> {
    /// Inserts a key-value pair into the cache.
    ///
    /// # Arguments
    /// * `key` - The key to insert
    /// * `value` - The value to cache  
    /// * `size` - Size of this entry for capacity tracking. `None` defaults to `1`.
    ///
    /// # Returns
    /// Returns `Some((evicted_key, evicted_value))` if an entry was evicted or replaced,
    /// `None` if inserted without eviction.
    pub fn put(&mut self, key: K, value: V, size: Option<u64>) -> Option<(K, V)>
}
```

**Concurrent caches:**
```rust
impl<K, V, S> Concurrent[Algorithm]<K, V, S> {
    pub fn put(&self, key: K, value: V, size: Option<u64>) -> Option<(K, V)>
}
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
cache.put("key", value, None);              // count-based (size=1)
cache.put("key", value, Some(1000));        // size-based

// Before (GDSF only)
cache.put("key", value, 1000);              // size required

// After (GDSF)
cache.put("key", value, None);              // size defaults to 1
cache.put("key", value, Some(1000));        // explicit size
```

---

## Implementation Strategy

### Phase 1: Single-Threaded Caches

Modify these files:

| File | Current Methods | New Signature |
|------|-----------------|---------------|
| `src/lru.rs` | `put()`, `put_with_size()` | `put(k, v, size: Option<u64>)` |
| `src/lfu.rs` | `put()`, `put_with_size()` | `put(k, v, size: Option<u64>)` |
| `src/lfuda.rs` | `put()`, `put_with_size()` | `put(k, v, size: Option<u64>)` |
| `src/slru.rs` | `put()`, `put_with_size()` | `put(k, v, size: Option<u64>)` |
| `src/gdsf.rs` | `put(k, v, size)` | `put(k, v, size: Option<u64>)` |

**Implementation pattern for LRU/LFU/LFUDA/SLRU:**
```rust
// Segment level
pub(crate) fn put(&mut self, key: K, value: V, size: Option<u64>) -> Option<(K, V)>
where
    K: Clone + Hash + Eq,
{
    let object_size = size.unwrap_or(1);
    // ... existing put_with_size logic with object_size ...
}

// Cache wrapper level
pub fn put(&mut self, key: K, value: V, size: Option<u64>) -> Option<(K, V)> {
    self.segment.put(key, value, size)
}
```

**Implementation pattern for GDSF:**
```rust
// Segment level
pub(crate) fn put(&mut self, key: K, val: V, size: Option<u64>) -> Option<V>
where
    K: Clone,
{
    let object_size = size.unwrap_or(1);
    // ... existing put logic with object_size ...
}
```

### Phase 2: Concurrent Caches

Modify these files:

| File | Current Methods | New Signature |
|------|-----------------|---------------|
| `src/concurrent/lru.rs` | `put()`, `put_with_size()` | `put(k, v, size: Option<u64>)` |
| `src/concurrent/lfu.rs` | `put()`, `put_with_size()` | `put(k, v, size: Option<u64>)` |
| `src/concurrent/lfuda.rs` | `put()`, `put_with_size()` | `put(k, v, size: Option<u64>)` |
| `src/concurrent/slru.rs` | `put()`, `put_with_size()` | `put(k, v, size: Option<u64>)` |
| `src/concurrent/gdsf.rs` | `put(k, v, size)` | `put(k, v, size: Option<u64>)` |

### Phase 3: Update Tests

Files requiring updates:

- `tests/correctness_tests.rs` - Update all `put()` and `put_with_size()` calls
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
