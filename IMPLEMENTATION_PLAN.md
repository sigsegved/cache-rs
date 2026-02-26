# Implementation Plan: Add `pop()`, `pop_r()`, and `contains()` to cache-rs

**Status:** COMPLETE  
**Created:** February 8, 2026  
**Target Version:** v0.4.0

---

## Overview

Add three new methods to all cache algorithms in cache-rs to support the `pconn` library migration from the `lru` crate:

| Method | Signature | Purpose |
|--------|-----------|---------|
| `pop()` | `fn pop(&mut self) -> Option<(K, V)>` | Remove and return the eviction candidate (LRU/lowest priority) |
| `pop_r()` | `fn pop_r(&mut self) -> Option<(K, V)>` | Remove and return the MRU/highest priority entry |
| `contains()` | `fn contains<Q>(&self, key: &Q) -> bool` | Non-promoting key existence check |

---

## Task Checklist

### Phase 1: Single-Threaded Cache Implementations

#### 1.1 LRU Cache
- [x] Add `contains()` to `LruSegment`
- [x] Add `contains()` to `LruCache`
- [x] Add `pop()` to `LruSegment` (remove LRU/tail entry)
- [x] Add `pop()` to `LruCache`
- [x] Add `pop_r()` to `LruSegment` (remove MRU/head entry)
- [x] Add `pop_r()` to `LruCache`
- [x] Add unit tests for `pop()`, `pop_r()`, `contains()`

#### 1.2 LFU Cache
- [x] Add `contains()` to `LfuSegment`
- [x] Add `contains()` to `LfuCache`
- [x] Add `pop()` to `LfuSegment` (remove lowest frequency entry)
- [x] Add `pop()` to `LfuCache`
- [x] Add `pop_r()` to `LfuSegment` (remove highest frequency entry)
- [x] Add `pop_r()` to `LfuCache`
- [x] Add unit tests for `pop()`, `pop_r()`, `contains()`

#### 1.3 LFUDA Cache
- [x] Add `contains()` to `LfudaSegment`
- [x] Add `contains()` to `LfudaCache`
- [x] Add `pop()` to `LfudaSegment` (remove lowest priority, update global_age)
- [x] Add `pop()` to `LfudaCache`
- [x] Add `pop_r()` to `LfudaSegment` (remove highest priority entry)
- [x] Add `pop_r()` to `LfudaCache`
- [x] Add unit tests for `pop()`, `pop_r()`, `contains()`

#### 1.4 SLRU Cache
- [x] Add `contains()` to `SlruSegment`
- [x] Add `contains()` to `SlruCache`
- [x] Add `pop()` to `SlruSegment` (remove from probationary first, then protected)
- [x] Add `pop()` to `SlruCache`
- [x] Add `pop_r()` to `SlruSegment` (remove from protected first, then probationary)
- [x] Add `pop_r()` to `SlruCache`
- [x] Add unit tests for `pop()`, `pop_r()`, `contains()`

#### 1.5 GDSF Cache
- [x] Add `contains()` to `GdsfSegment` (note: `contains_key()` exists, add alias)
- [x] Add `contains()` to `GdsfCache`
- [x] Add `pop()` to `GdsfSegment` (remove lowest priority, update global_age)
- [x] Add `pop()` to `GdsfCache`
- [x] Add `pop_r()` to `GdsfSegment` (remove highest priority entry)
- [x] Add `pop_r()` to `GdsfCache`
- [x] Add unit tests for `pop()`, `pop_r()`, `contains()`

### Phase 2: Concurrent Cache Implementations

#### 2.1 Concurrent LRU Cache
- [x] Add `contains()` to `ConcurrentLruCache`
- [x] Add `pop()` to `ConcurrentLruCache`
- [x] Add `pop_r()` to `ConcurrentLruCache`
- [x] Add unit tests

#### 2.2 Concurrent LFU Cache
- [x] Add `contains()` to `ConcurrentLfuCache`
- [x] Add `pop()` to `ConcurrentLfuCache`
- [x] Add `pop_r()` to `ConcurrentLfuCache`
- [x] Add unit tests

#### 2.3 Concurrent LFUDA Cache
- [x] Add `contains()` to `ConcurrentLfudaCache`
- [x] Add `pop()` to `ConcurrentLfudaCache`
- [x] Add `pop_r()` to `ConcurrentLfudaCache`
- [x] Add unit tests

#### 2.4 Concurrent SLRU Cache
- [x] Add `contains()` to `ConcurrentSlruCache`
- [x] Add `pop()` to `ConcurrentSlruCache`
- [x] Add `pop_r()` to `ConcurrentSlruCache`
- [x] Add unit tests

#### 2.5 Concurrent GDSF Cache
- [x] Add `contains()` to `ConcurrentGdsfCache`
- [x] Add `pop()` to `ConcurrentGdsfCache`
- [x] Add `pop_r()` to `ConcurrentGdsfCache`
- [x] Add unit tests

### Phase 3: Documentation & Benchmarks

- [ ] Add criterion benchmarks for `pop()`, `pop_r()`, `contains()`
- [ ] Update CHANGELOG.md
- [ ] Update README.md with new API examples

### Phase 4: Validation

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --features "std,concurrent" -- -D warnings`
- [x] `cargo test --features "std,concurrent"`
- [x] `cargo doc --no-deps --document-private-items`

---

## Implementation Details

### Method Signatures

```rust
// For all caches
impl<K: Hash + Eq, V, S: BuildHasher> Cache<K, V, S> {
    /// Check if key exists without promoting it in the eviction order.
    ///
    /// Unlike `get()`, this method does NOT update access metadata.
    /// Useful for conditional logic without affecting cache behavior.
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq;
}

impl<K: Hash + Eq + Clone, V: Clone, S: BuildHasher> Cache<K, V, S> {
    /// Remove and return the eviction candidate.
    ///
    /// - LRU: least recently used entry
    /// - LFU: lowest frequency entry
    /// - LFUDA: lowest priority entry (also updates global_age)
    /// - SLRU: from probationary segment first, then protected
    /// - GDSF: lowest priority entry (also updates global_age)
    ///
    /// Returns `None` if the cache is empty.
    pub fn pop(&mut self) -> Option<(K, V)>;

    /// Remove and return the most recently used / highest priority entry.
    ///
    /// This is the opposite of `pop()`:
    /// - LRU: most recently used entry
    /// - LFU: highest frequency entry
    /// - LFUDA: highest priority entry
    /// - SLRU: from protected segment first, then probationary
    /// - GDSF: highest priority entry
    ///
    /// Returns `None` if the cache is empty.
    pub fn pop_r(&mut self) -> Option<(K, V)>;
}
```

### Algorithm-Specific Behavior

#### LRU
```
pop()   → remove_last()  → tail (LRU)
pop_r() → remove_first() → head (MRU)
```

#### LFU
```
pop()   → min frequency list → remove_last() from that list
pop_r() → max frequency list → remove_first() from that list
```

#### LFUDA
```
pop()   → min priority list → remove_last() → UPDATE global_age
pop_r() → max priority list → remove_first()
```

#### SLRU
```
pop()   → probationary.remove_last() else protected.remove_last()
pop_r() → protected.remove_first() else probationary.remove_first()
```

#### GDSF
```
pop()   → min priority list → remove_last() → UPDATE global_age
pop_r() → max priority list → remove_first()
```

### Concurrent Cache Behavior

For segmented concurrent caches, `pop()` and `pop_r()` iterate through segments:

```rust
pub fn pop(&self) -> Option<(K, V)> {
    for segment in self.segments.iter() {
        if let Some(entry) = segment.lock().pop() {
            return Some(entry);
        }
    }
    None
}
```

---

## Test Cases

### Basic Functionality
```rust
#[test]
fn test_pop_returns_lru() {
    let mut cache = LruCache::new(NonZeroUsize::new(3).unwrap());
    cache.put("a", 1);
    cache.put("b", 2);
    cache.put("c", 3);
    
    assert_eq!(cache.pop(), Some(("a", 1)));  // LRU
    assert_eq!(cache.len(), 2);
}

#[test]
fn test_pop_r_returns_mru() {
    let mut cache = LruCache::new(NonZeroUsize::new(3).unwrap());
    cache.put("a", 1);
    cache.put("b", 2);
    cache.put("c", 3);
    
    assert_eq!(cache.pop_r(), Some(("c", 3)));  // MRU
    assert_eq!(cache.len(), 2);
}

#[test]
fn test_contains_non_promoting() {
    let mut cache = LruCache::new(NonZeroUsize::new(2).unwrap());
    cache.put("a", 1);
    cache.put("b", 2);
    
    assert!(cache.contains(&"a"));  // Does NOT promote "a"
    
    cache.put("c", 3);  // Should evict "a", not "b"
    assert!(!cache.contains(&"a"));
    assert!(cache.contains(&"b"));
}

#[test]
fn test_pop_empty_cache() {
    let mut cache: LruCache<&str, i32> = LruCache::new(NonZeroUsize::new(2).unwrap());
    assert_eq!(cache.pop(), None);
    assert_eq!(cache.pop_r(), None);
}
```

### Edge Cases
- Pop from single-element cache
- Pop after get() (verify correct ordering)
- Contains on non-existent key
- Pop interleaved with put()

---

## Files to Modify

| File | Additions |
|------|-----------|
| `src/lru.rs` | `contains()`, `pop()`, `pop_r()` on Segment + Cache |
| `src/lfu.rs` | `contains()`, `pop()`, `pop_r()` on Segment + Cache |
| `src/lfuda.rs` | `contains()`, `pop()`, `pop_r()` on Segment + Cache |
| `src/slru.rs` | `contains()`, `pop()`, `pop_r()` on Segment + Cache |
| `src/gdsf.rs` | `contains()`, `pop()`, `pop_r()` on Segment + Cache |
| `src/concurrent/lru.rs` | `contains()`, `pop()`, `pop_r()` |
| `src/concurrent/lfu.rs` | `contains()`, `pop()`, `pop_r()` |
| `src/concurrent/lfuda.rs` | `contains()`, `pop()`, `pop_r()` |
| `src/concurrent/slru.rs` | `contains()`, `pop()`, `pop_r()` |
| `src/concurrent/gdsf.rs` | `contains()`, `pop()`, `pop_r()` |
| `benches/criterion_benchmarks.rs` | Benchmarks for new methods |
| `CHANGELOG.md` | Document new features |

---

## Dependencies

- No new crate dependencies required
- Uses existing `List::remove_first()` and `List::remove_last()` methods

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking existing API | These are additions only, no breaking changes |
| Memory leaks in unsafe pop code | Follow existing eviction patterns, comprehensive tests |
| Incorrect LRU ordering after pop | Test pop interleaved with get/put operations |
| Concurrent pop race conditions | Use same locking patterns as existing methods |

---

## Progress Log

| Date | Task | Status |
|------|------|--------|
| 2026-02-08 | Created implementation plan | ✅ |
| | Phase 1.1: LRU Cache | 🔄 In Progress |
