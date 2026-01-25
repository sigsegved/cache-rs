# Size-Aware Cache Design - Actionable Recommendations

**Date**: January 25, 2026  
**Context**: Review of cache-rs-002 design specification  
**Status**: Recommendations for Design Revision

---

## Quick Summary

The size-aware cache design spec is **well-researched** but has **critical architectural flaws** that must be fixed before implementation. The primary issue is choosing a "unified metadata" approach that imposes 6x memory overhead on simple caches like LRU, contradicting cache-rs's memory-efficiency goals.

**Recommendation**: Adopt a **zero-cost abstraction** approach using generics and algorithm-specific metadata.

---

## Top 5 Critical Issues

### 1. ❌ Memory Overhead Miscalculation (CRITICAL)

**Problem**: The spec claims "+8 bytes per entry" but the unified metadata structure is actually **+40 bytes** (48 total vs 8 current).

**Impact**:
- 1M entry LRU cache: 8 MB → 48 MB (6x increase!)
- Embedded/no_std use cases become impractical
- Contradicts "memory efficient" design goal in README

**Fix**:
```rust
// WRONG (current spec):
struct EntryMetadata<K, V> {  // 48 bytes!
    node: *mut Entry<CacheEntry<K, V>>,
    weight: u64,
    frequency: u64,
    age_at_insertion: u64,
    cached_priority: f64,
    location: Location,
}

// RIGHT (algorithm-specific):
// LRU: 8 bytes (just pointer)
// LFU: 16 bytes (pointer + frequency)  
// GDSF: 40 bytes (all fields)
```

**Action**: Redesign to use algorithm-specific metadata, not unified.

---

### 2. ❌ Type Erasure Cost Not Analyzed (CRITICAL)

**Problem**: The spec proposes `Box<dyn Weigher>` (dynamic dispatch) without benchmarking vs monomorphization.

**Issues**:
- Virtual function call overhead on every `put()`
- Heap allocation for weigher
- Prevents `const fn` construction
- Harder to optimize (compiler can't inline)

**Fix**: Use generic parameter (like quick_cache does):
```rust
pub struct LruCache<K, V, S = DefaultHashBuilder, W = NoWeigher> {
    weigher: W,  // Zero-sized when W = NoWeigher
    // ...
}
```

**Action**: Benchmark both approaches and document tradeoffs.

---

### 3. ⚠️ GDSF Breaking Change Ignored (HIGH)

**Problem**: Current GDSF users call `put(key, value, size)`. Proposed design removes explicit size parameter, breaking all existing code.

**Impact**: All GDSF users must rewrite code on upgrade.

**Fix**: Support both APIs during transition:
```rust
// Keep existing (mark deprecated in v0.3)
pub fn put(&mut self, key: K, value: V, size: u64) -> Option<V> { ... }

// Add new weigher-based API
pub fn put_weighted(&mut self, key: K, value: V) -> Option<V> {
    let size = self.weigher.weight(&key, &value);
    self.put(key, value, size)
}
```

**Action**: Add explicit migration strategy to spec.

---

### 4. ⚠️ Zero-Weight Semantics Underspecified (HIGH)

**Problem**: Spec says "zero weight = pinned" but doesn't address:
- What if cache fills with zero-weight entries?
- Infinite loop in eviction?
- Does zero-weight count toward `max_entries`?

**Fix**: Add explicit limits and error handling:
```rust
fn put(&mut self, key: K, value: V) -> Result<Option<V>, CacheError> {
    let weight = self.weigher.weight(&key, &value);
    
    if weight == 0 {
        if self.pinned_count >= self.config.max_pinned {
            return Err(CacheError::TooManyPinnedEntries);
        }
    }
    // ...
}
```

**Action**: Specify zero-weight behavior completely.

---

### 5. ⚠️ Concurrent Cache Integration Missing (MEDIUM)

**Problem**: No discussion of how weighers work with concurrent caches.

**Questions**:
- Does each segment get its own weigher instance?
- Are weighers required to be `Send + Sync`?
- How are weights updated under concurrent access?

**Fix**: Add dedicated section on concurrent caches.

**Action**: Expand spec with concurrent cache design.

---

## Recommended Alternative Architecture

### Option D (NEW): Zero-Cost Generic Weigher

```rust
// Cache with optional weight support via generic
pub struct LruCache<K, V, S = DefaultHashBuilder, W = NoWeigher> {
    segment: LruSegment<K, V, S, W>,
}

// Zero-sized marker (no runtime cost!)
pub struct NoWeigher;

// Active weigher with function
pub struct CustomWeigher<F> {
    func: F,
    max_weight: u64,
}

// Conditional weight storage
enum WeightData<W> {
    None(PhantomData<W>),  // 0 bytes when W = NoWeigher
    Some(u64),              // 8 bytes when weighing enabled
}

// Algorithm-specific metadata
struct LruMetadata<W> {
    node: *mut Entry<(K, V)>,     // 8 bytes
    weight: WeightData<W>,        // 0 or 8 bytes
}

struct GdsfMetadata<W> {
    node: *mut Entry<(K, V)>,     // 8 bytes
    weight: WeightData<W>,        // 0 or 8 bytes
    frequency: u64,               // 8 bytes
    age_at_insertion: u64,        // 8 bytes
    cached_priority: f64,         // 8 bytes
}
```

**Benefits**:
- ✅ **Zero-cost**: NoWeigher adds 0 bytes overhead
- ✅ **Backward compatible**: Add as default generic parameter
- ✅ **No heap allocation**: Weigher stored inline
- ✅ **Compiler optimization**: Monomorphization enables inlining
- ✅ **Type-safe**: Compile-time enforcement
- ✅ **Algorithm-specific**: Each cache pays only for its features

**Drawbacks**:
- Complex type signatures (mitigated by type aliases)
- Code bloat from monomorphization (acceptable for perf)

### Migration Path

```rust
// Phase 1 (v0.3.0): Add generics with defaults (backward compatible)
pub struct LruCache<K, V, S = DefaultHashBuilder, W = NoWeigher> { ... }

// Existing code still works:
let cache = LruCache::new(cap);  // Uses NoWeigher default

// New code can use weights:
let cache = LruCache::with_weigher(cap, |k, v| v.len() as u64);

// Phase 2 (v0.4.0): Add convenience type aliases
pub type UnweightedLruCache<K, V> = LruCache<K, V, DefaultHashBuilder, NoWeigher>;
pub type WeightedLruCache<K, V, F> = LruCache<K, V, DefaultHashBuilder, CustomWeigher<F>>;

// Phase 3 (v1.0.0): Deprecate old GDSF API
#[deprecated(since = "0.3.0", note = "Use `put_weighted` instead")]
pub fn put(&mut self, key: K, value: V, size: u64) { ... }
```

---

## Specific Changes Required

### Update Section 5: Options Considered

**Add Option D** (Zero-Cost Generic Weigher):
- Memory overhead: 0 bytes for LRU, 8 bytes for weight-aware
- Performance: No virtual dispatch, inline-able
- Backward compatible: Default generic parameter
- Verdict: ✅ **Recommended** (better than Option C)

**Update Decision Matrix**:

| Criterion | Option C (Spec) | Option D (New) |
|-----------|----------------|----------------|
| Code duplication | ✅ None | ✅ None |
| API complexity | ✅ Simple | ⚠️ Medium (generics) |
| Memory overhead | ❌ +40 bytes | ✅ +0/+8 bytes |
| Eviction speed | ✅ O(1) | ✅ O(1) |
| Performance | ⚠️ Dynamic dispatch | ✅ Monomorphized |
| Backward compatible | ⚠️ Partial | ✅ Full |
| no_std friendly | ⚠️ Requires Box | ✅ No allocation |

### Update Section 6: Recommended Approach

**Replace unified metadata** with algorithm-specific:

```rust
// LRU metadata (just pointer, optionally weight)
struct LruMetadata<W = NoWeigher> {
    node: *mut Entry<(K, V)>,
    weight: WeightData<W>,  // Conditional: 0 or 8 bytes
}

// LFU metadata (add frequency)
struct LfuMetadata<W = NoWeigher> {
    node: *mut Entry<(K, V)>,
    weight: WeightData<W>,
    frequency: u64,
}

// GDSF metadata (full set)
struct GdsfMetadata<W = NoWeigher> {
    node: *mut Entry<(K, V)>,
    weight: WeightData<W>,
    frequency: u64,
    age_at_insertion: u64,
    cached_priority: f64,
}
```

### Update Section 6.2: Weigher Trait

**Split into local and concurrent traits**:

```rust
// Base trait (no thread-safety requirement)
pub trait Weigher<K, V> {
    fn weight(&self, key: &K, value: &V) -> u64;
}

// Thread-safe variant for concurrent caches
pub trait ConcurrentWeigher<K, V>: Weigher<K, V> + Send + Sync + 'static {}

// Auto-implement for types that meet bounds
impl<K, V, T> ConcurrentWeigher<K, V> for T
where
    T: Weigher<K, V> + Send + Sync + 'static
{}
```

### Add Section 6.6: Concurrent Cache Integration

```markdown
## 6.6 Concurrent Cache Design

Concurrent caches (enabled with `concurrent` feature) wrap segment-based caches with `RwLock`:

```rust
pub struct ConcurrentLruCache<K, V, S = DefaultHashBuilder, W = NoWeigher> {
    segments: Vec<RwLock<LruSegment<K, V, S, W>>>,
    weigher: Arc<W>,  // Shared across segments
}
```

**Weigher Requirements**:
- Must implement `ConcurrentWeigher` (implies `Send + Sync + 'static`)
- Shared via `Arc` to avoid cloning large weighers
- Each segment uses same weigher instance

**Weight Tracking**:
- Each segment tracks its own `current_weight`
- Global weight = sum of all segment weights (computed on demand)
- Weight limits enforced per-segment (total / num_segments)
```

### Update Section 8: Migration Strategy

**Add explicit GDSF migration**:

```markdown
### 8.4 GDSF-Specific Migration

Current API:
```rust
cache.put("key", value, 100);  // Explicit size
```

New API (v0.3.0+):
```rust
// Option 1: Use weigher
let cache = GdsfCache::with_weigher(cap, |k, v| estimate_size(v));
cache.put("key", value);

// Option 2: Use old API (deprecated)
cache.put_with_size("key", value, 100);
```

**Timeline**:
- v0.3.0: Add `with_weigher()`, deprecate explicit size
- v0.4.0: Add deprecation warnings to `put(..., size)`
- v1.0.0: Remove `put(..., size)` API
```

### Update Section 9: Implementation Plan

**Add missing tasks**:
- T0: Performance baseline benchmarks
- T1.5: Finalize metadata design (unified vs specific)
- T14.5: Backward compatibility test suite
- T15: Write migration guide
- T16: Security audit of unsafe code
- T17: Miri testing
- T18: Property-based testing (QuickCheck/proptest)
- T19: Loom testing for concurrent caches

### Update Section 10: Open Questions

**Add new questions**:

| Question | Options | Recommendation |
|----------|---------|----------------|
| Generic vs Type-Erased Weigher? | (A) Generic (B) Type-erased (C) Both | **A** - Zero-cost |
| Unified vs Algorithm-Specific Metadata? | (A) Unified (B) Specific | **B** - Pay for what you use |
| Zero-weight behavior? | (A) Error (B) Pin (C) Configurable | **C** - Opt-in pinning |
| GDSF migration timeline? | (A) 1 version (B) 2-3 versions | **B** - Gradual migration |

---

## Performance Targets

Define explicit performance targets for the new design:

| Metric | Target | Measurement |
|--------|--------|-------------|
| Memory overhead (unweighted) | +0 bytes per entry | Must match current |
| Memory overhead (weighted) | +8 bytes per entry | For weight tracking only |
| `put()` performance (unweighted) | ±2% of current | Backward compat perf |
| `put()` performance (weighted) | ±10% of current | Acceptable for feature |
| `get()` performance | ±0% of current | No impact on reads |

**Acceptance Criteria**:
- All targets must be met in benchmarks
- Document where targets not met and why
- Create performance regression tests

---

## Testing Requirements

Expand testing beyond what's in spec:

### 1. Unit Tests
- [x] Entry-count mode (existing)
- [x] Weight mode (planned)
- [x] Mixed mode (planned)
- [ ] **Zero-weight entries** (new)
- [ ] **Weight update on value change** (new)
- [ ] **Large weight values (u64::MAX)** (new)
- [ ] **Overflow protection** (new)

### 2. Integration Tests
- [x] Cache-simulator with Moka (planned)
- [ ] **Real-world workload traces** (new)
- [ ] **Memory leak detection** (new)
- [ ] **Cross-algorithm consistency** (new)

### 3. Property-Based Tests (NEW)
```rust
#[quickcheck]
fn prop_weight_never_exceeds_max(ops: Vec<CacheOp>) -> bool {
    let mut cache = WeightedLruCache::new(1000);
    for op in ops {
        apply_op(&mut cache, op);
        assert!(cache.current_weight() <= cache.max_weight());
    }
    true
}
```

### 4. Concurrent Tests (NEW)
Use [Loom](https://github.com/tokio-rs/loom) for model checking:
```rust
#[test]
fn loom_concurrent_put_get() {
    loom::model(|| {
        let cache = Arc::new(ConcurrentLruCache::new(10));
        // Test concurrent access patterns
    });
}
```

### 5. Embedded/no_std Tests
- [ ] Test on actual ARM Cortex-M target
- [ ] Memory usage profiling on embedded
- [ ] Stack usage analysis

---

## Documentation Requirements

### 1. Migration Guide (NEW)
Create `docs/MIGRATION_v0.3.md`:
- Step-by-step migration for each cache type
- Code examples: before/after
- Common pitfalls and solutions
- Performance comparison

### 2. Design Rationale (NEW)
Create `docs/DESIGN_RATIONALE.md`:
- Why generic weigher over type-erasure
- Why algorithm-specific metadata
- Memory/performance tradeoffs explained
- When to use weighted vs unweighted

### 3. API Documentation Updates
- Add examples to every public method
- Document performance characteristics
- Add "Panics" sections
- Add "Safety" sections for unsafe

### 4. README Updates
- Update feature matrix with weight support
- Add "Choosing the Right Cache" section
- Update performance table with weighted variants
- Add examples of size-aware usage

---

## Review Checklist for Revised Spec

Before implementing, the revised spec must satisfy:

- [ ] Memory overhead accurately calculated for all algorithms
- [ ] Performance benchmarks comparing generic vs type-erased weigher
- [ ] Backward compatibility verified (no breaking changes in v0.3)
- [ ] GDSF migration path clearly defined
- [ ] Zero-weight behavior completely specified
- [ ] Concurrent cache integration fully designed
- [ ] All open questions answered with data
- [ ] Testing strategy includes property-based and model checking
- [ ] Migration guide drafted
- [ ] Performance targets defined and measurable
- [ ] Security implications analyzed (unsafe code review)
- [ ] no_std compatibility verified
- [ ] Miri testing plan defined
- [ ] Documentation plan complete

---

## Timeline Recommendation

**Current Spec Status**: Not ready for implementation

**Recommended Timeline**:

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| **Revision** | 1-2 weeks | Updated spec addressing all critical issues |
| **Review** | 1 week | Second review by maintainers + community |
| **Prototyping** | 2 weeks | Proof-of-concept for generic weigher approach |
| **Benchmarking** | 1 week | Performance comparison: baseline vs proposed |
| **Decision** | 1 week | Final design decision based on data |
| **Implementation** | 4-6 weeks | Full implementation with tests |
| **Documentation** | 1 week | Migration guide, examples, API docs |
| **Testing** | 2 weeks | Comprehensive testing, security audit |
| **Release** | 1 week | v0.3.0 release with new features |

**Total**: ~3-4 months from spec revision to release

---

## Next Steps

1. **Immediate** (This Week):
   - [ ] Distribute this review to cache-rs maintainers
   - [ ] Discuss critical issues in team meeting
   - [ ] Decide: revise spec or alternative approach?

2. **Short Term** (Next 2 Weeks):
   - [ ] Revise specification addressing critical issues
   - [ ] Create proof-of-concept prototype (generic weigher)
   - [ ] Run benchmarks: current vs proposed designs
   - [ ] Draft migration guide

3. **Medium Term** (Next 1-2 Months):
   - [ ] Second review cycle
   - [ ] Begin implementation of chosen design
   - [ ] Write comprehensive tests
   - [ ] Update all documentation

4. **Long Term** (Next 3-4 Months):
   - [ ] Complete implementation
   - [ ] Security audit
   - [ ] Beta testing period
   - [ ] Release v0.3.0

---

## Conclusion

The size-aware cache design is an **important and valuable feature** for cache-rs, but the current specification has critical flaws that must be addressed before implementation.

**Key Recommendation**: Adopt a **zero-cost abstraction approach** using generic weighers and algorithm-specific metadata. This aligns with Rust's philosophy, maintains cache-rs's performance goals, and provides maximum flexibility for users.

**Next Action**: Revise the specification based on this review and recirculate for approval before beginning implementation.

---

**Questions or Feedback**: Contact the architecture review team or open an issue on GitHub.
