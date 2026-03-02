# API Surface Rationalization: `pop()`, `pop_r()`, and Eviction Listener

## Enhancement Overview

**Feature Name**: Public API surface review for `pop()`/`pop_r()` + eviction listener design direction
**Scope**: All 5 algorithms (LRU, LFU, LFUDA, SLRU, GDSF) + concurrent variants
**Backward Compatibility**: Breaking — `pop_r()` demoted from public API
**Work Type**: ENHANCEMENT

### Problem Statement

PR #33 added `pop()`, `pop_r()`, `contains()`, and `peek()` to all cache algorithms and their concurrent variants. The question is whether `pop()`/`pop_r()` are the right API primitives, or whether an eviction listener (callback-based pattern) would better serve consumers — and whether both can coexist.

This spec answers three questions:
1. Should `pop()` remain public? **Yes.**
2. Should `pop_r()` remain public? **No — demote to `pub(crate)`.**
3. Should an eviction listener replace either? **No — it's complementary, design alongside TTL.**

### Use Cases

1. **`lru` crate migration (pconn)**: Consumer calls `pop()` before `put()` to inspect the eviction candidate and perform cleanup (count connections in evicted bucket). This is the primary motivating use case from `feature_req.md`.
2. **Manual cache draining**: Consumer pops entries one-by-one to gracefully shut down or transfer cache contents.
3. **Eviction notification**: Consumer wants to react when `put()` auto-evicts an entry (cleanup, persistence, logging). This is the eviction listener use case — **not served by `pop()`**.

---

## Industry Survey

| Library | `pop()` (remove eviction candidate) | `pop_r()` (remove most valuable) | Eviction Listener |
|---------|--------------------------------------|----------------------------------|-------------------|
| **Caffeine** (Java) | No | No | Yes — `RemovalListener` with `RemovalCause` enum |
| **Moka** (Rust) | No | No | Yes — `eviction_listener(key, val, cause)` |
| **lru crate** (Rust) | Yes — `pop_lru()` | No | No |
| **quick-cache** (Rust) | No | No | No |
| **Redis** | No (policy-driven) | No | Yes — keyspace notifications |
| **CacheLib** (Meta, C++) | No | No | Yes — `RemovalCallback` |

**Key findings**:
- No major cache library exposes "remove the most valuable entry" (`pop_r()`).
- `pop()` (remove eviction candidate) exists only in the simple `lru` crate, which is cache-rs's primary migration target.
- All high-performance libraries (Caffeine, Moka, CacheLib) use eviction listeners instead of imperative pop — but these are reactive (fire after eviction), not imperative (fire before).

---

## Design Decisions

### Decision 1: Keep `pop()` Public

**Context**: `pop()` removes and returns the eviction candidate (LRU entry, lowest-frequency entry, etc.). It gives consumers imperative control over when eviction happens.

**Option A: Keep `pop()` public** ← Recommended
- **Justification**: Direct migration path from `lru::pop_lru()`. The pconn use case (inspect-then-evict) cannot be cleanly replaced by an eviction listener because the consumer needs the evicted value *before* inserting. In Rust, capturing `&mut` state in a stored closure is painful compared to Java/Kotlin.
- **API cost**: Low — one method per algorithm, clear semantics.
- **Performance cost**: Zero — it's the same code path as internal eviction.

**Option B: Remove `pop()`, rely on eviction listener only**
- **Problem**: Eviction listener fires *after* `put()` auto-evicts. The pconn pattern needs to evict *before* `put()` and inspect the result. Restructuring this into callback form requires `Arc<Mutex<State>>` or similar, adding complexity.
- **Problem**: Eviction listener requires `Box<dyn Fn>` or generics, adding binary size and compile time. Not ideal for `no_std`.

**Decision**: **Keep `pop()` public on all 5 algorithms + concurrent variants.**

### Decision 2: Demote `pop_r()` to `pub(crate)`

**Context**: `pop_r()` removes and returns the *most valuable* entry — the MRU item (LRU), highest-frequency item (LFU), highest-priority item (LFUDA/GDSF), or MRU from protected segment (SLRU).

**Option A: Keep `pop_r()` public**
- No industry precedent — zero major cache libraries expose this.
- Semantically incoherent: "remove the entry you most want to keep" is an anti-pattern for a cache.
- The only motivating use case was `LruSet::pop_r()` for LIFO pconn connection retrieval, but `feature_req.md` recommends Option A (keep VecDeque, skip LruSet). Speculative.
- TOCTOU hazard in concurrent variants (documented in commit `345853b`): sequential segment scan can't guarantee globally "most valuable" entry is returned.

**Option B: Demote `pop_r()` to `pub(crate)`** ← Recommended
- Preserves internal capability for future `LruSet` if needed.
- Removes confusing public API surface.
- No consumer exists for this method today.
- Concurrent variants can also drop `pop_r()` from public API.

**Option C: Remove `pop_r()` entirely**
- Risk: If `LruSet` is ever built, we'd need to re-implement it.
- Unnecessary — `pub(crate)` achieves the same API cleanliness without code deletion.

**Decision**: **Demote `pop_r()` to `pub(crate)` on all 5 algorithms. Remove from concurrent public API.**

### Decision 3: Eviction Listener — Design Alongside TTL (Future)

**Context**: Should an eviction listener be implemented now as a replacement for `pop()`/`pop_r()`?

**Option A: Implement eviction listener now**
- Premature — no TTL support yet, so the only eviction cause is `Size` (capacity-based).
- Adds closure storage overhead to every cache instance.
- `no_std` compatibility constrains the callback type (no `Box<dyn Fn>` without `alloc`... but we do have `alloc`).
- Forced to make design decisions about `RemovalCause` enum before TTL/expiration semantics are defined.

**Option B: Design eviction listener alongside TTL** ← Recommended
- Natural pairing — TTL introduces `Expired` as a removal cause, making the `RemovalCause` enum meaningful.
- Caffeine, Moka, and CacheLib all pair eviction listener with TTL.
- Allows a single, well-considered callback API instead of iterating on it.
- `pop()` covers the immediate pconn need without the callback machinery.

**Option C: Never add eviction listener, rely on `pop()` only**
- Insufficient for reactive patterns (logging, metrics, persistence on eviction).
- Out of step with every major cache library.

**Decision**: **Defer eviction listener to TTL work. Document the intended `RemovalCause` enum shape below for forward compatibility.**

---

## Implementation Strategy

### Affected Components

- [x] Core algorithms: LRU, LFU, LFUDA, SLRU, GDSF — change `pop_r()` visibility
- [x] Concurrent variants: all 5 — remove `pop_r()` from public API
- [ ] Configuration structs: No changes
- [ ] Metadata system: No changes
- [ ] Metrics: No changes (metrics already correctly separate pop from eviction)

### Algorithm-Specific Considerations

| Algorithm | `pop()` semantics (unchanged) | `pop_r()` change |
|-----------|-------------------------------|-------------------|
| **LRU** | Remove LRU (tail of list) | `pub` → `pub(crate)`: remove MRU (head of list) |
| **LFU** | Remove lowest-frequency entry | `pub` → `pub(crate)`: remove highest-frequency entry |
| **LFUDA** | Remove lowest-priority entry | `pub` → `pub(crate)`: remove highest-priority entry |
| **SLRU** | Remove LRU from probationary | `pub` → `pub(crate)`: remove MRU from protected |
| **GDSF** | Remove lowest-priority entry | `pub` → `pub(crate)`: remove highest-priority entry |

### Concurrent Variant Changes

| Concurrent Algorithm | `pop()` (keep public) | `pop_r()` change |
|---------------------|----------------------|-------------------|
| `ConcurrentLruCache` | Keep | Remove from public API |
| `ConcurrentLfuCache` | Keep | Remove from public API |
| `ConcurrentLfudaCache` | Keep | Remove from public API |
| `ConcurrentSlruCache` | Keep | Remove from public API |
| `ConcurrentGdsfCache` | Keep | Remove from public API |

For concurrent variants, `pop_r()` has an additional TOCTOU problem: segments are scanned sequentially, so the "most valuable" entry across all segments can't be atomically identified. The current implementation (documented in commit `345853b`) acknowledges this. Removing it from the public API eliminates this confusing guarantee.

### Performance Impact

**Expected Overhead**: Zero — this is purely a visibility change.

**No changes to**:
- Data structures
- Memory layout
- Unsafe code
- Algorithmic complexity
- Benchmarks

---

## Forward-Compatible Eviction Listener Design (Informational)

This section documents the intended eviction listener design for when TTL is implemented. It is NOT part of the current change but informs API decisions (e.g., keeping `pop()` and listener as complementary, not competing).

### Intended `RemovalCause` Enum

```rust
/// Reason an entry was removed from the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalCause {
    /// Entry was evicted to make room (capacity or size limit exceeded).
    Size,
    /// Entry was explicitly removed via `remove()` or `clear()`.
    Explicit,
    /// Entry was replaced by a `put()` with the same key.
    Replaced,
    /// Entry's TTL expired (future — requires TTL support).
    Expired,
}
```

### Intended Listener API Shape

```rust
// Single-threaded caches — generic callback, no Box<dyn Fn> overhead
pub struct LruCacheConfig<F = fn(K, V, RemovalCause)> {
    pub capacity: NonZeroUsize,
    pub max_size: u64,
    pub removal_listener: Option<F>,  // Monomorphized, no vtable
}

// Concurrent caches — need Send + Sync bound
pub struct ConcurrentLruCacheConfig<F = fn(K, V, RemovalCause)>
where
    F: Fn(K, V, RemovalCause) + Send + Sync,
{
    pub capacity: NonZeroUsize,
    pub max_size: u64,
    pub removal_listener: Option<F>,
}
```

### Relationship Between `pop()` and Eviction Listener

| Operation | Fires listener? | `RemovalCause` |
|-----------|----------------|----------------|
| `put()` auto-evicts | Yes | `Size` |
| `put()` replaces existing key | Yes | `Replaced` |
| `remove()` | Yes | `Explicit` |
| `clear()` | Yes (for each entry) | `Explicit` |
| `pop()` | **No** | N/A — consumer has the value, they handle it |
| TTL expiration (future) | Yes | `Expired` |

**Key design point**: `pop()` does NOT fire the eviction listener. The consumer explicitly requested the entry — they don't need a callback to tell them about it. This is consistent with Caffeine's behavior where `Cache.invalidate(key)` fires `RemovalCause.EXPLICIT` but the programmatic removal gives the value back directly.

---

## Implementation Plan

### Phase 1: Demote `pop_r()` Visibility

**Files to modify** (10 files):

Single-threaded caches:
- `src/lru.rs` — change `pub fn pop_r()` to `pub(crate) fn pop_r()`
- `src/lfu.rs` — change `pub fn pop_r()` to `pub(crate) fn pop_r()`
- `src/lfuda.rs` — change `pub fn pop_r()` to `pub(crate) fn pop_r()`
- `src/slru.rs` — change `pub fn pop_r()` to `pub(crate) fn pop_r()`
- `src/gdsf.rs` — change `pub fn pop_r()` to `pub(crate) fn pop_r()`

Concurrent caches:
- `src/concurrent/lru.rs` — remove or hide `pub fn pop_r()`
- `src/concurrent/lfu.rs` — remove or hide `pub fn pop_r()`
- `src/concurrent/lfuda.rs` — remove or hide `pub fn pop_r()`
- `src/concurrent/slru.rs` — remove or hide `pub fn pop_r()`
- `src/concurrent/gdsf.rs` — remove or hide `pub fn pop_r()`

**Approach**: Change visibility keyword only. No logic changes. Doc comments should be updated to reflect internal-only status.

### Phase 2: Update Documentation

- Remove `pop_r()` from public API examples in module-level docs
- Update `README.md` if `pop_r()` is mentioned in the API surface section
- Add a note in CHANGELOG.md about the API change

### Phase 3: Validate

```bash
cargo fmt --all -- --check
cargo clippy --features "std,concurrent" -- -D warnings
cargo test --features "std,concurrent"
cargo doc --no-deps --document-private-items
```

---

## Testing Strategy

### Existing Tests

`pop_r()` tests already exist in all algorithm modules and concurrent modules. Since `pop_r()` is being demoted to `pub(crate)` (not removed), all internal tests continue to work unchanged. Tests that use `pop_r()` from outside the crate (e.g., in `tests/`) would need to be moved to unit tests within the relevant module, or removed if they only test the public API surface.

### Tests to Verify

- [ ] All existing `pop()` tests pass (no changes to `pop()`)
- [ ] All existing `pop_r()` tests pass (visibility change only, internal tests still work)
- [ ] Integration tests in `tests/` that reference `pop_r()` are updated or removed
- [ ] Concurrent `pop()` tests pass
- [ ] No doc-test failures from visibility change

### Edge Cases

- Confirm `pop_r()` is not used in any doc-tests on public items (those would fail)
- Confirm `pop_r()` is not used in `examples/` directory
- Confirm `tests/correctness_tests.rs` and `tests/concurrent_correctness_tests.rs` don't call `pop_r()`

---

## Success Criteria

- [ ] **API Cleanliness**: `pop()` public, `pop_r()` internal-only across all 10 cache types
- [ ] **No Behavioral Change**: Zero logic modifications, visibility-only change
- [ ] **Tests Pass**: Full validation pipeline green
- [ ] **Documentation Updated**: Public docs don't reference `pop_r()`
- [ ] **Forward Compatible**: Eviction listener design documented, `pop()` semantics compatible with future listener

### Validation Checklist

```bash
cargo fmt --all -- --check                              # Formatting
cargo clippy --features "std,concurrent" -- -D warnings # Linting
cargo test --features "std,concurrent"                  # All tests
cargo doc --no-deps --document-private-items            # Docs build
```

---

## Appendix: Why `pop()` and Eviction Listener Coexist

The two APIs serve fundamentally different control flow patterns:

```
pop() — Imperative, synchronous, consumer-driven:
┌──────────┐     ┌───────────┐     ┌───────────┐
│ Consumer  │────▶│  pop()    │────▶│  Inspect  │
│ decides   │     │  returns  │     │  & handle │
│ to evict  │     │  (K, V)   │     │  evicted  │
└──────────┘     └───────────┘     └───────────┘

Eviction Listener — Reactive, callback-driven, cache-driven:
┌──────────┐     ┌───────────┐     ┌───────────┐
│ Consumer  │────▶│  put()    │────▶│  Cache    │──── callback ────▶ listener(K,V,Cause)
│ calls     │     │           │     │  evicts   │
│ put()     │     │           │     │  if full  │
└──────────┘     └───────────┘     └───────────┘
```

These are complementary, not competing. `pop()` is for power users who need imperative control (pconn). Eviction listener is for the common case where consumers just want notifications (logging, metrics, cleanup).
