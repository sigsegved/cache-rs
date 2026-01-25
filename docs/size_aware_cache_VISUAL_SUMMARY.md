# Size-Aware Cache Design Review - Visual Summary

This document provides a visual overview of the architecture review findings.

---

## Design Comparison

### Current Spec Proposal (Option C) ❌

```
┌─────────────────────────────────────────────────────────┐
│ LruCache<K, V, S>                                       │
├─────────────────────────────────────────────────────────┤
│ weigher: Box<dyn Weigher<K, V>>  ← Heap allocation!    │
│                                     Dynamic dispatch!   │
├─────────────────────────────────────────────────────────┤
│ HashMap<K, UnifiedMetadata>                             │
│   where UnifiedMetadata = 48 bytes per entry:          │
│     - node: 8 bytes                                     │
│     - weight: 8 bytes                                   │
│     - frequency: 8 bytes       ← LRU doesn't need!      │
│     - age_at_insertion: 8 bytes ← LRU doesn't need!     │
│     - cached_priority: 8 bytes  ← LRU doesn't need!     │
│     - location: 8 bytes         ← LRU doesn't need!     │
└─────────────────────────────────────────────────────────┘

Memory Impact: 1M entries = 48 MB (6x current!)
Performance: Virtual dispatch on every put()
```

### Recommended Approach (Option D) ✅

```
┌─────────────────────────────────────────────────────────┐
│ LruCache<K, V, S = DefaultHashBuilder, W = NoWeigher>  │
├─────────────────────────────────────────────────────────┤
│ weigher: W                      ← Zero-sized when       │
│                                   W = NoWeigher!        │
│                                   Inline when custom!   │
├─────────────────────────────────────────────────────────┤
│ HashMap<K, LruMetadata<W>>                              │
│   where LruMetadata<W> = 8 or 16 bytes:                │
│     - node: 8 bytes                                     │
│     - weight: WeightData<W>                             │
│         → 0 bytes when W = NoWeigher                    │
│         → 8 bytes when weighing enabled                 │
└─────────────────────────────────────────────────────────┘

Memory Impact: 1M entries = 8 MB (same as current!)
                            or 16 MB (2x when weighing)
Performance: Monomorphized, inline-able
```

---

## Memory Overhead Comparison

### Per-Entry Overhead by Algorithm

```
Algorithm  │ Current │ Spec (Option C) │ Recommended (Option D)
           │         │                 │ Unweighted | Weighted
───────────┼─────────┼─────────────────┼────────────┼──────────
LRU        │  8 bytes│     48 bytes    │   8 bytes  │ 16 bytes
LFU        │ 16 bytes│     48 bytes    │  16 bytes  │ 24 bytes
LFUDA      │ 24 bytes│     48 bytes    │  24 bytes  │ 32 bytes
SLRU       │ 16 bytes│     48 bytes    │  16 bytes  │ 24 bytes
GDSF       │ 40 bytes│     48 bytes    │  40 bytes  │ 48 bytes
```

**Overhead Analysis**:
- Option C: **Uniform 48 bytes** (wastes memory for simple algorithms)
- Option D: **Pay for what you use** (0-8 bytes for weight when enabled)

### Cache Size Impact (1 Million Entries)

```
                Current    Option C      Option D
                           (Spec)     (Unweighted | Weighted)
LRU             8 MB       48 MB        8 MB    | 16 MB
LFU            16 MB       48 MB       16 MB    | 24 MB
LFUDA          24 MB       48 MB       24 MB    | 32 MB
SLRU           16 MB       48 MB       16 MB    | 24 MB
GDSF           40 MB       48 MB       40 MB    | 48 MB

Overhead:       0 MB     +8-40 MB       0 MB    | +8 MB
Multiplier:      1x        2-6x          1x     | 1.5-2x
```

---

## Performance Comparison

### Weigher Function Call Overhead

```
┌──────────────────────────────────────────────────────────┐
│ Option C: Box<dyn Weigher>                               │
└──────────────────────────────────────────────────────────┘

cache.put(key, value)
  └─> weigher.weight(&key, &value)
        └─> (*vtable.weight_fn)(self, key, value)  ← Virtual dispatch
              └─> user_weigher_impl(key, value)

Cost: ~3-5ns per call (vtable lookup + indirect jump)
      + Cache miss if vtable not in L1/L2


┌──────────────────────────────────────────────────────────┐
│ Option D: Generic W                                      │
└──────────────────────────────────────────────────────────┘

cache.put(key, value)
  └─> weigher.weight(&key, &value)
        └─> user_weigher_impl(key, value)  ← Direct call, inline-able!

Cost: 0ns (inlined by compiler)
      or ~1ns for non-inlined direct call
```

**Performance Impact**:
- Option C: +5-10ns per `put()` (0.5-1% overhead)
- Option D: ~0ns (compiler optimizes away)

---

## API Evolution Diagram

### Migration Path

```
v0.2.x (Current)
├─ LruCache::new(capacity)
├─ GdsfCache::new(capacity).put(k, v, size)  ← Explicit size
└─ No weight support in other algorithms

                    ↓ Add generics with defaults

v0.3.0 (Backward Compatible)
├─ LruCache::new(capacity)                   ← Still works!
├─ LruCache<K, V, S, W = NoWeigher>          ← Generic added
├─ LruCache::with_weigher(cap, weigher)      ← New API
├─ GdsfCache::new(cap).put(k, v, size)       ← Still works (deprecated)
└─ GdsfCache::with_weigher(cap, w).put(k,v)  ← New API

                    ↓ Add convenience aliases

v0.4.0
├─ UnweightedLruCache<K, V>                  ← Type alias
├─ WeightedLruCache<K, V, W>                 ← Type alias
└─ GdsfCache.put(k, v, size) marked deprecated

                    ↓ Remove old APIs

v1.0.0
├─ LruCache<K, V, S = DefaultHashBuilder, W = NoWeigher>
└─ GdsfCache explicit size API removed
```

---

## Critical Issues Visualization

### Issue 1: Memory Overhead Explosion

```
Current LRU (1M entries):
████ 8 MB

Spec Option C (1M entries):
████████████████████████████████████████████████ 48 MB
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
           6x memory increase!

Recommended Option D (1M entries):
Unweighted: ████ 8 MB  ← Same as current!
Weighted:   ████████ 16 MB  ← 2x (acceptable for feature)
```

### Issue 2: Type Erasure Cost

```
Option C: Box<dyn Weigher>
┌─────────────────────────────────────────┐
│ Heap Allocation                         │
│ ┌─────────────────────────────────────┐ │
│ │ VTable Pointer                      │ │
│ │ ┌─────────────────────────────────┐ │ │
│ │ │ weight_fn: 0x12345678           │ │ │
│ │ │ drop_fn: 0x87654321             │ │ │
│ │ └─────────────────────────────────┘ │ │
│ │ User Data                           │ │
│ └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
       ↓ Every call goes through vtable
    3-5ns overhead + cache miss risk

Option D: Generic W
┌─────────────────────────────────────────┐
│ weigher: UserWeigher { ... }            │  ← Stored inline
│ Direct call to user_weigher.weight()    │  ← Monomorphized
└─────────────────────────────────────────┘
       ↓ Compiler can inline
    ~0ns overhead (optimized away)
```

### Issue 3: GDSF Breaking Change

```
Current API (v0.2):
┌────────────────────────────────────────────┐
│ cache.put("key", value, 100);              │  ← Explicit size
└────────────────────────────────────────────┘
               User code depends on this ↑

Proposed (Spec):
┌────────────────────────────────────────────┐
│ cache.put("key", value);                   │  ← Size removed
└────────────────────────────────────────────┘
    ↑ BREAKING CHANGE! All GDSF users break

Recommended:
┌────────────────────────────────────────────┐
│ // Old API (deprecated but works)          │
│ cache.put("key", value, 100);              │
│                                            │
│ // New API (weigher-based)                 │
│ cache.put_weighted("key", value);          │
└────────────────────────────────────────────┘
    ↑ Both APIs coexist during migration
```

---

## Decision Matrix Summary

```
Criterion                Option C (Spec)    Option D (Recommended)
─────────────────────────────────────────────────────────────────────
Code Duplication                ✅                   ✅
API Simplicity                  ✅                   ⚠️  (generics)
Memory (Unweighted)             ❌ (+40B)            ✅ (+0B)
Memory (Weighted)               ❌ (+40B)            ✅ (+8B)
Performance                     ⚠️  (dispatch)       ✅ (inline)
Backward Compat                 ⚠️  (partial)        ✅ (full)
no_std Friendly                 ⚠️  (Box)            ✅ (no alloc)
Type Safety                     ⚠️  (runtime)        ✅ (compile)
Industry Alignment              ✅ (Moka)            ✅ (quick_cache)
Implementation Complexity       ✅ (low)             ⚠️  (medium)
─────────────────────────────────────────────────────────────────────
SCORE                           5/10                 9/10
VERDICT                         ❌ Not Ready         ✅ Recommended
```

---

## Risk Assessment

### Option C Risks

| Risk | Severity | Probability | Impact |
|------|----------|-------------|--------|
| 6x memory increase unacceptable | CRITICAL | High | Adoption failure |
| Performance regression | HIGH | Medium | User complaints |
| GDSF users break on upgrade | HIGH | High | Churn |
| no_std users can't upgrade | MEDIUM | Medium | Fragmentation |
| Box allocation overhead | LOW | Low | Minor perf hit |

### Option D Risks

| Risk | Severity | Probability | Impact |
|------|----------|-------------|--------|
| Complex type signatures | MEDIUM | High | Learning curve |
| Code bloat from monomorphization | LOW | Medium | Binary size +5-10% |
| Generic parameter intimidating | LOW | Medium | Documentation needed |

---

## Testing Strategy Visualization

```
┌─────────────────────────────────────────────────────────────┐
│                    Testing Pyramid                          │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│                        Manual                               │
│                     ▲──────────▲                            │
│                    /            \                           │
│               Integration    Benchmarks                     │
│              ▲────────────────────▲                         │
│             /                      \                        │
│        Property-Based           Model Checking              │
│       ▲────────────────────────────────▲                    │
│      /                                  \                   │
│                 Unit Tests                                  │
│ ▲──────────────────────────────────────────▲               │
│                                                             │
└─────────────────────────────────────────────────────────────┘

Current Coverage:  ▓▓▓▓▓░░░░░ 50% (unit tests only)
Target Coverage:   ▓▓▓▓▓▓▓▓▓░ 90% (all layers)

Missing:
- [ ] Property-based tests (QuickCheck/proptest)
- [ ] Concurrent model checking (Loom)
- [ ] Memory leak detection (Valgrind)
- [ ] Stress tests (long-running)
- [ ] Embedded target testing
```

---

## Recommended Implementation Phases

```
Phase 1: Foundation (Week 1-2)
┌────────────────────────────────────┐
│ ☐ Revise spec with Option D       │
│ ☐ Prototype generic weigher       │
│ ☐ Benchmark vs current            │
└────────────────────────────────────┘

Phase 2: Core Implementation (Week 3-6)
┌────────────────────────────────────┐
│ ☐ Implement LRU with generics     │
│ ☐ Add algorithm-specific metadata │
│ ☐ Update LFU, LFUDA, SLRU         │
│ ☐ Refactor GDSF with migration    │
└────────────────────────────────────┘

Phase 3: Testing & Polish (Week 7-9)
┌────────────────────────────────────┐
│ ☐ Property-based tests             │
│ ☐ Concurrent model checking        │
│ ☐ Performance regression tests     │
│ ☐ Documentation                    │
└────────────────────────────────────┘

Phase 4: Release (Week 10-12)
┌────────────────────────────────────┐
│ ☐ Security audit                   │
│ ☐ Beta testing                     │
│ ☐ Migration guide                  │
│ ☐ Release v0.3.0                   │
└────────────────────────────────────┘

Timeline: ~3 months from revision to release
```

---

## Key Takeaways

### For Maintainers

1. **DO NOT PROCEED** with Option C implementation
2. **Revise spec** to use generic weigher (Option D)
3. **Calculate memory correctly** (48 bytes, not 8)
4. **Plan GDSF migration** to avoid breaking users
5. **Design concurrent integration** before implementation

### For Implementation

1. Use **algorithm-specific metadata** (not unified)
2. Use **generic weigher parameter** (not Box<dyn>)
3. Make weight **optional** with zero-sized `NoWeigher`
4. Support **both GDSF APIs** during migration
5. Add **extensive testing** (property-based, model checking)

### For Users (Future)

1. **Backward compatible**: Old code still works
2. **Zero overhead**: Unweighted caches same as before
3. **Simple upgrade**: Add `.with_weigher()` when needed
4. **Type aliases**: Use `WeightedLruCache` for simplicity
5. **Migration guide**: Step-by-step instructions provided

---

## Conclusion

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   Current Spec Status:  ❌ NOT READY FOR IMPLEMENTATION     │
│                                                             │
│   Recommended Action:   ⚠️  REVISE USING OPTION D          │
│                                                             │
│   Timeline:            📅 3-4 months to release             │
│                                                             │
│   Next Step:           🔄 Maintainer review meeting         │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

**This is a blocking review. Do not proceed without addressing critical issues.**

---

For full details, see:
- **Full Review**: `size_aware_cache_REVIEW.md`
- **Recommendations**: `size_aware_cache_RECOMMENDATIONS.md`
- **Executive Summary**: `size_aware_cache_EXECUTIVE_SUMMARY.md`
