# Copilot Instructions for Cache-RS

This document provides essential knowledge for AI agents working on cache-rs, a high-performance Rust caching library implementing multiple eviction algorithms (LRU, LFU, LFUDA, SLRU, GDSF).

## Architecture Overview

Cache-rs provides a high-performance Rust caching library implementing multiple eviction algorithms (LRU, LFU, LFUDA, SLRU, GDSF) with shared infrastructure.

### Core Design Pattern: **HashMap + Doubly-Linked Lists**

All cache algorithms follow the same architectural pattern:
```rust
struct Cache<K, V, S> {
    map: HashMap<K, EntryMetadata/NodePtr, S>,     // O(1) key lookup
    list/priority_lists: List<(K, V)>/BTreeMap<Priority, List<(K,V)>>, // Ordering
    config: CacheConfig,                            // Capacity, etc.
    metrics: CacheMetrics,                          // Performance tracking
}
```

**Key insight**: `HashMap` stores metadata/pointers, not values. Values live in `List` nodes for cache-friendly memory layout.

### Algorithm-Specific Architectures

- **LRU**: Single `List<(K,V)>` + `HashMap<K, *mut Entry<(K,V)>>`
- **LFU/LFUDA/GDSF**: `BTreeMap<Priority, List<(K,V)>>` for priority-based eviction
- **SLRU**: Two separate lists (probationary + protected segments)

## Critical Unsafe Code Patterns

All cache algorithms use **extensive unsafe code** for performance. Every `unsafe` block MUST have safety comments following this pattern:

```rust
// SAFETY: node comes from our map, so it's a valid pointer to an entry in our list
unsafe {
    let (key, value) = (*node).get_value();
    self.list.move_to_front(node);
}
```

**Common safety invariants**:
- Pointers in `HashMap` always point to valid `List` entries
- `Box::into_raw()` / `Box::from_raw()` pairs must be balanced
- Node manipulation preserves doubly-linked list integrity

## No-std and Feature Architecture

**Default configuration**: `no_std` + `hashbrown` feature enabled
- Uses `alloc` crate for heap allocation
- `hashbrown::HashMap` instead of `std::HashMap`
- Feature flags control std/hashbrown/nightly optimizations

**Critical**: Always use conditional imports:
```rust
#[cfg(feature = "hashbrown")]
use hashbrown::{HashMap, DefaultHashBuilder};

#[cfg(not(feature = "hashbrown"))]
use std::collections::{HashMap, hash_map::RandomState as DefaultHashBuilder};
```

## Metrics System Architecture

**BTreeMap-based metrics** (not HashMap) for deterministic ordering:
```rust
// All algorithms implement this trait
impl CacheMetrics for Cache {
    fn metrics(&self) -> BTreeMap<String, f64> { ... }
    fn algorithm_name(&self) -> &'static str { ... }
}
```

**Why BTreeMap**: Benchmarking and simulation require consistent metric ordering for reproducible comparisons.

## Development Workflow

### Required Validation Pipeline (ALL CHANGES MUST PASS):
```bash
cargo build --all-targets                              # Build check
cargo test --all                                       # All tests
cargo fmt --all -- --check                            # Formatting
cargo clippy --all-targets --all-features -- -D warnings  # Linting
cargo check --all-targets --all-features              # Additional checks
cargo doc --no-deps --document-private-items          # Documentation
cargo build --no-default-features --target thumbv6m-none-eabi  # no_std test
```

### Testing Patterns
- Comprehensive unit tests in each algorithm module
- Integration tests in `tests/no_std_tests.rs`
- Benchmarking with `criterion` in `benches/`
- Cross-platform CI on Linux/macOS/Windows with MSRV 1.74.0

## Module-Level Documentation Requirements

Every cache algorithm module MUST have comprehensive consumer-focused documentation:

```rust
//! # Algorithm Name (e.g., LRU, LFUDA)
//!
//! Brief description and what problem it solves.
//!
//! ## Algorithm Details
//! Mathematical formulation and key concepts.
//!
//! ## Performance Characteristics
//! - Time Complexity: Get/Put/Remove operations
//! - Space Complexity and memory overhead
//!
//! ## When to Use
//! - Ideal use cases
//! - When NOT to use (important!)
//!
//! ## Thread Safety
//! "Not thread-safe. Wrap with Mutex/RwLock for concurrent access."
```

## Priority-Based Cache Patterns (LFU/LFUDA/GDSF)

Complex algorithms use **priority lists** with these common patterns:

```rust
// Priority calculation and list management
unsafe fn update_priority(&mut self, key: &K) -> *mut Entry<(K, V)> {
    let metadata = self.map.get_mut(key).unwrap();
    let old_priority = calculate_old_priority(metadata);
    
    // Update frequency/priority
    metadata.frequency += 1; 
    let new_priority = calculate_new_priority(metadata);
    
    // Move between priority lists if needed
    if old_priority != new_priority {
        let node = self.priority_lists
            .get_mut(&old_priority).unwrap()
            .remove(metadata.node).unwrap();
        
        // Update data structures...
    }
}
```

## Code Quality Guidelines

1. **Linear over complex**: Prefer explicit if/else over complex iterator chains
2. **Document "why" not "what"**: Focus on algorithmic decisions and invariants  
3. **Safety-first unsafe**: Every unsafe block needs detailed safety reasoning
4. **Clone minimization**: Only clone when ownership semantics require it
5. **Early returns**: Reduce nesting with guard clauses
6. **No mod.rs pattern**: Use `module_name.rs` files instead of `module_name/mod.rs` directories. For example, use `src/lru.rs` not `src/lru/mod.rs`

## Key Files to Reference

---

## Additional Context for AI Agents

### Common Implementation Gotchas
- **Priority keys in BTreeMap**: LFUDA/GDSF multiply float priorities by 1000 to create integer keys for BTreeMap storage
- **Min priority tracking**: Complex algorithms maintain `min_priority` field for O(1) eviction target identification
- **Empty list cleanup**: Always remove empty priority lists from BTreeMap to prevent memory leaks
- **Node pointer updates**: When moving nodes between lists, update metadata.node pointers

### Simulation Integration Points
Cache algorithms are designed for performance analysis. Key patterns:
- `record_miss(object_size)` method for external miss tracking
- `estimate_object_size()` for metrics (simple heuristic, not exact)
- Metrics focus on hit rates, eviction patterns, and algorithm-specific behavior

### Performance Expectations
From benchmarks, relative performance hierarchy:
1. **LRU**: ~887ns (fastest, simplest)
2. **SLRU**: ~983ns (good scan resistance)  
3. **GDSF**: ~7.5µs (size-aware complexity)
4. **LFUDA**: ~20.5µs (aging calculations)
5. **LFU**: ~22.7µs (frequency-based without aging)

Focus performance work on hot paths: `get()`, `put()`, priority updates.

## Documentation Requirements

### Module-Level Documentation

Every module must have comprehensive documentation written from a **consumer's perspective**:

```rust
//! # Module Name
//!
//! Brief description of what this module provides to users.
//!
//! ## What It Does
//!
//! Detailed explanation of the module's purpose and functionality.
//! Explain the problem it solves and why someone would use it.
//!
//! ## How It Works
//!
//! High-level explanation of the algorithm or approach used.
//! Include time/space complexity where relevant.
//! Mention any important trade-offs or limitations.
//!
//! ## Usage Examples
//!
//! ```rust
//! // Provide clear, compilable examples showing typical usage
//! use cache::ModuleName;
//! 
//! let example = ModuleName::new();
//! // Show common operations
//! ```
//!
//! ## When to Use This Module
//!
//! - Specific use cases where this module excels
//! - Scenarios where other alternatives might be better
//! - Performance characteristics and suitability for different workloads
//!
//! ## Thread Safety
//!
//! Clearly state thread safety guarantees or lack thereof.
//! Provide guidance on concurrent usage patterns.
```

### Function-Level Documentation

- **Public functions**: Must have doc comments explaining purpose, parameters, return values, and examples
- **Complex private functions**: Should have comments explaining the approach and any non-obvious behavior
- **Unsafe functions**: Must have detailed safety requirements and invariants

### Code Comments

- Explain *why* something is done, not just *what* is done
- Add comments for non-obvious algorithmic choices
- Document invariants and assumptions
- Explain the reasoning behind performance optimizations

## Cache-Specific Guidelines

### Algorithm Implementations

1. **Consistency Across Cache Types**
   - All cache implementations should follow similar API patterns
   - Use consistent naming for similar operations across different cache types
   - Maintain similar error handling approaches

2. **Performance Characteristics**
   - Always document time and space complexity in module documentation
   - Include performance trade-offs in algorithm selection guidance
   - Provide benchmarking examples where appropriate

3. **Memory Management**
   - Be explicit about memory ownership in cache implementations
   - Document memory overhead per cache entry
   - Explain eviction behavior and its impact on memory usage

### Testing Requirements

- Write comprehensive unit tests for all public APIs
- Include edge cases (empty cache, single item, capacity limits)
- Test error conditions and boundary conditions
- Add integration tests for complex interactions between components

## Validation Requirements

**All changes must pass strict validation before being considered complete:**

### Required Checks

1. **Build**: `cargo build --all-targets`
   - Code must compile without errors
   - All features and targets must build successfully

2. **Test**: `cargo test --all`
   - All tests must pass
   - New functionality must include appropriate tests
   - Tests should cover both happy path and error cases

3. **Format**: `cargo fmt --all -- --check`
   - Code must be properly formatted using rustfmt
   - No formatting inconsistencies allowed

4. **Clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
   - No clippy warnings allowed
   - Use `#[allow(clippy::lint_name)]` sparingly and only with justification

5. **Check**: `cargo check --all-targets --all-features`
   - All code must pass additional semantic checks
   - Ensure no unused imports or dead code

6. **Documentation**: `cargo doc --no-deps --document-private-items`
   - Documentation must build without warnings
   - All public items must be documented

7. **Security**: `cargo audit` (if available)
   - No known security vulnerabilities in dependencies

### Additional Validation for Unsafe Code

When introducing or modifying `unsafe` code:

1. **Miri Testing**: Run `cargo +nightly miri test` to detect undefined behavior
2. **Safety Documentation**: Each unsafe block must have a detailed safety comment
3. **Code Review**: Unsafe code requires extra scrutiny and justification

## Error Handling

1. **Use Result Types**: Prefer `Result<T, E>` over panicking for recoverable errors
2. **Custom Error Types**: Define specific error types for different failure modes
3. **Error Context**: Provide meaningful error messages that help users understand what went wrong
4. **Fallible Operations**: Make fallible operations explicit in function signatures

## Dependencies

1. **Minimize Dependencies**: Only add dependencies that provide significant value
2. **Feature Gates**: Use feature flags for optional dependencies
3. **Version Compatibility**: Maintain compatibility with reasonable MSRV (Minimum Supported Rust Version)
4. **Security**: Regularly audit dependencies for security vulnerabilities

## Example Patterns to Follow

### Good: Linear, Clear Code
```rust
pub fn process_cache_entry(&mut self, key: &K, value: V) -> Result<Option<V>, CacheError> {
    // Check if key exists first
    if let Some(existing_node) = self.map.get(key) {
        return self.update_existing_entry(existing_node, value);
    }
    
    // Handle capacity limit
    if self.is_at_capacity() {
        self.evict_lru_entry()?;
    }
    
    // Insert new entry
    self.insert_new_entry(key.clone(), value)
}
```

### Avoid: Overly Complex Chaining
```rust
// Don't do this - too complex for maintainability
pub fn process_cache_entry(&mut self, key: &K, value: V) -> Result<Option<V>, CacheError> {
    self.map.get(key)
        .map(|node| self.update_existing_entry(node, value))
        .unwrap_or_else(|| {
            self.ensure_capacity()
                .and_then(|_| self.insert_new_entry(key.clone(), value))
        })
}
```

## Performance Considerations

1. **Benchmark Changes**: Use `cargo bench` to measure performance impact of changes
2. **Profile Memory Usage**: Be aware of memory allocation patterns
3. **Consider Cache Locality**: Structure data for good cache performance
4. **Avoid Premature Optimization**: Optimize only after measuring and identifying bottlenecks

## Commit and PR Guidelines

1. **Atomic Changes**: Each commit should represent a single logical change
2. **Descriptive Messages**: Commit messages should clearly explain what and why
3. **Breaking Changes**: Clearly document any breaking API changes
4. **Changelog**: Update CHANGELOG.md for significant changes

---

By following these guidelines, we ensure that the cache-rs codebase remains maintainable, performant, and accessible to both contributors and users. All code should be written with the principle that clarity and correctness are more important than clever solutions.
