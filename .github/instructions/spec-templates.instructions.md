---
applyTo: "docs/**/*.md"
---

# Specification Templates for Cache-RS

This file provides templates for creating design specifications in cache-rs. Each template is tailored to specific types of cache algorithm work.

## Template Selection Guide

| Work Type | Template to Use | When to Use |
|-----------|----------------|-------------|
| **New Cache Algorithm** | New Algorithm Template | Adding LRU, LFU, ARC, CLOCK, TinyLFU, etc. |
| **Algorithm Enhancement** | Enhancement Template | Adding TTL, compression, callbacks to existing algorithms |
| **Performance Optimization** | Optimization Template | Improving speed/memory without changing cache behavior |
| **Infrastructure Change** | Infrastructure Template | Modifying list.rs, metrics, concurrent system |

---

## New Cache Algorithm Template

Use this template when adding a completely new eviction algorithm (e.g., TinyLFU, ARC, CLOCK).

```markdown
# [Algorithm Name] Cache Implementation Specification

## Algorithm Overview

**Algorithm Name**: [Full name, e.g., "Adaptive Replacement Cache (ARC)"]
**Research Background**: [1-2 sentences about the algorithm's origin and research]
**Problem Solved**: [What cache workload pattern this algorithm optimizes for]

### Algorithm Theory

**Mathematical Formulation**:
[Provide the core algorithm formula, e.g.:]
- Priority = (Frequency / Size) + Global_Age  (for GDSF)
- Probability = frequency / (frequency + 1)   (for TinyLFU)

**Key Concepts**:
- [Concept 1]: [Definition and purpose]
- [Concept 2]: [Definition and purpose]  
- [Concept 3]: [Definition and purpose]

**Cache Behavior Characteristics**:
- **Temporal Locality**: [How algorithm handles recent vs old items]
- **Frequency Sensitivity**: [How algorithm tracks and uses access frequency]
- **Scan Resistance**: [Algorithm's resistance to large sequential scans]
- **Adaptation**: [How algorithm adapts to changing workload patterns]

## Performance Requirements

**Time Complexity** (MUST be maintained):
- `get(key)`: O(1) 
- `put(key, value)`: O(1)
- `remove(key)`: O(1)

**Space Complexity**:
- Memory per entry: [Target overhead, e.g., "~120 bytes"]
- Additional structures: [Any global data structures needed]

**Performance Targets**:
- Average latency: < 1μs per operation
- Throughput: [Expected ops/sec if known]
- Memory efficiency: [Compared to existing algorithms]

## Data Structure Design

### Core Data Structures

**Primary Storage**:
```rust
// Example structure - adapt to your algorithm
pub struct [Algorithm]Cache<K, V, S = DefaultHashBuilder> {
    // Key-to-metadata mapping (O(1) lookup)
    map: HashMap<K, NodePtr/Metadata, S>,
    
    // Algorithm-specific data structures
    [main_structure]: [Type], // e.g., List<Entry>, BTreeMap<Priority, List<Entry>>
    [aux_structure]: [Type],  // e.g., filter, counters, aging data
    
    // Configuration and metrics
    config: [Algorithm]CacheConfig,
    metrics: [Algorithm]CacheMetrics,
}
```

**Metadata Requirements**:
```rust
// Define what information each cache entry needs
pub struct [Algorithm]Meta {
    [field1]: [Type], // e.g., frequency: u64
    [field2]: [Type], // e.g., priority: f64
    [field3]: [Type], // e.g., timestamp: u64
}
```

### Memory Layout Considerations

**Node Structure**: [How cache entries are laid out in memory]
**Cache Locality**: [How data structure design improves CPU cache performance]
**Memory Overhead**: [Detailed breakdown of per-entry memory usage]

## Algorithm Implementation Strategy

### Eviction Logic

**Eviction Trigger**: [When eviction occurs - capacity exceeded, size limit, etc.]

**Eviction Target Selection**:
1. [Step 1]: [How algorithm identifies candidate for eviction]
2. [Step 2]: [Any prioritization or filtering steps]
3. [Step 3]: [Final selection and removal process]

**Example Eviction Sequence**:
[Provide a concrete example with 3-5 cache entries showing which gets evicted when cache is full]

### Core Operations

**get(key) Implementation**:
1. [Step 1]: [Lookup process]
2. [Step 2]: [Metadata updates]
3. [Step 3]: [Data structure maintenance]

**put(key, value) Implementation**:
1. [Step 1]: [Check for existing key]
2. [Step 2]: [Capacity management and eviction]
3. [Step 3]: [Insertion and metadata setup]

**remove(key) Implementation**:
1. [Step 1]: [Lookup and removal process]
2. [Step 2]: [Data structure cleanup]

### Dual-Capacity Integration

**Entry Count Limit**: [How algorithm respects capacity: NonZeroUsize limit]
**Size Limit**: [How algorithm respects max_size: u64 limit]  
**Eviction Priority**: [When both limits are approached, which takes priority]

## Unsafe Code Requirements

### Safety Invariants

[List all safety invariants that unsafe code must maintain:]

1. **[Invariant 1]**: [Description, e.g., "All pointers in HashMap point to valid List nodes"]
2. **[Invariant 2]**: [Description, e.g., "Doubly-linked list maintains forward/backward pointer consistency"]
3. **[Invariant 3]**: [Description]

### Required Safety Documentation

```rust
// Example of required safety documentation pattern
unsafe {
    // SAFETY: [Detailed explanation of why this unsafe operation is sound]
    // Invariants maintained: [List which invariants ensure safety]
    // Preconditions: [What conditions must be true before this code]
    let node = self.list.remove_node(node_ptr);
}
```

## API Contract Specification

### Universal API Methods (MUST implement exactly):

```rust
impl<K, V> [Algorithm]Cache<K, V> {
    // Constructor following cache-rs pattern
    pub fn init(config: [Algorithm]CacheConfig, hasher: Option<S>) -> Self;
    
    // Core cache operations
    pub fn get(&mut self, key: &K) -> Option<&V>;
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>; // Returns evicted
    pub fn remove(&mut self, key: &K) -> Option<V>;
    
    // Metadata operations  
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn clear(&mut self);
    
    // Size-aware operations (if algorithm supports variable sizes)
    pub fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>;
}
```

### Configuration Structure:

```rust
pub struct [Algorithm]CacheConfig {
    // Universal fields (REQUIRED)
    pub capacity: NonZeroUsize,      // Max number of entries
    pub max_size: u64,               // Max total size in bytes
    
    // Algorithm-specific fields
    pub [param1]: [Type],            // e.g., initial_age: f64
    pub [param2]: [Type],            // e.g., filter_size: usize
}
```

## Concurrent Implementation Strategy

### Concurrency Model

**Strategy**: [Choose: Segmented Mutex locking (default), Lock-free, or Hybrid]

**Implementation Pattern**:
```rust
// Segmented Mutex pattern (used by all concurrent caches in cache-rs)
// Mutex is used instead of RwLock because get() is a write operation
// (it updates recency/frequency metadata). Concurrency comes from segmentation.
pub struct Concurrent[Algorithm]Cache<K, V, S = DefaultHashBuilder> {
    segments: Box<[Mutex<[Algorithm]Segment<K, V, S>>]>,
    segment_mask: usize,
}

impl<K, V> Concurrent[Algorithm]Cache<K, V> {
    pub fn get(&self, key: &K) -> Option<V> {
        // Mutex lock on the relevant segment only
        self.get_segment(key).lock().get(key).cloned()
    }
    
    pub fn put(&self, key: K, value: V) -> Option<(K, V)> {
        // Mutex lock on the relevant segment only
        self.get_segment(&key).lock().put(key, value)
    }
}
```

**Thread Safety Considerations**:
- [How algorithm handles concurrent access to shared data structures]
- [Any algorithm-specific thread safety concerns]
- [Performance implications of chosen concurrency strategy]

## Testing Strategy

### Correctness Tests

**Eviction Policy Validation**:
```rust
// Example test structure for algorithm correctness
#[test]
fn test_[algorithm]_eviction_policy() {
    let mut cache = make_[algorithm](3); // Small cache for predictable behavior
    
    // Create specific access pattern
    cache.put("a", 1); 
    cache.put("b", 2);
    cache.put("c", 3);
    
    // Algorithm-specific accesses to establish eviction order
    [specific access pattern based on algorithm]
    
    // Trigger eviction and verify correct item was evicted
    cache.put("d", 4);
    assert!(cache.get(&"[expected_evicted_key]").is_none());
    assert!(cache.get(&"[expected_remaining_key]").is_some());
}
```

**Edge Cases to Test**:
- Empty cache operations
- Single-item cache behavior  
- Capacity limit boundary conditions
- Size limit boundary conditions
- Duplicate key handling
- Remove from empty cache

### Performance Tests

**Benchmark Structure**:
```rust
// Add to benches/criterion_benchmarks.rs
fn benchmark_[algorithm]_operations(c: &mut Criterion) {
    let mut cache = make_[algorithm](1000);
    
    // Warm up cache
    for i in 0..1000 { cache.put(i, i); }
    
    c.bench_function("[Algorithm] get hit", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.get(&(i % 1000)));
            }
        });
    });
    
    // Additional benchmarks: get miss, put existing, put new, remove
}
```

**Performance Validation**:
- [ ] Latency < 1μs average for get/put/remove
- [ ] No >10% regression compared to existing algorithms
- [ ] Memory overhead within reasonable bounds
- [ ] Concurrent performance scales with thread count

### Safety Tests

**Miri Validation**:
```bash
# MUST pass for any unsafe code
cargo +nightly miri test [algorithm]_tests
```

**Concurrent Stress Tests**:
```rust
// Add to tests/concurrent_stress_tests.rs  
#[test]  
fn stress_test_concurrent_[algorithm]() {
    // Multi-threaded stress test with random operations
    // Must not deadlock, race, or corrupt data
}
```

## Integration Requirements

### Module Integration

**Files to Create**:
- `src/[algorithm].rs` - Core algorithm implementation
- `src/config/[algorithm].rs` - Configuration structure
- `src/metrics/[algorithm].rs` - Algorithm-specific metrics
- `src/concurrent/[algorithm].rs` - Concurrent variant

**Files to Modify**:
- `src/lib.rs` - Add re-exports and documentation
- `src/meta.rs` - Add metadata type (if new type needed)
- `tests/correctness_tests.rs` - Add algorithm correctness tests
- `tests/concurrent_correctness_tests.rs` - Add concurrent tests
- `benches/criterion_benchmarks.rs` - Add performance benchmarks

### Documentation Requirements

**Algorithm Documentation** (add to `src/lib.rs`):
```rust
//! ### [Algorithm Name] ([Algorithm Acronym])
//!
//! [Brief description of algorithm and use case]
//!
//! ```rust
//! use cache_rs::[Algorithm]Cache;
//! use cache_rs::config::[Algorithm]CacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = [Algorithm]CacheConfig {
//!     capacity: NonZeroUsize::new(100).unwrap(),
//!     max_size: u64::MAX,
//!     // algorithm-specific fields
//! };
//! let mut cache = [Algorithm]Cache::init(config, None);
//!
//! // Demonstrate algorithm-specific behavior
//! cache.put("key1", "value1");
//! // [More example usage]
//! ```
```

## Success Criteria

### Implementation Complete When:

- [ ] **Algorithm Correctness**: Eviction policy matches specification exactly
- [ ] **Performance**: All operations maintain O(1) complexity and <1μs latency
- [ ] **API Consistency**: Universal API contract implemented identically to other algorithms
- [ ] **Safety**: All unsafe code has documented invariants and passes Miri
- [ ] **Concurrency**: Thread-safe variant works correctly under stress testing
- [ ] **Testing**: Comprehensive correctness, performance, and safety test coverage
- [ ] **Documentation**: Algorithm theory, usage examples, and performance characteristics documented
- [ ] **Integration**: Properly integrated into module system with appropriate re-exports

### Validation Checklist:

```bash
# All must pass:
cargo build --features "std,concurrent"                    # ✅ Builds cleanly
cargo test --features "std,concurrent"                     # ✅ All tests pass  
cargo clippy --features "std,concurrent" -- -D warnings    # ✅ No lint warnings
cargo +nightly miri test                                  # ✅ No undefined behavior
cargo bench --features "std,concurrent"                   # ✅ Performance targets met
cargo doc --no-deps --document-private-items              # ✅ Documentation builds
```

---

## Enhancement Template

Use this template when adding features that affect all existing algorithms.

```markdown
# [Feature Name] Enhancement Specification

## Enhancement Overview

**Feature Name**: [e.g., "Time-To-Live (TTL) Support"]
**Scope**: [All algorithms | Specific algorithms | Optional feature]
**Backward Compatibility**: [Breaking | Non-breaking | Configurable]

### Problem Statement

[Describe what problem this enhancement solves and why it's needed]

### Use Cases

1. **[Use Case 1]**: [Description and example]
2. **[Use Case 2]**: [Description and example]
3. **[Use Case 3]**: [Description and example]

## Design Decisions

### API Design

**New Methods** (if any):
```rust
impl<K, V> Cache<K, V> {
    // Example new methods
    pub fn put_with_ttl(&mut self, key: K, value: V, ttl: Duration) -> Option<(K, V)>;
    pub fn get_ttl(&self, key: &K) -> Option<Duration>;
}
```

**Configuration Changes**:
```rust
// Changes to all config structs
pub struct AlgorithmCacheConfig {
    // Existing fields...
    pub capacity: NonZeroUsize,
    pub max_size: u64,
    
    // New fields
    pub [new_field]: [Type],
}
```

### Implementation Strategy

**Affected Components**:
- [ ] Core algorithms: [List which algorithms need changes]
- [ ] Configuration structs: [Which config files]
- [ ] Metadata system: [Changes to CacheEntry or metadata types]
- [ ] Concurrent variants: [Thread-safety implications]
- [ ] Metrics: [New metrics to track]

**Algorithm-Specific Considerations**:
- **LRU**: [How enhancement affects LRU eviction policy]
- **LFU**: [How enhancement affects LFU eviction policy]  
- **LFUDA**: [How enhancement affects LFUDA eviction policy]
- **SLRU**: [How enhancement affects SLRU eviction policy]
- **GDSF**: [How enhancement affects GDSF eviction policy]

### Performance Impact

**Expected Overhead**:
- Memory per entry: [Additional bytes needed]
- CPU per operation: [Additional computation]
- Algorithmic complexity: [Any changes to O(1) guarantees]

**Optimization Strategies**:
[How to minimize performance impact]

## Implementation Plan

### Phase 1: Core Infrastructure
- [ ] Modify `CacheEntry` or metadata types
- [ ] Update configuration structures
- [ ] Add new metrics fields

### Phase 2: Algorithm Implementation
- [ ] Implement in LRU algorithm
- [ ] Implement in LFU algorithm  
- [ ] Implement in LFUDA algorithm
- [ ] Implement in SLRU algorithm
- [ ] Implement in GDSF algorithm

### Phase 3: Concurrent Support
- [ ] Update concurrent variants
- [ ] Test thread-safety implications
- [ ] Performance test concurrent scenarios

### Phase 4: Testing and Documentation
- [ ] Add correctness tests for all algorithms
- [ ] Add performance benchmarks
- [ ] Update documentation and examples

## Success Criteria

- [ ] **Consistency**: Feature works identically across all 5 algorithms
- [ ] **Performance**: No >5% regression in any benchmark  
- [ ] **Safety**: All unsafe code implications addressed
- [ ] **Testing**: Comprehensive test coverage for new functionality
- [ ] **Documentation**: Clear usage examples and performance implications
```

---

## Optimization Template

Use this template for performance improvements that don't change cache behavior.

```markdown
# [Optimization Name] Performance Optimization Specification

## Optimization Overview

**Target**: [What specific performance aspect: latency, throughput, memory usage]
**Scope**: [Which algorithms or components affected]
**Expected Improvement**: [Quantitative goals, e.g., "20% latency reduction"]

### Performance Problem Analysis

**Current Bottleneck**: [Specific performance issue identified]
**Profiling Evidence**: [Results from benchmarking or profiling tools]
**Root Cause**: [Technical explanation of why current approach is suboptimal]

### Optimization Strategy

**Approach**: [High-level description of optimization technique]
**Trade-offs**: [Any trade-offs in safety, maintainability, or other characteristics]

#### Technical Details

**Data Structure Changes**:
[Any modifications to HashMap, List, or other structures]

**Algorithm Modifications**:
[Changes to get/put/remove operation implementations]

**Memory Layout Optimizations**:
[Cache locality improvements, alignment, etc.]

**Unsafe Code Optimizations**:
[Any new unsafe code for performance gains]

## Implementation Plan

### Phase 1: Baseline Measurement
- [ ] Create comprehensive benchmark suite for affected operations
- [ ] Document current performance characteristics
- [ ] Identify specific hot paths through profiling

### Phase 2: Optimization Implementation  
- [ ] Implement optimized data structures
- [ ] Modify algorithm implementations
- [ ] Add necessary unsafe code with safety documentation

### Phase 3: Validation
- [ ] Verify performance improvements meet targets
- [ ] Ensure correctness is maintained (all tests pass)
- [ ] Validate unsafe code with Miri
- [ ] Test concurrent performance implications

## Safety Considerations

**New Unsafe Code** (if any):
[Document safety invariants and justification for unsafe optimizations]

**Existing Safety Guarantees**:
[Ensure optimization doesn't break existing safety properties]

## Success Criteria

- [ ] **Performance Target**: [Specific improvement achieved, e.g., "25% latency reduction"]
- [ ] **Correctness Maintained**: All existing tests continue to pass
- [ ] **Safety Preserved**: Miri validation passes, no new undefined behavior
- [ ] **Regression Testing**: No performance degradation in other operations
```

---

## Infrastructure Template

Use this template for changes to shared infrastructure (list.rs, metrics, concurrent system).

```markdown
# [Infrastructure Change] Specification  

## Change Overview

**Component**: [e.g., "Doubly-linked list implementation", "Metrics system", "Concurrent architecture"]
**Type**: [Enhancement | Refactor | Bug fix | Safety improvement]
**Impact**: [Which algorithms affected]

### Motivation

[Why this infrastructure change is needed]

### Current State Analysis

**Current Implementation**: [How the component currently works]
**Limitations**: [What problems exist with current approach]  
**Dependencies**: [What algorithms or components depend on this infrastructure]

## Design Changes

### API Changes

**Breaking Changes** (if any):
[List any changes that break existing code]

**New Functionality**:
[New methods, types, or capabilities added]

### Implementation Strategy

**Safety Implications**: 
[How change affects unsafe code safety guarantees]

**Performance Implications**:
[Expected performance impact on all algorithms]  

**Algorithm Compatibility**:
[How change integrates with all 5 existing algorithms]

## Migration Plan

### Compatibility Strategy
- [ ] Maintain backward compatibility
- [ ] Provide migration path for breaking changes
- [ ] Update all affected algorithms consistently

### Testing Strategy  
- [ ] Comprehensive testing of infrastructure component
- [ ] Integration testing with all algorithms
- [ ] Performance regression testing
- [ ] Safety validation with Miri

## Success Criteria

- [ ] **Functionality**: New infrastructure capabilities work as specified
- [ ] **Compatibility**: All existing algorithms continue to work correctly
- [ ] **Performance**: No regression in algorithm performance
- [ ] **Safety**: All safety guarantees maintained or improved
- [ ] **Documentation**: Infrastructure changes clearly documented
```

---

## Template Usage Guidelines

1. **Choose the Right Template**: Select based on the scope and type of work
2. **Customize for Your Needs**: Adapt sections based on specific requirements
3. **Be Specific**: Replace placeholder text with concrete technical details
4. **Focus on Cache Theory**: Ground designs in cache algorithm research and performance characteristics
5. **Document Safety**: Pay special attention to unsafe code requirements and invariants
6. **Consider All Algorithms**: Ensure consistency across the 5 existing algorithms when relevant

These templates provide structure while allowing flexibility for the specific needs of each cache algorithm implementation or enhancement.