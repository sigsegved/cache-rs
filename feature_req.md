# PCONN: Replace `lru` Crate with `cache-rs` + Add Cargo Benchmarks

**Status:** DRAFT (Rev 2 — updated with owner feedback)  
**Work Type:** ENHANCEMENT  
**Date:** February 6, 2026

---

## Overview

Replace the `lru` crate (v0.11) used by the `pconn` persistent connection pool library with the `cache-rs` crate — an open-source, `no_std`-compatible cache library supporting multiple eviction algorithms (LRU, SLRU, LFU, LFUDA, GDSF) with built-in metrics. Additionally, introduce `criterion`-based benchmarks for `pconn` to measure performance before and after the migration.

**Key update (Rev 2):** The `cache-rs` crate owner has confirmed willingness to add `pop()`, `pop_r()` (reverse pop), and non-promoting `contains()` to `cache-rs`. Additionally, a new `LruSet` data structure is proposed in `cache-rs` to potentially replace pconn's VecDeque-based `Bucket` type, yielding O(1) targeted removal.

**Affected Components:**
- `cache-rs` (upstream) — add `pop()`, `pop_r()`, `contains()` to `LruCache`; potentially add new `LruSet` type
- `roxy-rust/lib/pconn/` — replace `lru` dependency with `cache-rs`, adapt `PconnCache` internals
- `roxy-rust/lib/pconn/benches/` — new benchmark suite (NEW)
- `roxy-rust/Cargo.toml` — add `cache-rs` workspace dependency

---

## Problem Statement

The `pconn` library currently uses the `lru` crate (v0.11) as its key-level LRU eviction backend. While functional, several factors motivate evaluating a replacement:

1. **Limited algorithm choices**: The `lru` crate only offers standard LRU. `cache-rs` provides 5 algorithms (LRU, SLRU, LFU, LFUDA, GDSF), opening future optimization paths for different workload patterns (e.g., SLRU for scan resistance).
2. **Built-in metrics**: `cache-rs` ships with `CoreCacheMetrics` (hit rate, miss rate, byte hit rate, eviction counts) — pconn currently tracks these manually via `PconnStats`.
3. **`no_std` support**: `cache-rs` is `#![no_std]` with optional `std` and `concurrent` features, reducing dependency footprint.
4. **Dual-limit capacity**: `cache-rs` supports both entry-count and byte-size limits simultaneously, which could enable future memory-budget-based eviction.
5. **No existing benchmarks**: There are no benchmarks for `pconn` today, making it impossible to validate performance characteristics or detect regressions.
6. **O(n) connection removal**: The current `Bucket::remove(connection)` does a linear scan over the VecDeque. With `LruSet`, this becomes O(1).

### Requirements

1. Replace `lru::LruCache` with `cache_rs::LruCache` in `PconnCache` internals
2. Evaluate replacing VecDeque-based `Bucket` with `cache_rs::LruSet`
3. Maintain identical public API and semantics for `PconnCache`
4. All existing unit tests must pass without modification
5. Add criterion-based benchmarks covering: insert, get (hit/miss), remove, eviction, and mixed workloads
6. No behavioral changes — this is a like-for-like backend swap
7. Local path dependency for `cache-rs` during development, switch to crates.io for release

---

## Background: Current Implementation

### Architecture

```
PconnCache
├── cache: lru::LruCache<PconnKey, Bucket>   ← key-level LRU (this changes)
├── total_connections: usize
├── config: PconnConfig { max_per_key, max_keys }
└── stats: PconnStats { hits, misses, stores, evictions, ... }

Bucket (VecDeque<*mut c_void>)
├── entries: VecDeque<*mut c_void>   ← connection-level LIFO stack (unchanged)
└── max_per_key: usize
```

### `lru` Crate API Methods Used by `pconn`

| `lru::LruCache` method | Where used in `cache.rs` | Purpose |
|------------------------|--------------------------|---------|
| `LruCache::new(cap)` | `PconnCache::new()` | Create cache with NonZeroUsize capacity |
| `.get_mut(key)` | `get()`, `insert()`, `remove()` | Lookup key + promote to MRU |
| `.contains(key)` | `insert()` | Check if key exists (without promoting) |
| `.pop_lru()` | `insert()` | Evict LRU key + get its bucket |
| `.pop(key)` | `get()`, `remove()` | Remove specific key by reference |
| `.put(key, value)` | `insert()` | Insert key-value, auto-evicts if at capacity |
| `.len()` | Multiple | Current entry count |

### Key Design Insight

The `lru` crate is used **only for key-level management** (mapping `PconnKey → Bucket`). Within each bucket, connections are stored in a `VecDeque` with LIFO semantics. The LRU crate handles:
- Key ordering (promote on access)
- Key eviction (pop_lru when max_keys reached)
- Key existence checks

---

## Design Options Comparison

### Prerequisites: cache-rs Upstream Additions

Before evaluating pconn design options, the following additions to `cache-rs` are agreed upon with the crate owner:

| Addition | API | Purpose |
|----------|-----|---------|
| `pop()` | `fn pop(&mut self) -> Option<(K, V)>` | Pop LRU (least recently used) entry |
| `pop_r()` | `fn pop_r(&mut self) -> Option<(K, V)>` | Pop MRU (most recently used) entry |
| `contains()` | `fn contains<Q>(&self, key: &Q) -> bool` | Non-promoting existence check |
| `LruSet<T>` | New data structure (see Option B) | Ordered set with LRU semantics, no map overhead |

These additions are **dependencies** for the pconn migration and should be implemented in `cache-rs` first.

---

### Option A: `LruCache<PconnKey, Bucket>` — Same Two-Tier Architecture

Replace `lru::LruCache` with `cache_rs::LruCache` while keeping the VecDeque-based `Bucket` as-is.

```
LruCache<PconnKey, Bucket>
  └── Bucket = VecDeque<*mut c_void>   ← unchanged
```

**How it works:**
1. Swap `lru` import for `cache_rs::LruCache`
2. Use new `pop()` for LRU key eviction (was `pop_lru()`)
3. Use new `contains()` for non-promoting key check (was `contains()`)
4. Use `remove()` for specific key removal (was `pop()`)
5. Bucket internals unchanged

**API mapping with upstream additions:**

| `lru` crate | `cache-rs` (with additions) | Notes |
|-------------|---------------------------|-------|
| `LruCache::new(cap)` | `LruCache::init(config, None)` | Config-based construction |
| `.get_mut(key)` | `.get_mut(key)` | ✅ Direct |
| `.contains(key)` | `.contains(key)` | ✅ Direct (new, non-promoting) |
| `.pop_lru()` | `.pop()` | ✅ Direct (new) |
| `.pop(key)` | `.remove(key)` | ✅ Direct |
| `.put(key, val)` | `.put(key, val)` | ✅ Direct |
| `.len()` | `.len()` | ✅ Direct |

**Impact**: Near-zero risk. With the upstream additions, this is essentially a 1:1 API swap. `Bucket` and all its tests remain untouched.

### Option B: `LruCache<PconnKey, LruSet<*mut c_void>>` — Full cache-rs Stack

Replace both the key-level cache **and** the connection-level bucket with cache-rs types. `LruSet` is a new data structure to be added to cache-rs.

```
LruCache<PconnKey, LruSet<*mut c_void>>
  └── LruSet = doubly-linked list + hashbrown set (no map, no values)
```

**What is `LruSet`?**

An `LruSet<T>` is an **ordered set** — it holds unique items (no key-value pairs) with LRU ordering. Internally: `List<T>` + `HashSet<*mut ListEntry<T>>` for O(1) lookup by item. Think of it as "LruCache without values" — just ordered membership tracking.

| `LruSet` method | Semantics | Time | Replaces `Bucket` method |
|-----------------|-----------|------|--------------------------|
| `insert(item)` | Add at MRU position; if full, evict LRU and return it | O(1) | `Bucket::add(conn)` |
| `pop_r()` | Remove + return MRU item | O(1) | `Bucket::get()` (LIFO) |
| `pop()` | Remove + return LRU item | O(1) | Eviction in `Bucket::add()` |
| `remove(item)` | Remove specific item by identity | **O(1)** | `Bucket::remove(conn)` — **was O(n)** |
| `contains(item)` | Check membership | O(1) | n/a |
| `drain()` | Iterate + remove all | O(n) | `Bucket::drain()` |
| `len()` / `is_empty()` | Size queries | O(1) | Same |

**How it works:**
1. Replace `lru::LruCache` with `cache_rs::LruCache`
2. Replace `Bucket` (VecDeque) with `cache_rs::LruSet<*mut c_void>`
3. `Bucket::get()` (pop LIFO) → `LruSet::pop_r()` (pop MRU)
4. `Bucket::add(conn)` → `LruSet::insert(conn)` (auto-evicts LRU if at capacity)
5. `Bucket::remove(conn)` → `LruSet::remove(conn)` — **O(1) instead of O(n)**
6. `Bucket::drain()` → `LruSet::drain()`

**Key semantic mapping — pconn Bucket vs LruSet:**

| Bucket concept | LruSet mapping | Works? |
|----------------|---------------|--------|
| Newest at back (MRU) | MRU is head of list | ✅ |
| `get()` = pop from back (LIFO) | `pop_r()` = pop MRU | ✅ Semantically identical |
| `add()` = push to back | `insert()` = add at MRU | ✅ |
| `add()` evicts front (LRU) | `insert()` evicts LRU | ✅ |
| `remove(conn)` = linear scan O(n) | `remove(conn)` = hash lookup O(1) | ✅ **Upgrade** |
| No access promotion (conns are taken, not peeked) | LruSet insert-only, no get-and-promote needed | ✅ |

**`*mut c_void` as `LruSet` item**: Verified that `*mut c_void` implements `Hash + Eq` based on pointer address. Two pointers with the same address are considered equal.

**LruSet design constraints for `no_std`**: Since cache-rs is `#![no_std]`, `LruSet` should use the same `hashbrown::HashSet` + `List<T>` internals as `LruCache`, just without the key-value split.

### Option C: `LruCache<PconnKey, LruCache<*mut c_void, ()>>` — Nested LruCache

Use a nested LruCache with `()` values instead of creating a new `LruSet` type.

```
LruCache<PconnKey, LruCache<*mut c_void, ()>>
```

**How it works:**
1. Inner `LruCache<*mut c_void, ()>` behaves like a set
2. `put(conn, ())` to insert
3. `pop_r()` to get MRU (LIFO behavior)
4. `remove(conn)` for targeted removal — O(1)

---

### Comparison Table

| Aspect | Option A: Keep Bucket | Option B: LruSet | Option C: Nested LruCache |
|--------|----------------------|------------------|---------------------------|
| `Bucket::remove()` complexity | **O(n)** linear scan | **O(1)** hash lookup | **O(1)** hash lookup |
| Memory per connection | **~8 bytes** (pointer in VecDeque) | **~56-72 bytes** (list node + set entry) | **~80-96 bytes** (list node + map entry + `()` value) |
| Total memory (1K keys × 100 conns) | **~800 KB** | **~5.6-7.2 MB** | **~8-9.6 MB** |
| Code changes in pconn | Minimal (import swap) | Moderate (remove Bucket type) | Moderate (remove Bucket type) |
| New cache-rs work | `pop()`, `pop_r()`, `contains()` | Above + **LruSet** type | `pop()`, `pop_r()`, `contains()` only |
| Cache-friendliness | ✅ VecDeque is contiguous | ❌ Linked list = pointer chasing | ❌ Linked list = pointer chasing |
| `V: Clone` requirement | Need `Bucket: Clone` | Need `LruSet: Clone` | Need `LruCache: Clone` (complex) |
| Conceptual clarity | Two different systems | ✅ Unified cache-rs everywhere | ⚠️ Nested caches feel odd |
| Wasted map overhead | None (no inner map) | None (set, not map) | ⚠️ HashMap storing `()` values |

---

### Pros and Cons

#### Option A: `LruCache<PconnKey, Bucket>` — Keep Bucket

**Pros:**
1. **Minimal risk** — Bucket is well-tested, VecDeque is battle-hardened
2. **Best memory efficiency** — ~8 bytes per connection (just a pointer in contiguous array)
3. **Best cache-line locality** — VecDeque is contiguous, excellent for sequential access in `get()` validation loops
4. **Minimal code changes** — only `cache.rs` import/constructor changes
5. **Fastest path** — least new code needed in cache-rs (no LruSet design)

**Cons:**
1. **O(n) remove** — `Bucket::remove(conn)` does a linear scan. At max_per_key=100, this is up to 100 pointer comparisons per idle-timer removal
2. **Two mental models** — LruCache for keys + VecDeque for connections = two different data structures to reason about

#### Option B: `LruCache<PconnKey, LruSet<*mut c_void>>` — LruSet

**Pros:**
1. **O(1) targeted removal** — `LruSet::remove(conn)` is hash-lookup, critical for idle-timer callback path
2. **Unified mental model** — everything is cache-rs, LRU ordering everywhere
3. **Eliminates Bucket type** — less pconn code to maintain
4. **Reusable** — `LruSet` is generally useful for cache-rs (other users benefit)
5. **Future-proof** — if connection access tracking is ever needed, LruSet already supports it

**Cons:**
1. **~7-9x more memory per connection** — 56-72 bytes vs 8 bytes. At 100K connections: 5.6-7.2 MB vs 0.8 MB
2. **Worse cache locality** — linked list nodes are heap-allocated, pointer chasing on every access
3. **New data structure** — LruSet needs design, implementation, testing in cache-rs first
4. **Longer timeline** — depends on cache-rs LruSet being production-ready
5. **Overengineered for the use case** — connections within a bucket are equivalent; there's no "access pattern" to track. LIFO from VecDeque is perfectly suited

#### Option C: `LruCache<PconnKey, LruCache<*mut c_void, ()>>`

**Pros:**
1. O(1) targeted removal (same as Option B)
2. No new data structure needed in cache-rs

**Cons:**
1. **Worst memory** — stores `()` values, HashMap overhead for nothing
2. **`V: Clone` cascade** — inner `LruCache` implementing `Clone` is complex
3. **Nested cache semantics are confusing** — "a cache of caches of unit values"
4. **No clear benefit over Option B** — if we're paying linked-list overhead, LruSet is cleaner

---

### Performance Analysis

#### The `remove()` Question

The key performance differentiator is `Bucket::remove(connection)` — called when nginx's idle timer fires for a pooled connection. How often does this matter?

| Scenario | `remove()` calls/sec | O(n) cost (n=100) | Impact |
|----------|---------------------|--------------------|---------| 
| Low traffic (10 idle timers/sec) | 10 | 10 × ~100 comparisons | **Negligible** |
| Medium traffic (100 idle timers/sec) | 100 | 100 × ~100 comparisons | **~10K ops** — still fast |
| High traffic (1000 idle timers/sec) | 1000 | 1000 × ~100 comparisons | **~100K ops** — measurable |

For context: pointer comparison is ~1 ns. At worst case (1000 removals/sec × 100 comparisons): **~100 μs/sec** = 0.01% of a core. This is not a performance bottleneck.

#### Memory Impact at Scale

Production config: `max_keys=1000, max_per_key=100`. Worst case: 100K connections.

| | Option A (VecDeque) | Option B (LruSet) | Delta |
|-|-------|--------|-------|
| Per-connection | 8 bytes | ~64 bytes | +56 bytes |
| 100K connections | 0.8 MB | 6.4 MB | +5.6 MB |
| Per-worker total (including keys) | ~1.2 MB | ~7 MB | +5.8 MB |
| 8 workers | ~9.6 MB | ~56 MB | +46.4 MB |

Note: In practice, pools rarely reach 100% capacity across all keys. Typical utilization is 10-30%, making real-world impact ~1-2 MB per worker.

---

### Recommendation: **Option A — Keep Bucket, with Option B as Future Enhancement**

**Rationale:**

1. **Right-sized solution**: The O(n) `remove()` is not a real bottleneck. At max_per_key=100, linear scan of pointer comparisons takes ~100 nanoseconds. The idle-timer callback interval (seconds) is 7+ orders of magnitude larger.

2. **Memory discipline matters**: In nginx's per-worker model with 8+ workers, the 7-9x memory increase per connection from LruSet adds up. VecDeque's ~8 bytes/connection is ideal for storing raw pointers.

3. **Cache-line efficiency**: VecDeque stores pointers contiguously. The validation loop in `get()` iterates through connections checking liveness — contiguous memory means CPU prefetching works well. Linked-list LruSet would pointer-chase on every step.

4. **Fastest to ship**: With `pop()`, `pop_r()`, and `contains()` added to cache-rs, Option A is a clean 1:1 swap requiring only `cache.rs` changes. Bucket and all its tests remain untouched.

5. **LruSet is independently valuable**: The `LruSet` data structure makes sense in cache-rs regardless of pconn. It can be designed, built, and validated at cache-rs's pace, then adopted by pconn later if profiling shows `remove()` is a bottleneck.

**Phased approach:**
- **Phase 1**: Add `pop()`, `pop_r()`, `contains()` to cache-rs LruCache
- **Phase 2**: Swap pconn from `lru` → `cache-rs` (Option A)
- **Phase 3** (future, if needed): Build `LruSet` in cache-rs, benchmark pconn with it, adopt if beneficial

---

## Architecture (Option A — Recommended)

```
PconnCache (unchanged public API)
├── cache: cache_rs::LruCache<PconnKey, Bucket>   ← CHANGED: cache-rs backend
│           └── LruCacheConfig { capacity: max_keys, max_size: u64::MAX }
├── total_connections: usize                       ← unchanged
├── config: PconnConfig { max_per_key, max_keys }  ← unchanged
└── stats: PconnStats { ... }                      ← unchanged (keep manual stats)

Bucket (unchanged)
├── entries: VecDeque<*mut c_void>
└── max_per_key: usize
```

### Architecture (Option B — Future Enhancement)

```
PconnCache (unchanged public API)
├── cache: cache_rs::LruCache<PconnKey, LruSet<*mut c_void>>   ← both tiers are cache-rs
│           └── LruCacheConfig { capacity: max_keys, max_size: u64::MAX }
│           └── Each value: LruSet { capacity: max_per_key }
├── total_connections: usize
├── config: PconnConfig { max_per_key, max_keys }
└── stats: PconnStats { ... }    

No Bucket type needed — LruSet handles:
  insert(conn)   → add at MRU, evict LRU if full   [was Bucket::add()]
  pop_r()        → pop MRU (LIFO semantics)         [was Bucket::get()]  
  remove(conn)   → O(1) targeted removal            [was Bucket::remove() O(n)]
  drain()        → iterate all                       [was Bucket::drain()]
```

**Note on metrics**: While `cache-rs` has built-in `CoreCacheMetrics`, pconn's `PconnStats` tracks connection-level stats (not just key-level) and has custom counters (`stale_connections`, `key_evictions`). We keep `PconnStats` as-is and do **not** use cache-rs metrics for this change. Future work could align them.

---

## Interface Changes

### Public API: NO CHANGES

`PconnCache`, `PconnConfig`, `PconnKey`, `PconnStats` — all public types and methods remain identical. This is a purely internal backend swap.

### cache-rs Upstream Changes (Dependencies)

These must land in `cache-rs` before pconn migration:

```rust
// New methods on LruCache<K, V>
impl<K: Hash + Eq, V: Clone, S: BuildHasher> LruCache<K, V, S> {
    /// Pop the least recently used entry. Returns `None` if empty.
    pub fn pop(&mut self) -> Option<(K, V)>;

    /// Pop the most recently used entry. Returns `None` if empty.
    pub fn pop_r(&mut self) -> Option<(K, V)>;
}

impl<K: Hash + Eq, V, S: BuildHasher> LruCache<K, V, S> {
    /// Check if key exists without promoting it in the LRU order.
    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: ?Sized + Hash + Eq;
}
```

### Internal Changes in `cache.rs` (Option A)

**Import change:**
```rust
// Before
use lru::LruCache;

// After
use cache_rs::LruCache;
use cache_rs::config::LruCacheConfig;
use core::num::NonZeroUsize;
```

**Constructor change:**
```rust
// Before
LruCache::new(max_keys)  // NonZeroUsize

// After
let config = LruCacheConfig {
    capacity: NonZeroUsize::new(config.max_keys).ok_or(...)?,
    max_size: u64::MAX,  // No size-based eviction, count only
};
LruCache::init(config, None)
```

**Method mapping (`insert()` logic stays structurally identical):**
```rust
// Before                          // After
self.cache.contains(key)      →    self.cache.contains(key)      // same!
self.cache.pop_lru()          →    self.cache.pop()              // renamed
self.cache.pop(key)           →    self.cache.remove(key)        // renamed
self.cache.put(key, val)      →    self.cache.put(key, val)      // same!
self.cache.get_mut(key)       →    self.cache.get_mut(key)       // same!
self.cache.len()              →    self.cache.len()              // same!
```

### Value type constraint

`cache-rs` requires `V: Clone` for `LruCache<K, V>`. `Bucket` does not currently implement `Clone`. We need to either:

1. Derive `Clone` on `Bucket` (makes sense — it's just a `VecDeque<*mut c_void>` + usize)
2. Or implement it manually

Since `VecDeque<*mut c_void>` is `Clone` and `usize` is `Clone`, deriving is trivial and safe. The raw pointers are just addresses — cloning them doesn't create ownership issues since pconn doesn't own the connections.

---

## Dependency Changes

### `roxy-rust/Cargo.toml` (workspace root)

```toml
[workspace.dependencies]
# Add cache-rs — use local path during development
cache-rs = { path = "../../cache-rs" }
# Later for release: cache-rs = "0.3.1"
```

### `roxy-rust/lib/pconn/Cargo.toml`

```toml
[dependencies]
# Remove: lru = { workspace = true }
# Add:
cache-rs = { workspace = true }
```

### Optional: Remove `lru` from workspace

If no other crates use `lru`, remove it from `roxy-rust/Cargo.toml` workspace dependencies. Check first:

| Crate | Uses `lru`? |
|-------|-------------|
| `pconn` | Yes (being replaced) |
| `mds` | **Yes** — also depends on `lru = { workspace = true }` |
| All others | No |

⚠️ **Cannot remove `lru` from workspace** until `mds` is also migrated or continues to use it independently. Both `lru` and `cache-rs` can coexist as workspace dependencies.

---

## Benchmarks Design

### Files to Create

| Path | Purpose |
|------|---------|
| `roxy-rust/lib/pconn/benches/pconn_benchmarks.rs` | Criterion benchmark suite |

### `Cargo.toml` additions for `pconn`

```toml
[dev-dependencies]
criterion = { workspace = true }

[[bench]]
name = "pconn_benchmarks"
harness = false
```

### Benchmark Scenarios

The benchmarks should mirror real-world nginx pconn usage patterns:

| Benchmark | Description | What it measures |
|-----------|-------------|-----------------|
| `insert_single_key` | Insert N connections to one key | Per-connection insert throughput |
| `insert_many_keys` | Insert 1 connection per key, N keys | Key creation + LRU management |
| `get_hit` | Get from populated cache | Hit path latency |
| `get_miss` | Get from key not in cache | Miss path latency |
| `get_with_validation` | Get with validate_fn that checks conn | Validation overhead |
| `remove` | Remove known connection | Remove path latency |
| `insert_eviction_per_key` | Insert to full bucket (max_per_key) | Per-key eviction cost |
| `insert_eviction_key_level` | Insert new key when at max_keys | Key eviction + close_fn cost |
| `mixed_workload` | 60% get, 30% insert, 10% remove | Realistic mixed usage |

### Benchmark Parameters

```
Cache sizes to benchmark:
- Small:  max_keys=10,   max_per_key=10
- Medium: max_keys=100,  max_per_key=100
- Large:  max_keys=1000, max_per_key=100  (production-like)
```

### Key Construction for Benchmarks

Use realistic key sizes (45 bytes, matching IPv6 + tenant_id + ssl flag):
```rust
fn make_bench_key(id: usize) -> PconnKey {
    let mut data = [0u8; 45];
    data[..8].copy_from_slice(&id.to_le_bytes());
    PconnKey::new(&data).unwrap()
}
```

### Benchmark Workflow

1. Run benchmarks **before** the `lru` → `cache-rs` swap: `cargo bench -p pconn`
2. Save results (criterion auto-saves to `target/criterion/`)
3. Apply the `cache-rs` migration
4. Run benchmarks again — criterion auto-compares with previous run
5. Report: per-operation throughput deltas

---

## Files to Modify

### cache-rs (upstream, Phase 0)

| File | Change Description |
|------|-------------------|
| `cache-rs/src/lru.rs` | Add `pop()`, `pop_r()`, `contains()` to `LruSegment` + `LruCache` |
| `cache-rs/src/lru.rs` (tests) | Add test cases for new methods |
| `cache-rs/benches/criterion_benchmarks.rs` | Add benchmarks for `pop()`, `pop_r()`, `contains()` |

### roxy (Phase 1 + Phase 2)

| File | Change Description |
|------|-------------------|
| `roxy-rust/Cargo.toml` | Add `cache-rs` workspace dependency |
| `roxy-rust/lib/pconn/Cargo.toml` | Swap `lru` for `cache-rs`, add `criterion` dev-dependency + bench target |
| `roxy-rust/lib/pconn/src/cache.rs` | Replace `lru::LruCache` with `cache_rs::LruCache`, rename method calls |
| `roxy-rust/lib/pconn/src/types/bucket.rs` | Add `#[derive(Clone)]` to `Bucket` |
| `roxy-rust/lib/pconn/benches/pconn_benchmarks.rs` | **NEW** — criterion benchmark suite |

---

## Backward Compatibility

**Breaking changes:** No

- Public API of `PconnCache`, `PconnConfig`, `PconnKey`, `PconnStats` is unchanged
- All existing tests remain valid
- FFI boundary (if pconn_ffi exists) is unaffected

**Feature flag:** Not needed — this is an internal dependency swap with no behavioral changes.

---

## Test Strategy

### Existing Tests (must pass unchanged)

| Test | Validates |
|------|-----------|
| `test_insert_and_get` | Insert/get round-trip |
| `test_get_miss` | Cache miss path |
| `test_remove` | Connection removal |
| `test_per_key_eviction` | Bucket-level LRU eviction |
| `test_key_eviction` | Key-level LRU eviction |
| `test_key_eviction_respects_recent_use` | LRU ordering correctness |
| `test_max_per_key_override` | Per-key limit override |
| `test_lifo_order` | LIFO connection retrieval |
| `test_stats_reset` | Stats reset behavior |
| `test_get_with_validation` | Validation callback |
| `test_get_all_invalid` | All-stale scenario |
| `test_full_workflow` | End-to-end integration |
| `test_tenant_isolation` | Tenant key isolation |
| `test_new_rejects_zero_max_keys` | Config validation |
| `test_new_rejects_zero_max_per_key` | Config validation |

### Additional Test Cases (if needed)

| # | Scenario | Validates |
|---|----------|-----------|
| 1 | Eviction returns correct bucket via `put()` | Option B eviction handling |
| 2 | Multiple rapid inserts to same key | No regression in hot-key path |

---

## Performance Expectations

| Metric | `lru` crate (baseline) | `cache-rs` Option A (expected) | Notes |
|--------|----------------------|-------------------------------|-------|
| `get` hit | O(1) | O(1) | Both use HashMap + linked list |
| `put` new key | O(1) | O(1) | Similar structure |
| `put` with eviction | O(1) | O(1) | `cache-rs` evicts inline |
| `remove` by key | O(1) | O(1) | Direct map lookup |
| `Bucket::remove(conn)` | O(n) | O(n) | Unchanged in Option A |
| Memory per key entry | ~80 bytes | ~80-90 bytes | `cache-rs` has `CacheEntry` wrapper with timestamps |
| Memory per connection | ~8 bytes (pointer) | ~8 bytes | Unchanged — Bucket is untouched |

**Expected outcome**: Performance should be within ±5% for all operations. Any regression >10% warrants investigation.

### Option B Performance Delta (future reference)

| Metric | Option A (VecDeque Bucket) | Option B (LruSet Bucket) |
|--------|---------------------------|-------------------------|
| `Bucket::remove(conn)` | O(n) — 100ns worst case | **O(1) — ~10ns** |
| Memory per connection | 8 bytes | ~64 bytes |
| `Bucket::get()` cache locality | Contiguous | Pointer chasing |

---

## Implementation Order

1. **Phase 0: cache-rs upstream additions** (DEPENDENCY — blocks Phase 2)
   - Add `pop()` to `LruCache` — pop LRU entry
   - Add `pop_r()` to `LruCache` — pop MRU entry
   - Add `contains()` to `LruCache` — non-promoting existence check
   - Add tests for all three methods
   - Publish new cache-rs version (or use path dependency)

2. **Phase 1: Add pconn benchmarks** (can ship independently, no cache-rs dependency)
   - Add criterion benchmarks to pconn with current `lru` backend
   - Run baseline benchmarks
   - This is independently useful regardless of the cache-rs migration

3. **Phase 2: Swap backend** (depends on Phase 0 + Phase 1)
   - Add `cache-rs` workspace dependency (path = local copy)
   - Derive `Clone` on `Bucket`
   - Replace `lru` with `cache-rs` in `cache.rs` (import + method renames)
   - Run all existing tests
   - Run benchmarks and compare with Phase 1 baseline

4. **Phase 3: Dependency finalization**
   - Decide: path dependency vs crates.io version
   - Keep `lru` in workspace (still used by `mds` crate)
   - Update Cargo.lock

5. **Phase 4 (future, if profiling warrants): LruSet**
   - Design and implement `LruSet<T>` in cache-rs
   - Benchmark pconn with `LruCache<PconnKey, LruSet<*mut c_void>>`
   - Adopt if remove() latency or memory profile justifies it

---

## Open Questions

1. **`mds` crate also depends on `lru`** — Confirmed. Both `lru` and `cache-rs` will coexist as workspace deps. Consider migrating `mds` separately in a future change.
2. **Should we explore SLRU for key eviction?** — SLRU provides scan resistance, which could benefit workloads with many one-off tenants flooding the cache. Now that cache-rs supports it natively, this is easy to evaluate post-migration. Run benchmarks with SLRU key eviction as a follow-up.
3. **Path dependency vs crates.io for `cache-rs`?** — The local copy at `/workspace/cache-rs` is used during development. For CI/release, we need either a crates.io publish (with the new `pop`/`pop_r`/`contains` methods) or a git dependency.
4. **Should `cache-rs` metrics replace/augment `PconnStats`?** — Currently keeping both separate. Future work could unify key-level metrics from cache-rs with connection-level metrics in PconnStats.
5. **LruSet priority** — Should LruSet be designed in parallel with the pconn migration, or strictly after? Given Option A doesn't need it, it can be a separate workstream. However, designing the LruSet API early allows pconn's benchmark suite to include LruSet comparison benchmarks from the start.

---

## References

- Current `lru` crate: https://crates.io/crates/lru (v0.11)
- `cache-rs` crate: https://crates.io/crates/cache-rs (v0.3.1)
- Local cache-rs copy: `/workspace/cache-rs`
- Existing benchmark pattern: `roxy-rust/lib/stormbreaker_afd/benches/top_talker_benchmarks.rs`
- pconn source: `roxy-rust/lib/pconn/src/cache.rs`
