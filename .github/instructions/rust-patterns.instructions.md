---
applyTo: "**/*.rs"
---

# Rust Patterns for Cache-RS

This file documents Rust-specific patterns, conventions, and best practices for cache-rs development. It focuses on the unique aspects of implementing high-performance cache algorithms in Rust.

## Core Rust Patterns for Cache Algorithms

### Universal API Contract Pattern

All cache algorithms MUST implement identical method signatures:

```rust
impl<K, V, S> AlgorithmCache<K, V, S> 
where
    K: Hash + Eq + Clone,
    V: Clone,
    S: BuildHasher,
{
    /// Constructor using the init pattern (not new)
    pub fn init(config: AlgorithmCacheConfig, hasher: Option<S>) -> Self {
        // Implementation...
    }

    /// Core cache operations with exact signatures
    pub fn get(&mut self, key: &K) -> Option<&V>;
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)>;
    pub fn remove(&mut self, key: &K) -> Option<V>;
    
    /// Size-aware operations (required for all algorithms)
    pub fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)>;
    
    /// Metadata operations
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn clear(&mut self);
}
```

### Configuration Pattern with Public Fields

**Always use public fields** (no builders or constructors):

```rust
/// Configuration for [Algorithm] cache
#[derive(Debug, Clone)]
pub struct AlgorithmCacheConfig {
    /// Maximum number of entries (REQUIRED)
    pub capacity: NonZeroUsize,
    
    /// Maximum total size in bytes (REQUIRED)
    pub max_size: u64,
    
    /// Algorithm-specific parameters with descriptive docs
    pub algorithm_param: AlgorithmSpecificType,
}

// Usage pattern:
let config = AlgorithmCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    max_size: 10 * 1024 * 1024,  // 10MB
    algorithm_param: default_value,
};
```

**Critical**: Always validate `NonZeroUsize` creation:
```rust
// GOOD: Validate capacity at construction
pub fn new_config(capacity: usize) -> Result<AlgorithmCacheConfig, &'static str> {
    let capacity = NonZeroUsize::new(capacity)
        .ok_or("Capacity must be greater than zero")?;
    Ok(AlgorithmCacheConfig {
        capacity,
        max_size: u64::MAX,
    })
}

// BAD: Will panic at runtime if capacity is 0
let config = AlgorithmCacheConfig {
    capacity: NonZeroUsize::new(user_input).unwrap(), // DANGEROUS!
    max_size: u64::MAX,
};
```

### Error Handling Philosophy

**Cache-rs uses a specific error handling strategy**:

#### Use `.unwrap()` for Programming Errors:
```rust
// Configuration validation (programmer error)
let capacity = NonZeroUsize::new(cap).unwrap(); // cap MUST be > 0

// Internal invariant violations (should never happen if code is correct)
let node = *self.map.get(&key_cloned).unwrap(); // Key MUST exist if invariants hold

// Test code (expected successes)
assert_eq!(cache.put("key", "value"), None);
let value = cache.get(&"key").unwrap(); // Expected to exist in test
```

#### Use `Option` for Normal Cache Operations:
```rust
// Cache operations that naturally might not succeed
pub fn get(&mut self, key: &K) -> Option<&V> {
    // Cache miss is expected and normal
}

pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
    // Returns evicted entry if capacity exceeded
}

pub fn remove(&mut self, key: &K) -> Option<V> {
    // Key might not exist, returns old value if found
}
```

#### NEVER Use `Result` Types:
```rust
// DON'T do this - cache-rs doesn't use Results
pub fn get(&mut self, key: &K) -> Result<&V, CacheError> { ... }  // WRONG

// DO this - use Option for cache operations
pub fn get(&mut self, key: &K) -> Option<&V> { ... }              // CORRECT
```

### Feature Flag Conditional Compilation

**Always use conditional imports for compatibility**:

```rust
// HashMap selection based on feature flags
#[cfg(feature = "hashbrown")]
use hashbrown::{HashMap, DefaultHashBuilder};

#[cfg(not(feature = "hashbrown"))]
use std::collections::{HashMap, hash_map::RandomState as DefaultHashBuilder};

// Parking lot synchronization for concurrent caches
#[cfg(feature = "concurrent")]
use parking_lot::Mutex;

// no_std compatibility  
#![no_std]
extern crate alloc;
use alloc::{boxed::Box, vec::Vec, collections::BTreeMap};

// Conditional std support
#[cfg(feature = "std")]
extern crate std;
```

**Feature flag patterns in Cargo.toml**:
```toml
[features]
default = ["hashbrown"]
nightly = ["hashbrown/nightly"]
std = []
concurrent = ["parking_lot"]

[dependencies]
hashbrown = { version = "0.16", optional = true }
parking_lot = { version = "0.12", optional = true }
```

### Metrics Pattern with BTreeMap

**Always use `BTreeMap<String, f64>` for deterministic ordering**:

```rust
use alloc::collections::BTreeMap;

pub trait CacheMetrics {
    /// Returns metrics in deterministic order (BTreeMap, not HashMap)
    fn metrics(&self) -> BTreeMap<String, f64>;
    
    /// Returns algorithm name for identification
    fn algorithm_name(&self) -> &'static str;
}

impl CacheMetrics for AlgorithmCache<K, V, S> {
    fn metrics(&self) -> BTreeMap<String, f64> {
        let mut metrics = BTreeMap::new();
        
        // Core metrics (consistent across all algorithms)
        metrics.insert("hits".to_string(), self.hits as f64);
        metrics.insert("misses".to_string(), self.misses as f64);
        metrics.insert("evictions".to_string(), self.evictions as f64);
        metrics.insert("entries".to_string(), self.len() as f64);
        metrics.insert("size_bytes".to_string(), self.total_size as f64);
        
        // Algorithm-specific metrics
        metrics.insert("algorithm_specific_metric".to_string(), self.specific_value as f64);
        
        metrics
    }
    
    fn algorithm_name(&self) -> &'static str {
        "Algorithm"  // e.g., "LRU", "LFU", "GDSF"
    }
}
```

**Why BTreeMap**: Benchmarking and simulation require consistent metric ordering for reproducible comparisons.

### Dual-Capacity System Pattern

**All algorithms must support both entry count and size limits**:

```rust
pub struct AlgorithmCache<K, V, S = DefaultHashBuilder> {
    map: HashMap<K, NodePtr, S>,
    data_structure: DataStructure<Entry<K, V>>,
    
    // Capacity tracking
    config: AlgorithmCacheConfig,
    current_size: u64,  // Total size of all entries
    
    // Metrics
    metrics: AlgorithmCacheMetrics,
}

impl<K, V, S> AlgorithmCache<K, V, S> {
    fn needs_eviction(&self, new_entry_size: u64) -> bool {
        // CRITICAL: OR condition, not AND
        (self.len() >= self.config.capacity.get()) || 
        (self.current_size + new_entry_size > self.config.max_size)
    }
    
    pub fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)> {
        // Handle existing key update
        if let Some(existing) = self.update_existing(&key, value, size) {
            return existing;
        }
        
        // Evict if either capacity limit would be exceeded
        let mut evicted = None;
        while self.needs_eviction(size) {
            evicted = self.evict_one();
        }
        
        // Insert new entry
        self.insert_new(key, value, size);
        evicted
    }
    
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        // Default size of 1 for backwards compatibility
        self.put_with_size(key, value, 1)
    }
}
```

### Generic Parameter Patterns

**Standard generic parameter ordering and defaults**:

```rust
// Standard pattern for cache algorithms
pub struct AlgorithmCache<K, V, S = DefaultHashBuilder> 
where
    K: Hash + Eq + Clone,     // Keys must be hashable, comparable, and cloneable
    V: Clone,                 // Values must be cloneable for cache operations
    S: BuildHasher,           // Hasher for HashMap customization
{
    map: HashMap<K, NodePtr, S>,
    // ... other fields
}

// For concurrent variants (segmented Mutex pattern)
pub struct ConcurrentAlgorithmCache<K, V, S = DefaultHashBuilder>
where
    K: Hash + Eq + Clone + Send + Sync,  // Additional Send + Sync for thread safety
    V: Clone + Send + Sync,
    S: BuildHasher + Clone + Send + Sync,
{
    segments: Box<[Mutex<AlgorithmSegment<K, V, S>>]>,
    segment_mask: usize,
}
```

**Constructor pattern with optional hasher**:
```rust
impl<K, V> AlgorithmCache<K, V, DefaultHashBuilder> {
    pub fn init(config: AlgorithmCacheConfig, hasher: Option<DefaultHashBuilder>) -> Self {
        Self::with_hasher(config, hasher.unwrap_or_default())
    }
}

impl<K, V, S> AlgorithmCache<K, V, S> 
where 
    S: BuildHasher,
{
    pub fn with_hasher(config: AlgorithmCacheConfig, hasher: S) -> Self {
        let capacity = config.capacity.get();
        Self {
            map: HashMap::with_capacity_and_hasher(capacity, hasher),
            // ... initialize other fields
        }
    }
}
```

## Unsafe Code Patterns

### Safety Documentation Pattern

**Every unsafe block MUST have detailed safety comments**:

```rust
unsafe {
    // SAFETY: This is safe because:
    // 1. `node_ptr` comes from our HashMap, so it points to a valid allocation
    // 2. The allocation is owned by our List, which hasn't been dropped
    // 3. We maintain the invariant that all pointers in HashMap are valid
    // 4. The List ensures nodes remain valid until explicitly removed
    let entry = &mut (*node_ptr);
    entry.update_metadata();
}
```

### Pointer Management Pattern

**Standard patterns for raw pointer management in cache algorithms**:

```rust
use alloc::boxed::Box;

// Converting to raw pointer for storage in HashMap
let boxed_entry = Box::new(Entry::new(key, value));
let node_ptr = Box::into_raw(boxed_entry);
self.map.insert(key.clone(), node_ptr);

// Converting back to Box for proper cleanup
let node_ptr = self.map.remove(&key).unwrap();
unsafe {
    // SAFETY: node_ptr came from Box::into_raw above and hasn't been freed
    let boxed_entry = Box::from_raw(node_ptr);
    // Box automatically drops when it goes out of scope
}

// Accessing data through raw pointer
unsafe {
    // SAFETY: node_ptr is valid as per HashMap invariant
    let entry = &(*node_ptr);
    let value = &entry.value;
}

// Mutating data through raw pointer  
unsafe {
    // SAFETY: node_ptr is valid and we have exclusive access to the cache
    let entry = &mut (*node_ptr);
    entry.frequency += 1;
}
```

### Doubly-Linked List Safety Pattern

**Standard pattern for safe doubly-linked list operations**:

```rust
pub struct ListNode<T> {
    pub data: T,
    pub prev: *mut ListNode<T>,
    pub next: *mut ListNode<T>,
}

impl<T> List<T> {
    pub unsafe fn link_nodes(&mut self, prev: *mut ListNode<T>, next: *mut ListNode<T>) {
        // SAFETY: Caller must ensure:
        // 1. prev and next are valid pointers or null
        // 2. prev and next are not currently linked to other nodes
        // 3. This operation maintains list invariants
        if !prev.is_null() {
            (*prev).next = next;
        }
        if !next.is_null() {
            (*next).prev = prev;
        }
    }
    
    pub unsafe fn unlink_node(&mut self, node: *mut ListNode<T>) -> *mut ListNode<T> {
        // SAFETY: Caller must ensure node is currently linked in this list
        let prev = (*node).prev;
        let next = (*node).next;
        
        // Unlink from neighbors
        self.link_nodes(prev, next);
        
        // Clear node's pointers (defensive programming)
        (*node).prev = core::ptr::null_mut();
        (*node).next = core::ptr::null_mut();
        
        node
    }
}
```

## Concurrent Programming Patterns

### Segmented Mutex Pattern

**Standard pattern for concurrent cache variants** (used by all concurrent caches in cache-rs):

Cache algorithms like LRU, LFU, LFUDA, GDSF, and SLRU require **mutable access even for
read operations**. Every `get()` call updates internal state (LRU moves to front, LFU
increments frequency, etc.). Therefore `Mutex` is used instead of `RwLock` — since `get()`
is inherently a write operation, `RwLock` provides no benefit.

Concurrency is achieved through **segmentation**: different keys can be accessed in parallel
as long as they hash to different segments.

```rust
use parking_lot::Mutex;

pub struct ConcurrentAlgorithmCache<K, V, S = DefaultHashBuilder> {
    segments: Box<[Mutex<AlgorithmSegment<K, V, S>>]>,
    segment_mask: usize,  // For fast modulo using bitwise AND
}

impl<K, V, S> ConcurrentAlgorithmCache<K, V, S> 
where
    K: Hash + Eq + Clone + Send + Sync,
    V: Clone + Send + Sync,
    S: BuildHasher + Clone + Send + Sync,
{
    pub fn new(config: AlgorithmCacheConfig, num_segments: usize) -> Self {
        // num_segments must be a power of 2 for efficient hashing
        assert!(num_segments.is_power_of_two());
        
        let per_segment_capacity = config.capacity.get() / num_segments;
        let per_segment_size = config.max_size / num_segments as u64;
        
        let segments = (0..num_segments)
            .map(|_| {
                let segment_config = AlgorithmCacheConfig {
                    capacity: NonZeroUsize::new(per_segment_capacity.max(1)).unwrap(),
                    max_size: per_segment_size.max(1),
                    ..config
                };
                Mutex::new(AlgorithmSegment::init(segment_config, None))
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
            
        Self {
            segments,
            segment_mask: num_segments - 1,
        }
    }
    
    fn get_segment(&self, key: &K) -> &Mutex<AlgorithmSegment<K, V, S>> {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish() as usize;
        &self.segments[hash & self.segment_mask]
    }
    
    pub fn get(&self, key: &K) -> Option<V> {
        self.get_segment(key).lock().get(key).cloned()
    }
    
    pub fn put(&self, key: K, value: V) -> Option<(K, V)> {
        self.get_segment(&key).lock().put(key, value)
    }
    
    pub fn remove(&self, key: &K) -> Option<V> {
        self.get_segment(key).lock().remove(key)
    }
    
    pub fn len(&self) -> usize {
        self.segments.iter().map(|s| s.lock().len()).sum()
    }
    
    pub fn is_empty(&self) -> bool {
        self.segments.iter().all(|s| s.lock().is_empty())
    }
}
```

## Algorithm-Specific Patterns

### Priority-Based Algorithm Pattern (LFU, LFUDA, GDSF)

**Common pattern for algorithms using priority queues**:

```rust
use alloc::collections::BTreeMap;

pub struct PriorityBasedCache<K, V, S = DefaultHashBuilder> {
    // Key to metadata mapping
    map: HashMap<K, EntryMetadata, S>,
    
    // Priority to list of entries mapping
    priority_lists: BTreeMap<Priority, List<Entry<K, V>>>,
    
    // Track minimum priority for O(1) eviction target
    min_priority: Priority,
    
    // Configuration and metrics
    config: PriorityCacheConfig,
    metrics: PriorityCacheMetrics,
}

impl<K, V, S> PriorityBasedCache<K, V, S> {
    fn calculate_priority(&self, metadata: &EntryMetadata) -> Priority {
        // Algorithm-specific priority calculation
        // For GDSF: (frequency / size) + age
        // For LFU: frequency
        // For LFUDA: frequency + age
    }
    
    fn update_priority(&mut self, key: &K, old_priority: Priority, new_priority: Priority) {
        if old_priority == new_priority {
            return; // No change needed
        }
        
        // Move entry from old priority list to new priority list
        let entry = self.priority_lists
            .get_mut(&old_priority)
            .unwrap()
            .remove_entry(key)
            .unwrap();
            
        // Clean up empty priority lists to prevent memory leaks
        if self.priority_lists.get(&old_priority).unwrap().is_empty() {
            self.priority_lists.remove(&old_priority);
        }
        
        // Add to new priority list
        self.priority_lists
            .entry(new_priority)
            .or_insert_with(List::new)
            .push_front(entry);
            
        // Update minimum priority tracking
        self.update_min_priority();
    }
    
    fn evict_min_priority(&mut self) -> Option<(K, V)> {
        // Find list with minimum priority
        let min_list = self.priority_lists.get_mut(&self.min_priority)?;
        
        // Remove least recently added entry from minimum priority
        let evicted = min_list.pop_back()?;
        
        // Clean up empty list
        if min_list.is_empty() {
            self.priority_lists.remove(&self.min_priority);
            self.update_min_priority();
        }
        
        // Remove from main map
        self.map.remove(&evicted.key);
        
        Some((evicted.key, evicted.value))
    }
}
```

### Simple List-Based Algorithm Pattern (LRU, SLRU)

**Common pattern for algorithms using simple lists**:

```rust
pub struct ListBasedCache<K, V, S = DefaultHashBuilder> {
    // Key to node pointer mapping
    map: HashMap<K, *mut ListNode<Entry<K, V>>, S>,
    
    // Doubly-linked list for ordering
    list: List<Entry<K, V>>,
    
    // Configuration and metrics
    config: ListCacheConfig,
    metrics: ListCacheMetrics,
}

impl<K, V, S> ListBasedCache<K, V, S> {
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let node_ptr = *self.map.get(key)?;
        
        unsafe {
            // SAFETY: node_ptr comes from our map, so it's valid
            let entry = &(*node_ptr).data;
            
            // Move to front (most recently used)
            self.list.move_to_front(node_ptr);
            
            Some(&entry.value)
        }
    }
    
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        // Check if key already exists
        if let Some(&node_ptr) = self.map.get(&key) {
            return self.update_existing(node_ptr, value);
        }
        
        // Evict if at capacity
        let mut evicted = None;
        if self.len() >= self.config.capacity.get() {
            evicted = self.evict_lru();
        }
        
        // Insert new entry
        let entry = Entry::new(key.clone(), value);
        let node_ptr = self.list.push_front(entry);
        self.map.insert(key, node_ptr);
        
        evicted
    }
}
```

## Testing Patterns

### Correctness Testing Pattern

**Standard pattern for testing cache eviction policies**:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroUsize;
    
    fn make_cache<K: Hash + Eq + Clone, V: Clone>(cap: usize) -> AlgorithmCache<K, V> {
        let config = AlgorithmCacheConfig {
            capacity: NonZeroUsize::new(cap).unwrap(),
            max_size: u64::MAX,
            // ... algorithm-specific fields
        };
        AlgorithmCache::init(config, None)
    }
    
    #[test]
    fn test_eviction_policy() {
        let mut cache = make_cache(3); // Small cache for predictable behavior
        
        // Fill cache to capacity
        cache.put("a", 1);
        cache.put("b", 2);  
        cache.put("c", 3);
        
        // Create access pattern specific to algorithm
        // For LRU: access "a" and "b" to make "c" least recently used
        cache.get(&"a");
        cache.get(&"b");
        
        // Trigger eviction - "c" should be evicted for LRU
        let evicted = cache.put("d", 4);
        assert_eq!(evicted, Some(("c".to_string(), 3)));
        
        // Verify cache state
        assert!(cache.get(&"a").is_some());
        assert!(cache.get(&"b").is_some());
        assert!(cache.get(&"c").is_none());
        assert!(cache.get(&"d").is_some());
    }
    
    #[test]
    fn test_dual_capacity_limits() {
        let config = AlgorithmCacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            max_size: 10, // Very small size limit
        };
        let mut cache = AlgorithmCache::init(config, None);
        
        // Test that size limit triggers eviction before entry count limit
        cache.put_with_size("small1", 1, 3);
        cache.put_with_size("small2", 2, 3);
        cache.put_with_size("small3", 3, 3);
        assert_eq!(cache.len(), 3); // Just under size limit (9 bytes)
        
        // This should trigger eviction due to size limit (9 + 5 > 10)
        let evicted = cache.put_with_size("large", 4, 5);
        assert!(evicted.is_some());
        assert_eq!(cache.len(), 3); // One evicted, one added
    }
    
    #[test]  
    fn test_empty_cache_operations() {
        let mut cache = make_cache(10);
        
        assert!(cache.get(&"nonexistent").is_none());
        assert!(cache.remove(&"nonexistent").is_none());
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }
}
```

### Performance Testing Pattern

**Standard pattern for benchmarking cache algorithms**:

```rust
// In benches/criterion_benchmarks.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_algorithm_operations(c: &mut Criterion) {
    const CACHE_SIZE: usize = 1000;
    
    let mut cache = make_algorithm_cache(CACHE_SIZE);
    
    // Warm up cache with data
    for i in 0..CACHE_SIZE {
        cache.put(i, i);
    }
    
    let mut group = c.benchmark_group("Algorithm Operations");
    
    // Test hit performance
    group.bench_function("get hit", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.get(&(i % CACHE_SIZE)));
            }
        });
    });
    
    // Test miss performance  
    group.bench_function("get miss", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.get(&(i + CACHE_SIZE)));
            }
        });
    });
    
    // Test put performance (existing keys)
    group.bench_function("put existing", |b| {
        b.iter(|| {
            for i in 0..100 {
                black_box(cache.put(i % CACHE_SIZE, i));
            }
        });
    });
    
    // Test put performance (new keys, triggers eviction)
    group.bench_function("put new (eviction)", |b| {
        let mut counter = CACHE_SIZE;
        b.iter(|| {
            for _ in 0..10 {
                counter += 1;
                black_box(cache.put(counter, counter));
            }
        });
    });
    
    group.finish();
}

criterion_group!(benches, benchmark_algorithm_operations);
criterion_main!(benches);
```

### Safety Testing with Miri

**Pattern for testing unsafe code with Miri**:

```rust
// Run with: cargo +nightly miri test
#[cfg(test)]
mod miri_tests {
    use super::*;
    
    #[test]
    fn test_pointer_safety() {
        let mut cache = make_cache(100);
        
        // Fill cache with data
        for i in 0..150 { // More than capacity to trigger evictions
            cache.put(i, i);
        }
        
        // Perform many operations that manipulate pointers
        for i in 0..1000 {
            cache.put(i, i);           // May trigger eviction and pointer cleanup
            cache.get(&(i / 2));       // May trigger pointer dereference
            cache.remove(&(i / 4));    // May trigger pointer cleanup
        }
        
        // Clear entire cache (tests cleanup of all pointers)
        cache.clear();
        assert_eq!(cache.len(), 0);
    }
    
    #[test]
    fn test_concurrent_safety() {
        use std::sync::Arc;
        use std::thread;
        
        let cache = Arc::new(ConcurrentAlgorithmCache::new(
            make_config(1000)
        ));
        
        let handles: Vec<_> = (0..8).map(|thread_id| {
            let cache = Arc::clone(&cache);
            thread::spawn(move || {
                for i in 0..1000 {
                    let key = thread_id * 1000 + i;
                    cache.put(key, key);
                    cache.get(&key);
                    if i % 10 == 0 {
                        cache.remove(&key);
                    }
                }
            })
        }).collect();
        
        for handle in handles {
            handle.join().unwrap();
        }
    }
}
```

## Documentation Patterns

### Algorithm Module Documentation

**Standard pattern for documenting cache algorithm modules**:

```rust
//! # [Algorithm Name] Cache Implementation
//!
//! [Brief description of what problem this algorithm solves]
//!
//! ## Algorithm Details
//!
//! [Mathematical formulation and key concepts]
//! 
//! ### When to Use
//!
//! - **Ideal for**: [Specific workload patterns]
//! - **Avoid when**: [Workload patterns where algorithm performs poorly]
//! - **Performance**: [Time/space complexity and typical latency]
//!
//! ## Usage Example
//!
//! ```rust
//! use cache_rs::[Algorithm]Cache;
//! use cache_rs::config::[Algorithm]CacheConfig;
//! use core::num::NonZeroUsize;
//!
//! let config = [Algorithm]CacheConfig {
//!     capacity: NonZeroUsize::new(1000).unwrap(),
//!     max_size: u64::MAX,
//!     // algorithm-specific fields...
//! };
//! 
//! let mut cache = [Algorithm]Cache::init(config, None);
//! cache.put("key", "value");
//! 
//! if let Some(value) = cache.get(&"key") {
//!     println!("Found: {}", value);
//! }
//! ```
//!
//! ## Performance Characteristics
//!
//! | Operation | Time Complexity | Notes |
//! |-----------|----------------|-------|
//! | get()     | O(1)          | [Algorithm-specific notes] |
//! | put()     | O(1)          | [Algorithm-specific notes] |
//! | remove()  | O(1)          | [Algorithm-specific notes] |
//!
//! **Memory overhead**: ~[X] bytes per entry
//!
//! ## Thread Safety
//!
//! This cache is **not thread-safe**. For concurrent access, use:
//! - `Concurrent[Algorithm]Cache` (requires `concurrent` feature)
//! - Manual synchronization with `Mutex` or `RwLock`
//!
//! ## Algorithm Research
//!
//! Based on: [Citation to research paper or reference implementation]

// Module implementation follows...
```

This comprehensive guide covers the essential Rust patterns for cache-rs development. Following these patterns ensures consistency, safety, and performance across all cache algorithm implementations.