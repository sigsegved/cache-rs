# Size-Aware Caching: Detailed Design Specification

**Status**: Draft  
**Author**: cache-rs team  
**Date**: January 2026  
**RFC**: cache-rs-002

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Problem Statement](#2-problem-statement)
3. [Goals and Non-Goals](#3-goals-and-non-goals)
4. [Industry Analysis](#4-industry-analysis)
5. [Options Considered](#5-options-considered)
6. [Recommended Approach](#6-recommended-approach)
7. [Detailed Design](#7-detailed-design)
8. [Migration Strategy](#8-migration-strategy)
9. [Implementation Plan](#9-implementation-plan)
10. [Open Questions](#10-open-questions)
11. [References](#11-references)

---

## 1. Executive Summary

This specification proposes adding **inherent size-aware caching** to cache-rs by introducing a unified entry metadata system. Rather than creating separate size-aware cache variants (the original proposal), we recommend making size/weight tracking a core feature of all cache implementations through a `Weigher` trait and per-entry metadata storage.

**Key Decision**: Store weight per-entry (4-8 bytes overhead) to enable O(1) eviction decisions, matching the approach used by Moka, Caffeine, and other production caches.

---

## 2. Problem Statement

### 2.1 Current Behavior

All cache-rs implementations (LRU, LFU, LFUDA, SLRU, GDSF) use **entry-count based capacity**:

```rust
// Current: capacity = number of entries
let cache = LruCache::new(NonZeroUsize::new(100_000).unwrap());
cache.put("small.txt", small_1kb_value);    // Takes 1 slot
cache.put("video.mp4", large_500mb_value);  // Also takes 1 slot!
```

### 2.2 Why This Is Problematic

| Problem | Impact |
|---------|--------|
| **Unpredictable Memory** | A 100K-entry cache could use 100KB to 100TB depending on object sizes |
| **Unfair Benchmarking** | Cannot compare with Moka's weighted mode—apples to oranges |
| **Misleading Simulations** | Simulator generates 1KB-10MB objects but treats them equally |
| **No Memory Bounds** | Cannot guarantee cache stays within RAM budget |

### 2.3 Current Workarounds and Their Limitations

**Workaround 1**: Use GDSF (already has size in priority calculation)
- *Limitation*: Still evicts by entry count, not total size

**Workaround 2**: Estimate size via `mem::size_of`
- *Limitation*: Doesn't capture heap allocations, Vecs, Strings, etc.

**Workaround 3**: Configure Moka without weigher for comparison
- *Limitation*: Doesn't test real-world size-aware scenarios

---

## 3. Goals and Non-Goals

### 3.1 Goals

1. **G1**: Enable size-based eviction across all cache algorithms
2. **G2**: Maintain backward compatibility with entry-count mode
3. **G3**: Provide a unified API pattern matching industry standards (Moka, Caffeine)
4. **G4**: Support custom weight functions via trait/closure
5. **G5**: Enable fair benchmarking against Moka in cache-simulator
6. **G6**: Preserve `no_std` compatibility for core functionality

### 3.2 Non-Goals

1. **NG1**: TTL/expiration support (separate RFC)
2. **NG2**: Automatic memory measurement (requires unsafe introspection)
3. **NG3**: Dynamic weight updates after insertion (complexity vs benefit)
4. **NG4**: Changing the fundamental data structure (HashMap + List)

---

## 4. Industry Analysis

We analyzed five production caching systems to understand how they solve size-aware caching.

### 4.1 Moka (Rust)

**Repository**: [moka-rs/moka](https://github.com/moka-rs/moka)

**Architecture**:
```rust
// Per-entry metadata structure
pub(crate) struct EntryInfo<K> {
    key_hash: KeyHash<K>,
    is_admitted: AtomicBool,
    entry_gen: AtomicU16,
    policy_gen: AtomicU16,
    last_accessed: AtomicInstant,
    last_modified: AtomicInstant,
    policy_weight: AtomicU32,  // ← Weight stored here (4 bytes)
}
```

**API Design**:
```rust
let cache = Cache::builder()
    .max_capacity(32 * 1024 * 1024)  // 32 MB total weight
    .weigher(|_key, value: &String| -> u32 {
        value.len().try_into().unwrap_or(u32::MAX)
    })
    .build();
```

**Key Characteristics**:
- Weight type: `u32` per entry, `u64` for total
- Weigher: `Fn(&K, &V) -> u32 + Send + Sync + 'static`
- Zero-weight items: Never selected for eviction
- Eviction: TinyLFU with size-aware admission control

**Lessons for cache-rs**:
- ✅ Store weight per-entry for O(1) eviction
- ✅ Use builder pattern for configuration
- ✅ `u32` per-entry is sufficient (4GB max per item)
- ✅ Special handling for zero-weight items

### 4.2 Caffeine (Java)

**Repository**: [ben-manes/caffeine](https://github.com/ben-manes/caffeine)

**Architecture**:
```java
// Weigher functional interface
@FunctionalInterface
public interface Weigher<K, V> {
    @NonNegative int weigh(K key, V value);
}

// Node stores weight
interface Node<K, V> {
    int getPolicyWeight();
    void setPolicyWeight(int weight);
}
```

**Eviction Logic** (from `BoundedLocalCache.java`):
```java
void afterWrite(Node<K, V> node, int weight, boolean onlyIfAbsent) {
    if (evicts()) {
        setWeightedSize(weightedSize() + weight);
        node.setPolicyWeight(node.getPolicyWeight() + weight);
        // Evict if over maximum
    }
}
```

**Key Characteristics**:
- Weight type: `int` (32-bit signed)
- Eviction: W-TinyLFU (Window-TinyLFU)
- Weight recomputation: On update, delta applied
- Statistics: `CacheStats.evictionWeight()` for monitoring

**Lessons for cache-rs**:
- ✅ Recompute weight on updates (put with existing key)
- ✅ Track eviction weight in metrics
- ✅ Consider window-based admission

### 4.3 CacheLib (Facebook/Meta, C++)

**Repository**: [facebook/CacheLib](https://github.com/facebook/CacheLib)

**Architecture**:
CacheLib uses a **slab allocator** with predefined size classes:

```cpp
// Size is implicit from allocation class
class CACHELIB_PACKED_ATTR KAllocation {
    // Size encoded in allocation, not stored separately
    uint32_t valSize_ : 24;  // Max ~16MB values
    uint32_t keySize_ : 8;   // Small key optimization
};

// Allocation classes (configured at startup)
std::set<uint32_t> allocSizes{100, 1000, 2000, 5000, ...};
```

**Key Characteristics**:
- Size implicit from slab class (no per-entry overhead)
- Internal fragmentation within size classes
- O(1) allocation within class
- Navy (NVM) uses `sizeHint` compressed to 6-bit exponent

**Lessons for cache-rs**:
- ⚠️ Slab allocation is too complex for our use case
- ✅ Consider size hints for memory efficiency
- ✅ Pre-allocation improves performance

### 4.4 quick_cache (Rust)

**Repository**: [arthurprs/quick-cache](https://github.com/arthurprs/quick-cache)

**Architecture**:
```rust
// Weighter trait (note: u64, not u32)
pub trait Weighter<Key, Val> {
    fn weight(&self, key: &Key, val: &Val) -> u64;
}

// Shard tracks hot/cold weight separately
pub struct CacheShard<...> {
    weight_capacity: u64,
    weight_target_hot: u64,
    weight_hot: u64,      // Hot tier current weight
    weight_cold: u64,     // Cold tier current weight
}
```

**Key Characteristics**:
- Weight computed on demand (not stored per-entry)
- Only 21 bytes overhead per entry (very efficient)
- Segmented LRU with hot/cold tiers
- Pinning support for non-evictable items

**Lessons for cache-rs**:
- 🤔 Computing on demand saves memory but slower eviction
- ✅ Separate hot/cold weight tracking is elegant
- ✅ `u64` weight allows very large objects

### 4.5 Squid Proxy Cache

**Documentation**: [wiki.squid-cache.org](https://wiki.squid-cache.org)

**Size-Aware Replacement Policies**:

| Policy | Formula | Optimizes |
|--------|---------|-----------|
| `heap GDSF` | `priority = (cost × frequency) / size` | Hit rate (favors small objects) |
| `heap LFUDA` | `priority = frequency + age` | Byte hit rate (favors popular objects) |
| `heap LRU` | Standard LRU | Recency |

**Key Characteristics**:
- Object size stored in `StoreEntry` metadata
- GDSF/LFUDA use size in priority calculations
- Dynamic aging prevents cache pollution

**Lessons for cache-rs**:
- ✅ GDSF approach (size in priority) is proven at scale
- ✅ Our GDSF already has this—just need proper integration
- ✅ Squid validates our algorithm choices

### 4.6 Summary Comparison

| System | Weight Storage | Per-Entry Overhead | Weight Type | Eviction |
|--------|---------------|-------------------|-------------|----------|
| **Moka** | Per-entry | +4 bytes | u32/u64 | TinyLFU |
| **Caffeine** | Per-entry | +4 bytes | int (32-bit) | W-TinyLFU |
| **CacheLib** | Implicit (slab) | 0 bytes | N/A | Per-class LRU |
| **quick_cache** | On-demand | 0 bytes | u64 | Segmented LRU |
| **Squid** | Per-entry | +8 bytes | size_t | GDSF/LFUDA |

**Consensus Pattern**: All systems except quick_cache store weight per-entry. The 4-8 byte overhead is universally accepted as worthwhile for O(1) eviction decisions.

---

## 5. Options Considered

### 5.1 Option A: Separate Size-Aware Cache Types

**Description**: Create new types like `SizedLruCache<K, V, W>`, `SizedLfuCache<K, V, W>`, keeping existing types unchanged.

```rust
// New size-aware types alongside existing
pub struct SizedLruCache<K, V, W: Weigher<K, V>> {
    max_weight: u64,
    current_weight: u64,
    weigher: W,
    // ... same internals as LruCache
}

// Existing types unchanged
pub struct LruCache<K, V, S = DefaultHashBuilder> { /* ... */ }
```

**Pros**:
- No breaking changes to existing API
- Clear separation of concerns
- Users choose explicitly

**Cons**:
- **Code duplication**: ~90% of code shared between variants
- **Maintenance burden**: Bug fixes needed in 10 places (5 algorithms × 2 variants)
- **API inconsistency**: Two ways to do similar things
- **Feature interaction**: New features must be added to both variants

**Verdict**: ❌ **Rejected** — Maintenance burden unacceptable

### 5.2 Option B: Weigher as Generic Parameter

**Description**: Add weigher as a generic parameter to existing types.

```rust
pub struct LruCache<K, V, S = DefaultHashBuilder, W = UnitWeigher> {
    weigher: W,
    max_weight: u64,
    current_weight: u64,
    // ...
}

impl<K, V, S, W: Weigher<K, V>> LruCache<K, V, S, W> {
    pub fn with_weigher(cap: u64, weigher: W) -> Self { /* ... */ }
}
```

**Pros**:
- Single implementation per algorithm
- Type-safe weigher configuration
- Zero-cost abstraction (monomorphization)

**Cons**:
- **Complex type signatures**: `LruCache<String, Vec<u8>, DefaultHashBuilder, MyWeigher>`
- **Breaking change**: Existing code needs updates
- **Generic proliferation**: Every function touching cache needs `W` parameter

**Verdict**: ❌ **Rejected** — Type complexity too high

### 5.3 Option C: Unified Entry Metadata (RECOMMENDED)

**Description**: Store weight in entry metadata, use builder pattern for configuration, default to entry-count mode via `UnitWeigher`.

```rust
// Weigher trait (type-erased via Box<dyn> internally)
pub trait Weigher<K, V>: Send + Sync {
    fn weight(&self, key: &K, value: &V) -> u64;
}

// Entry metadata includes weight
struct EntryMetadata<K, V> {
    node: *mut Entry<(K, V)>,
    weight: u64,  // +8 bytes per entry
    // ... algorithm-specific fields
}

// Builder pattern for construction
let cache = LruCache::builder()
    .max_weight(32 * 1024 * 1024)  // 32 MB
    .weigher(|_k, v: &Vec<u8>| v.len() as u64)
    .build();

// Or entry-count mode (backward compatible)
let cache = LruCache::new(NonZeroUsize::new(10_000).unwrap());
```

**Pros**:
- **Single codebase**: One implementation per algorithm
- **Backward compatible**: `new(cap)` still works (uses `UnitWeigher`)
- **Industry standard**: Matches Moka, Caffeine patterns
- **O(1) eviction**: Weight stored, not computed during eviction
- **Clean API**: Builder pattern hides complexity

**Cons**:
- **Memory overhead**: +8 bytes per entry
- **Runtime cost**: Box<dyn Weigher> has indirection (mitigated by caching weight)

**Verdict**: ✅ **Recommended**

### 5.4 Option D: Compute Weight On-Demand

**Description**: Store weigher but compute weight during eviction, not at insertion.

```rust
struct LruSegment<K, V, W: Weigher<K, V>> {
    weigher: W,
    max_weight: u64,
    // NO per-entry weight storage
}

impl<K, V, W: Weigher<K, V>> LruSegment<K, V, W> {
    fn evict_to_fit(&mut self, new_weight: u64) {
        let mut total = self.compute_total_weight(); // O(n)!
        while total + new_weight > self.max_weight {
            let victim = self.find_victim();
            total -= self.weigher.weight(&victim.key, &victim.value); // Called during eviction
            self.remove_victim(victim);
        }
    }
}
```

**Pros**:
- **Zero per-entry overhead**: No weight stored
- **Always current**: Weight never stale

**Cons**:
- **O(n) total weight**: Must iterate to compute total
- **Slower eviction**: Weigher called on hot path
- **Inconsistent weights**: If value mutated via `get_mut`, weight changes unexpectedly

**Verdict**: ❌ **Rejected** — Performance unacceptable for eviction-heavy workloads

### 5.5 Decision Matrix

| Criterion | Option A | Option B | Option C | Option D |
|-----------|----------|----------|----------|----------|
| Code duplication | ❌ High | ✅ None | ✅ None | ✅ None |
| API complexity | ✅ Simple | ❌ Complex | ✅ Simple | ⚠️ Medium |
| Memory overhead | ⚠️ +8 bytes | ⚠️ +8 bytes | ⚠️ +8 bytes | ✅ 0 bytes |
| Eviction speed | ✅ O(1) | ✅ O(1) | ✅ O(1) | ❌ O(n) |
| Backward compatible | ✅ Yes | ❌ No | ✅ Yes | ⚠️ Partial |
| Industry alignment | ⚠️ Partial | ⚠️ Partial | ✅ Full | ❌ Rare |

**Winner**: **Option C** — Best balance of simplicity, performance, and compatibility.

---

## 6. Recommended Approach

### 6.1 Core Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Cache<K, V>                                 │
├─────────────────────────────────────────────────────────────────────┤
│  config: CacheConfig                                                │
│    ├─ max_entries: Option<NonZeroUsize>   // Entry count limit      │
│    ├─ max_weight: Option<u64>             // Total weight limit     │
│    └─ weigher: Box<dyn Weigher<K, V>>     // Weight calculator      │
│                                                                     │
│  state: CacheState                                                  │
│    ├─ current_weight: u64                 // Σ entry weights        │
│    └─ entry_count: usize                  // Number of entries      │
│                                                                     │
│  map: HashMap<K, EntryMetadata>           // O(1) lookup            │
│  ordering: List<CacheEntry<K, V>>         // Algorithm ordering     │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                      EntryMetadata                                  │
├─────────────────────────────────────────────────────────────────────┤
│  node: *mut Entry<CacheEntry<K, V>>   // Pointer to list node       │
│  weight: u64                          // Cached weight value        │
│  // Algorithm-specific fields:                                      │
│  frequency: Option<u64>               // LFU, LFUDA, GDSF           │
│  age_at_insertion: Option<u64>        // LFUDA, GDSF                │
│  priority: Option<f64>                // GDSF                       │
│  location: Option<Location>           // SLRU                       │
└─────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────┐
│                      CacheEntry<K, V>                               │
├─────────────────────────────────────────────────────────────────────┤
│  key: K                               // The cache key              │
│  value: V                             // The cached value           │
└─────────────────────────────────────────────────────────────────────┘
```

### 6.2 Weigher Trait

```rust
/// Calculates the weight (size) of a cache entry.
/// 
/// # Examples
/// 
/// ```rust
/// use cache_rs::Weigher;
/// 
/// // Custom weigher for strings
/// struct StringWeigher;
/// impl Weigher<String, String> for StringWeigher {
///     fn weight(&self, key: &String, value: &String) -> u64 {
///         (key.len() + value.len()) as u64
///     }
/// }
/// ```
pub trait Weigher<K, V>: Send + Sync {
    /// Returns the weight of an entry.
    /// 
    /// # Contract
    /// - Must return a non-negative value
    /// - Should be deterministic for the same key-value pair
    /// - Zero weight means the entry is never evicted (pinned)
    fn weight(&self, key: &K, value: &V) -> u64;
}

/// Default weigher that assigns weight 1 to all entries.
/// This preserves entry-count based eviction behavior.
#[derive(Clone, Copy, Debug, Default)]
pub struct UnitWeigher;

impl<K, V> Weigher<K, V> for UnitWeigher {
    #[inline]
    fn weight(&self, _key: &K, _value: &V) -> u64 {
        1
    }
}

/// Weigher that uses `mem::size_of` for types with known size.
/// Note: Does not account for heap allocations.
#[derive(Clone, Copy, Debug, Default)]
pub struct MemSizeWeigher;

impl<K, V> Weigher<K, V> for MemSizeWeigher {
    #[inline]
    fn weight(&self, _key: &K, _value: &V) -> u64 {
        (core::mem::size_of::<K>() + core::mem::size_of::<V>()) as u64
    }
}

/// Weigher from a closure.
impl<K, V, F> Weigher<K, V> for F
where
    F: Fn(&K, &V) -> u64 + Send + Sync,
{
    #[inline]
    fn weight(&self, key: &K, value: &V) -> u64 {
        self(key, value)
    }
}
```

### 6.3 Builder Pattern API

```rust
impl<K, V> LruCache<K, V> {
    /// Creates a builder for configuring an LRU cache.
    pub fn builder() -> LruCacheBuilder<K, V> {
        LruCacheBuilder::new()
    }
}

pub struct LruCacheBuilder<K, V> {
    max_entries: Option<NonZeroUsize>,
    max_weight: Option<u64>,
    weigher: Option<Box<dyn Weigher<K, V>>>,
    hash_builder: Option<Box<dyn Any>>,
}

impl<K, V> LruCacheBuilder<K, V> {
    /// Sets the maximum number of entries.
    /// If both max_entries and max_weight are set, eviction occurs
    /// when either limit is exceeded.
    pub fn max_entries(mut self, n: NonZeroUsize) -> Self {
        self.max_entries = Some(n);
        self
    }
    
    /// Sets the maximum total weight (e.g., bytes).
    pub fn max_weight(mut self, weight: u64) -> Self {
        self.max_weight = Some(weight);
        self
    }
    
    /// Sets the weigher for calculating entry weights.
    /// If not set, UnitWeigher is used (weight = 1 for all entries).
    pub fn weigher<W: Weigher<K, V> + 'static>(mut self, weigher: W) -> Self {
        self.weigher = Some(Box::new(weigher));
        self
    }
    
    /// Builds the cache.
    /// 
    /// # Panics
    /// Panics if neither max_entries nor max_weight is set.
    pub fn build(self) -> LruCache<K, V>
    where
        K: Hash + Eq + Clone,
        V: Clone,
    {
        assert!(
            self.max_entries.is_some() || self.max_weight.is_some(),
            "Must set either max_entries or max_weight"
        );
        // ... construction logic
    }
}
```

### 6.4 Eviction Logic Changes

**Current** (entry-count only):
```rust
fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
    // Evict if at capacity
    if self.map.len() >= self.cap() {
        self.evict_one();
    }
    // Insert new entry
    self.insert(key, value);
}
```

**Proposed** (weight-aware):
```rust
fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
    let new_weight = self.weigher.weight(&key, &value);
    
    // Reject if single entry exceeds max_weight
    if let Some(max) = self.config.max_weight {
        if new_weight > max {
            return Some((key, value)); // Rejected
        }
    }
    
    // Evict until we have room
    while self.should_evict(new_weight) {
        self.evict_one();
    }
    
    // Insert new entry with cached weight
    self.insert_with_weight(key, value, new_weight);
}

fn should_evict(&self, new_weight: u64) -> bool {
    // Check entry count limit
    if let Some(max_entries) = self.config.max_entries {
        if self.len() >= max_entries.get() {
            return true;
        }
    }
    
    // Check weight limit
    if let Some(max_weight) = self.config.max_weight {
        if self.current_weight + new_weight > max_weight {
            return true;
        }
    }
    
    false
}
```

### 6.5 Algorithm-Specific Considerations

| Algorithm | Current Metadata | Additional Changes |
|-----------|-----------------|-------------------|
| **LRU** | `*mut Entry<(K,V)>` | Add `weight: u64` |
| **LFU** | `(frequency, node)` | Add `weight: u64` |
| **LFUDA** | `{frequency, age, node}` | Add `weight: u64` |
| **SLRU** | `(node, Location)` | Add `weight: u64` |
| **GDSF** | `{frequency, size, priority, node}` | Use existing `size` as `weight` |

**GDSF Special Case**: GDSF already stores `size` and uses it in priority calculation. We'll rename this to `weight` and ensure the weigher is called to populate it, rather than using the current `put(key, value, size)` API with explicit size parameter.

---

## 7. Detailed Design

### 7.1 New Files

| File | Purpose |
|------|---------|
| `src/weigher.rs` | `Weigher` trait, `UnitWeigher`, `MemSizeWeigher` |
| `src/builder.rs` | `CacheBuilder` generic builder pattern |
| `src/entry.rs` | `CacheEntry<K, V>` and `EntryMetadata` structures |

### 7.2 Modified Files

| File | Changes |
|------|---------|
| `src/lru.rs` | Add weight to `LruSegment`, update eviction logic |
| `src/lfu.rs` | Add weight to `FrequencyMetadata`, update eviction |
| `src/lfuda.rs` | Add weight to `EntryMetadata`, update eviction |
| `src/slru.rs` | Add weight tracking, update eviction |
| `src/gdsf.rs` | Rename `size` to `weight`, integrate weigher |
| `src/config/*.rs` | Add `max_weight`, `weigher` options |
| `src/metrics/*.rs` | Add `weighted_size`, `avg_entry_weight` |
| `src/lib.rs` | Export new types |

### 7.3 EntryMetadata Unification

We'll create a unified metadata structure that accommodates all algorithms:

```rust
/// Unified entry metadata for all cache algorithms.
/// 
/// Each algorithm uses a subset of these fields.
/// Unused fields are zero-cost due to Option<T> optimization.
pub(crate) struct EntryMetadata<K, V> {
    /// Pointer to the entry in the ordering list.
    pub node: *mut Entry<CacheEntry<K, V>>,
    
    /// Cached weight of this entry (from weigher).
    pub weight: u64,
    
    /// Access frequency (LFU, LFUDA, GDSF).
    pub frequency: u64,
    
    /// Global age at insertion time (LFUDA, GDSF).
    pub age_at_insertion: u64,
    
    /// Cached priority value (GDSF).
    pub cached_priority: f64,
    
    /// Which segment this entry belongs to (SLRU).
    pub location: Location,
}

/// SLRU segment location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Location {
    #[default]
    Probationary,
    Protected,
}

impl<K, V> Default for EntryMetadata<K, V> {
    fn default() -> Self {
        Self {
            node: core::ptr::null_mut(),
            weight: 1,
            frequency: 0,
            age_at_insertion: 0,
            cached_priority: 0.0,
            location: Location::default(),
        }
    }
}
```

**Memory Analysis**:
- `node`: 8 bytes (pointer)
- `weight`: 8 bytes
- `frequency`: 8 bytes
- `age_at_insertion`: 8 bytes
- `cached_priority`: 8 bytes
- `location`: 1 byte (+ padding)

**Total**: ~48 bytes per entry metadata

**Alternative**: Use algorithm-specific metadata enums to reduce overhead for simpler algorithms:

```rust
pub(crate) enum AlgorithmMetadata {
    Lru,  // No additional data
    Lfu { frequency: u64 },
    Lfuda { frequency: u64, age_at_insertion: u64 },
    Slru { location: Location },
    Gdsf { frequency: u64, age_at_insertion: u64, cached_priority: f64 },
}
```

### 7.4 Metrics Extensions

```rust
impl CacheMetrics for LruCache<K, V> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        let mut m = self.segment.metrics.metrics();
        
        // New weight-related metrics
        m.insert("weighted_size".to_string(), self.weighted_size() as f64);
        m.insert("weight_capacity".to_string(), 
                 self.config.max_weight.unwrap_or(0) as f64);
        m.insert("avg_entry_weight".to_string(), 
                 self.weighted_size() as f64 / self.len().max(1) as f64);
        m.insert("weight_utilization".to_string(),
                 self.weighted_size() as f64 / self.config.max_weight.unwrap_or(1) as f64);
        
        m
    }
}
```

### 7.5 Zero-Weight Handling

Following Moka/Caffeine patterns, entries with weight 0 receive special treatment:

```rust
fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
    let weight = self.weigher.weight(&key, &value);
    
    if weight == 0 {
        // Zero-weight items are "pinned" - never evicted
        // They still count against max_entries but not max_weight
        self.insert_pinned(key, value);
        return None;
    }
    
    // Normal eviction logic for weight > 0
    // ...
}
```

**Use Case**: Zero-weight allows "pinning" critical entries that should never be evicted, such as configuration or metadata.

---

## 8. Migration Strategy

### 8.1 Phase 1: Additive API (Non-Breaking)

Add new APIs without changing existing ones:

```rust
// New builder API
let cache = LruCache::builder()
    .max_weight(1_000_000)
    .weigher(|k, v| v.len() as u64)
    .build();

// Existing API still works (uses UnitWeigher internally)
let cache = LruCache::new(NonZeroUsize::new(1000).unwrap());
```

### 8.2 Phase 2: Deprecation Warnings

```rust
#[deprecated(since = "0.3.0", note = "Use LruCache::builder() instead")]
pub fn new(cap: NonZeroUsize) -> Self {
    Self::builder().max_entries(cap).build()
}
```

### 8.3 Phase 3: API Removal (v1.0)

In the 1.0 release:
- Remove deprecated `new()` constructor
- Builder pattern becomes the only construction method
- Clean, consistent API across all cache types

---

## 9. Implementation Plan

### 9.1 Task Breakdown

| Task | Priority | Effort | Dependencies |
|------|----------|--------|--------------|
| T1: Create `src/weigher.rs` | P0 | S | None |
| T2: Create `src/entry.rs` | P0 | M | None |
| T3: Update `LruSegment` metadata | P0 | M | T1, T2 |
| T4: Update `LruSegment` eviction | P0 | M | T3 |
| T5: Add `LruCacheBuilder` | P0 | M | T4 |
| T6: Update `LfuSegment` | P1 | M | T1, T2 |
| T7: Update `LfudaSegment` | P1 | M | T1, T2 |
| T8: Update `SlruSegment` | P1 | M | T1, T2 |
| T9: Refactor `GdsfSegment` | P1 | M | T1, T2 |
| T10: Update metrics | P2 | S | T3-T9 |
| T11: Update concurrent wrappers | P2 | M | T3-T9 |
| T12: Update cache-simulator | P2 | M | T3-T9 |
| T13: Documentation | P2 | M | All |
| T14: Deprecation warnings | P3 | S | T5 |

**Legend**: P0 = Critical, P1 = High, P2 = Medium, P3 = Low | S = Small, M = Medium, L = Large

### 9.2 Milestones

| Milestone | Tasks | Target |
|-----------|-------|--------|
| M1: Core Infrastructure | T1, T2 | Week 1 |
| M2: LRU Implementation | T3, T4, T5 | Week 2 |
| M3: Other Algorithms | T6, T7, T8, T9 | Week 3-4 |
| M4: Polish & Release | T10-T14 | Week 5 |

### 9.3 Testing Strategy

1. **Unit Tests**: Each algorithm tested with:
   - Entry-count mode (UnitWeigher)
   - Weight mode (custom weigher)
   - Mixed mode (both limits)
   - Zero-weight entries

2. **Integration Tests**: Cache-simulator comparison with Moka

3. **Benchmark Tests**: Performance regression testing

4. **Miri Tests**: Memory safety validation

---

## 10. Open Questions

### 10.1 Resolved Questions

| Question | Decision | Rationale |
|----------|----------|-----------|
| Store weight per-entry or compute on-demand? | Store per-entry | O(1) eviction, industry standard |
| `u32` or `u64` for per-entry weight? | `u64` | Supports very large objects, no overflow risk |
| Unified metadata or algorithm-specific? | Start unified, optimize later | Simpler implementation, measure overhead first |

### 10.2 Open Questions

| Question | Options | Recommendation |
|----------|---------|----------------|
| Should `get_mut` trigger weight recomputation? | (A) Yes, always (B) No, use explicit API (C) Configurable | B - Explicit is safer |
| Support TTL in this RFC? | (A) Yes (B) No, separate RFC | B - Keep scope focused |
| Thread-safe weigher requirement? | (A) Require Send+Sync (B) Optional | A - Matches concurrent cache needs |

---

## 11. References

1. **Moka Cache**: https://github.com/moka-rs/moka
2. **Caffeine**: https://github.com/ben-manes/caffeine
3. **CacheLib**: https://github.com/facebook/CacheLib
4. **quick_cache**: https://github.com/arthurprs/quick-cache
5. **Size-Aware Admission**: https://arxiv.org/abs/2105.08770
6. **GDSF Paper**: "Web Caching and Zipf-like Distributions" (1999)
7. **TinyLFU Paper**: "TinyLFU: A Highly Efficient Cache Admission Policy" (2017)

---

## Appendix A: Example Usage

```rust
use cache_rs::{LruCache, Weigher};
use std::num::NonZeroUsize;

// Example 1: Entry-count mode (backward compatible)
let mut cache = LruCache::new(NonZeroUsize::new(1000).unwrap());
cache.put("key", "value");

// Example 2: Weight-based mode with closure
let mut cache = LruCache::builder()
    .max_weight(32 * 1024 * 1024) // 32 MB
    .weigher(|_key: &String, value: &Vec<u8>| value.len() as u64)
    .build();

cache.put("small".to_string(), vec![0u8; 1024]);      // 1 KB
cache.put("large".to_string(), vec![0u8; 1024 * 1024]); // 1 MB

println!("Weighted size: {} bytes", cache.weighted_size());

// Example 3: Both limits
let mut cache = LruCache::builder()
    .max_entries(NonZeroUsize::new(10_000).unwrap())
    .max_weight(100 * 1024 * 1024) // 100 MB
    .weigher(|_, v: &String| v.len() as u64)
    .build();

// Example 4: Custom weigher struct
struct JsonWeigher;
impl Weigher<String, serde_json::Value> for JsonWeigher {
    fn weight(&self, _key: &String, value: &serde_json::Value) -> u64 {
        // Estimate JSON size
        serde_json::to_string(value)
            .map(|s| s.len() as u64)
            .unwrap_or(64)
    }
}

let mut cache = LruCache::builder()
    .max_weight(10 * 1024 * 1024)
    .weigher(JsonWeigher)
    .build();
```

---

## Appendix B: Benchmark Expectations

Based on quick_cache benchmarks (which uses on-demand weighing), we expect:

| Operation | Current | With Weight Storage | Delta |
|-----------|---------|--------------------| ------|
| `put` (new) | ~800ns | ~850ns | +6% |
| `put` (update) | ~600ns | ~650ns | +8% |
| `get` (hit) | ~50ns | ~50ns | 0% |
| `eviction` | ~1μs | ~1μs | 0% |

**Memory overhead**: +8 bytes per entry = +8MB per million entries

These are acceptable trade-offs for the functionality gained.
