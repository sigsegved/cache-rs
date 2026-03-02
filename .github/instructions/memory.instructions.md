# Memory Safety and Critical Gotchas for Cache-RS

This file documents critical memory safety patterns, common pitfalls, and gotchas that new developers MUST understand when working on cache-rs. These patterns are essential for maintaining the safety and correctness of high-performance cache algorithms with extensive unsafe code.

## Memory: CRITICAL - Understanding These Patterns is ESSENTIAL

### 1. Unsafe List Operations - MOST DANGEROUS

The `src/list.rs` module contains extensive unsafe raw pointer operations that are the foundation of all cache algorithms. **Understanding these patterns is absolutely critical.**

#### The Danger: Node Pointer Invalidation

```rust
// EXTREMELY DANGEROUS - DO NOT DO THIS:
let node_ptr = self.map.get(&key).unwrap();
unsafe {
    self.list.remove_first(); // ⚠️ node_ptr is now INVALID!
    let value = &(*node_ptr).value; // ⚠️ USE AFTER FREE - UNDEFINED BEHAVIOR!
}
```

**Why this happens**: List operations like `remove_first()`, `remove_last()`, and `remove_node()` deallocate memory and invalidate pointers.

#### Safe Pattern for List Operations:

```rust
// SAFE: Get data BEFORE any list operations
let node_ptr = *self.map.get(&key).unwrap();
unsafe {
    // SAFETY: node_ptr is valid as it comes from our HashMap and list hasn't been modified
    let key_value = (*node_ptr).data.clone(); // Get data FIRST
    
    // Now safe to perform list operations
    self.list.remove_node(node_ptr);
    
    // node_ptr is now invalid - DO NOT USE
}
```

#### List Operation Safety Rules:

1. **NEVER use a node pointer after ANY list operation**
2. **Extract all needed data BEFORE calling list methods**
3. **Each list operation may invalidate multiple pointers**
4. **Always clone or copy data before list modifications**

### 2. Configuration Panics - RUNTIME CRASHES

`NonZeroUsize` validation is a common source of runtime panics that are easy to miss in testing.

#### The Danger: Zero Capacity Panic

```rust
// WILL PANIC at runtime if user passes 0:
let config = LruCacheConfig {
    capacity: NonZeroUsize::new(user_input).unwrap(), // ⚠️ PANIC if user_input == 0
    max_size: u64::MAX,
};
```

**Why this is dangerous**: `NonZeroUsize::new(0)` returns `None`, and `.unwrap()` panics. This can crash in production if user input isn't validated.

#### Safe Pattern for Configuration:

```rust
// SAFE: Validate input before creating config
pub fn new_cache_config(capacity: usize) -> Result<LruCacheConfig, &'static str> {
    let capacity = NonZeroUsize::new(capacity)
        .ok_or("Capacity must be greater than zero")?;
        
    Ok(LruCacheConfig {
        capacity,
        max_size: u64::MAX,
    })
}

// Or in tests (where panic is acceptable):
let capacity = NonZeroUsize::new(100).unwrap(); // Safe because 100 > 0
```

#### Configuration Safety Rules:

1. **ALWAYS validate user input before NonZeroUsize::new()**
2. **Use Result types for user-facing configuration APIs**
3. **Only use .unwrap() with hardcoded non-zero values**
4. **Test with zero values to catch configuration bugs**

### 3. Dual-Limit Eviction Logic - ALGORITHM CORRECTNESS

The dual-capacity system (entry count + size limits) has subtle logic that's easy to get wrong.

#### The Trap: AND vs OR Logic

```rust
// WRONG: AND condition (both limits must be exceeded)
if self.len() >= capacity && self.total_size > max_size {
    self.evict(); // ⚠️ BUG: Won't evict when only one limit exceeded
}

// CORRECT: OR condition (either limit exceeded triggers eviction)
if self.len() >= capacity || self.total_size + new_size > max_size {
    self.evict(); // ✅ Evicts when either limit would be exceeded
}
```

**Why this matters**: A size-limited cache should evict when approaching the size limit, even if it hasn't hit the entry count limit yet.

#### Safe Pattern for Dual-Limit Eviction:

```rust
impl<K, V> Cache<K, V> {
    fn needs_eviction(&self, new_entry_size: u64) -> bool {
        // CRITICAL: OR condition, not AND
        (self.len() >= self.config.capacity.get()) || 
        (self.current_size + new_entry_size > self.config.max_size)
    }
    
    pub fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)> {
        // Evict until both limits are satisfied
        let mut evicted = None;
        while self.needs_eviction(size) {
            evicted = self.evict_one(); // May evict multiple entries
        }
        
        self.insert_new(key, value, size);
        evicted // Return last evicted entry
    }
}
```

#### Dual-Limit Safety Rules:

1. **Use OR logic for eviction triggers, not AND**
2. **Check limits BEFORE adding new entry size**
3. **May need multiple evictions to satisfy both limits**
4. **Test with both small entry-count and small size limits**

### 4. Concurrent Segment Distribution - CAPACITY MATH

Concurrent caches use segmented locking, but capacity limits apply to the ENTIRE cache, not per segment.

#### The Trap: Per-Segment vs Total Capacity

```rust
// WRONG: Each segment gets full capacity
let segments = 16;
let per_segment_config = CacheConfig {
    capacity: total_capacity,     // ⚠️ BUG: 16x more capacity than intended
    max_size: total_max_size,     // ⚠️ BUG: 16x more size than intended
};

// CORRECT: Divide capacity across segments
let segments = 16;
let per_segment_config = CacheConfig {
    capacity: NonZeroUsize::new(total_capacity.get() / segments).unwrap(),
    max_size: total_max_size / segments as u64,
};
```

**Why this matters**: Users expect concurrent caches to respect the same capacity limits as single-threaded caches.

#### Safe Pattern for Segment Capacity:

```rust
impl<K, V> ConcurrentSegmentedCache<K, V> {
    pub fn new(total_config: CacheConfig, num_segments: usize) -> Self {
        // Ensure num_segments is power of 2 for efficient hashing
        assert!(num_segments.is_power_of_two());
        
        let per_segment_capacity = total_config.capacity.get() / num_segments;
        let per_segment_size = total_config.max_size / num_segments as u64;
        
        // Ensure each segment gets at least capacity 1
        let per_segment_capacity = per_segment_capacity.max(1);
        let per_segment_size = per_segment_size.max(1);
        
        let segment_config = CacheConfig {
            capacity: NonZeroUsize::new(per_segment_capacity).unwrap(),
            max_size: per_segment_size,
        };
        
        let segments = (0..num_segments)
            .map(|_| RwLock::new(Cache::init(segment_config.clone(), None)))
            .collect();
            
        Self { segments, segment_mask: num_segments - 1 }
    }
}
```

#### Segment Capacity Safety Rules:

1. **Divide total capacity by number of segments**
2. **Ensure each segment gets at least capacity 1**
3. **Use power-of-2 segments for efficient hashing**
4. **Test that total cache respects expected limits**

### 5. Algorithm-Specific Size Requirements - API CONSISTENCY

Different algorithms have different requirements for size parameters, leading to API inconsistencies.

#### The Trap: Required vs Optional Size Parameters

```rust
// GDSF REQUIRES size parameter (not optional):
cache.put("key", value, size); // ✅ GDSF needs size for priority calculation

// LRU/LFU can use default size:
cache.put("key", value); // ✅ Defaults to size=1

// But mixing APIs causes confusion:
cache.put("key", value);        // Works for LRU
cache.put("key", value, size);  // Also works for LRU
// Users don't know which to use!
```

#### Safe Pattern for Size-Aware APIs:

```rust
impl<K, V> Cache<K, V> {
    /// Put with explicit size (recommended for size-aware algorithms)
    pub fn put_with_size(&mut self, key: K, value: V, size: u64) -> Option<(K, V)> {
        // Algorithm implementation...
    }
    
    /// Put with default size (for backward compatibility)
    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        self.put_with_size(key, value, 1) // Default size = 1
    }
}

// Document size requirements clearly:
/// For GDSF cache, always use `put_with_size()` for accurate size-aware eviction.
/// Using `put()` defaults to size=1, which may not reflect actual memory usage.
```

#### Size API Safety Rules:

1. **Provide both `put()` and `put_with_size()` for all algorithms**
2. **Document when size matters for algorithm correctness**
3. **Default size should be 1, not 0**
4. **Test both APIs to ensure consistent behavior**

### 6. Priority-Based Algorithm Invariants - MEMORY LEAKS

Algorithms using priority queues (LFU, LFUDA, GDSF) have subtle invariants around empty list cleanup.

#### The Trap: Memory Leaks from Empty Priority Lists

```rust
// MEMORY LEAK: Empty priority lists accumulate
let priority_list = self.priority_lists.get_mut(&priority).unwrap();
let entry = priority_list.remove_last(); // List might now be empty

// ⚠️ BUG: Empty list remains in BTreeMap, wasting memory
// Over time, many empty lists accumulate
```

#### Safe Pattern for Priority List Management:

```rust
fn remove_from_priority_list(&mut self, priority: Priority, entry_key: &K) -> Entry<K, V> {
    let priority_list = self.priority_lists.get_mut(&priority).unwrap();
    let entry = priority_list.remove_entry(entry_key).unwrap();
    
    // CRITICAL: Clean up empty lists to prevent memory leaks
    if priority_list.is_empty() {
        self.priority_lists.remove(&priority);
        
        // Update minimum priority tracking if needed
        if priority == self.min_priority {
            self.update_min_priority();
        }
    }
    
    entry
}
```

#### Priority Algorithm Safety Rules:

1. **ALWAYS remove empty priority lists from BTreeMap**
2. **Update min/max priority tracking when lists are removed**
3. **Test long-running scenarios to detect memory leaks**
4. **Monitor memory usage in benchmarks**

### 7. Box Pointer Management - DOUBLE FREE / MEMORY LEAKS

Raw pointer management with `Box::into_raw()` and `Box::from_raw()` must be perfectly balanced.

#### The Danger: Unbalanced Box Operations

```rust
// MEMORY LEAK: Box created but never freed
let entry = Box::new(Entry::new(key, value));
let node_ptr = Box::into_raw(entry);
self.map.insert(key, node_ptr);
// ⚠️ If map.remove() is never called, this leaks memory

// DOUBLE FREE: Box freed twice  
let node_ptr = self.map.remove(&key).unwrap();
unsafe {
    let _entry1 = Box::from_raw(node_ptr); // First free
    let _entry2 = Box::from_raw(node_ptr); // ⚠️ DOUBLE FREE - UNDEFINED BEHAVIOR
}
```

#### Safe Pattern for Box Management:

```rust
impl<K, V> Cache<K, V> {
    fn insert_new(&mut self, key: K, value: V) {
        let entry = Box::new(Entry::new(key.clone(), value));
        let node_ptr = Box::into_raw(entry);
        
        // Store pointer in map for later cleanup
        self.map.insert(key, node_ptr);
        
        // Store pointer in list for ordering
        self.list.push_front(node_ptr);
    }
    
    fn remove_entry(&mut self, key: &K) -> Option<Entry<K, V>> {
        let node_ptr = self.map.remove(key)?;
        
        // Remove from list first
        unsafe {
            self.list.remove_node(node_ptr);
        }
        
        // Convert back to Box for automatic cleanup
        unsafe {
            // SAFETY: node_ptr came from Box::into_raw and hasn't been freed
            Some(*Box::from_raw(node_ptr))
        }
    }
    
    fn clear(&mut self) {
        // Clean up all remaining pointers
        for (_, node_ptr) in self.map.drain() {
            unsafe {
                // SAFETY: All pointers came from Box::into_raw
                let _entry = Box::from_raw(node_ptr);
                // Box automatically drops
            }
        }
        self.list.clear();
    }
}
```

#### Box Management Safety Rules:

1. **Every `Box::into_raw()` must have exactly one matching `Box::from_raw()`**
2. **Remove pointers from ALL data structures before freeing**
3. **Implement `clear()` method to clean up all pointers**
4. **Use RAII patterns where possible to avoid manual management**

## Memory Safety Validation

### Required Miri Testing

**All unsafe code MUST pass Miri validation**:

```bash
# Install Miri
rustup +nightly component add miri

# Run Miri on all tests (REQUIRED for unsafe code)
cargo +nightly miri test

# Run Miri on specific test
cargo +nightly miri test test_unsafe_operations

# Run Miri with extra debugging
MIRIFLAGS="-Zmiri-debug" cargo +nightly miri test
```

**Common Miri errors and fixes**:

| Miri Error | Meaning | Typical Fix |
|------------|---------|-------------|
| "use of uninitialized memory" | Reading uninitialized data | Initialize all fields in constructors |
| "memory access failed: pointer is dangling" | Use after free | Don't use pointers after cleanup operations |
| "attempted read access using <tag> at alloc..." | Use after drop | Ensure proper Box management |
| "data race" | Concurrent access without sync | Add proper synchronization |

### Memory Leak Detection

**Test for memory leaks in long-running scenarios**:

```rust
#[test]
fn test_memory_stability() {
    let mut cache = make_cache(1000);
    
    // Simulate long-running usage with many evictions
    for i in 0..100_000 {
        cache.put(i, i);
        if i % 1000 == 0 {
            cache.clear(); // Test cleanup
        }
    }
    
    // Cache should be stable at this point
    assert_eq!(cache.len(), 1000);
}
```

### Concurrent Safety Testing

**Test concurrent safety under stress**:

```rust
#[test]
fn test_concurrent_memory_safety() {
    use std::sync::Arc;
    use std::thread;
    
    let cache = Arc::new(ConcurrentCache::new(make_config(1000)));
    
    // Stress test with many threads
    let handles: Vec<_> = (0..16).map(|thread_id| {
        let cache = Arc::clone(&cache);
        thread::spawn(move || {
            for i in 0..10_000 {
                let key = thread_id * 10_000 + i;
                cache.put(key, key);
                cache.get(&key);
                if i % 100 == 0 {
                    cache.remove(&key);
                }
            }
        })
    }).collect();
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    // Should not deadlock or corrupt data
}
```

## Debug Patterns for Memory Issues

### Debug Prints for Pointer Tracking

```rust
// Add debug assertions to track pointer validity
unsafe fn debug_validate_pointer(&self, node_ptr: *mut ListNode<T>) {
    #[cfg(debug_assertions)]
    {
        assert!(!node_ptr.is_null(), "Null pointer detected");
        assert!(self.map.values().any(|&p| p == node_ptr), 
               "Pointer not found in map - possibly freed");
    }
}
```

### Memory Usage Monitoring

```rust
impl<K, V> Cache<K, V> {
    /// Get detailed memory usage statistics (debug only)
    #[cfg(debug_assertions)]
    pub fn debug_memory_stats(&self) -> MemoryStats {
        MemoryStats {
            map_entries: self.map.len(),
            list_nodes: self.list.len(),
            total_pointers: self.map.len() + self.list.len(),
            estimated_bytes: self.estimate_memory_usage(),
        }
    }
}
```

## Memory Safety Checklist

When working with cache-rs unsafe code, verify:

**Pointer Safety:**
- [ ] All pointers in HashMap point to valid allocations
- [ ] No pointers used after list operations that might invalidate them
- [ ] All `Box::into_raw()` calls balanced with `Box::from_raw()`
- [ ] No null pointer dereferences

**Configuration Safety:**
- [ ] All `NonZeroUsize::new()` calls validated or use hardcoded non-zero values
- [ ] User input validated before creating configurations
- [ ] Capacity limits properly divided in concurrent variants

**Algorithm Safety:**
- [ ] Dual-limit eviction uses OR logic, not AND
- [ ] Empty priority lists cleaned up to prevent memory leaks
- [ ] Algorithm-specific invariants maintained

**Concurrent Safety:**
- [ ] Proper synchronization for all shared data
- [ ] No data races in concurrent variants
- [ ] Deadlock-free lock ordering

**Testing:**
- [ ] Miri validation passes for all unsafe code
- [ ] Memory leak testing for long-running scenarios
- [ ] Concurrent stress testing under heavy load
- [ ] Edge case testing (empty cache, single entry, capacity limits)

**Remember**: Cache algorithms are performance-critical code with extensive unsafe operations. Every memory safety pattern must be understood and followed rigorously to prevent undefined behavior, memory leaks, and data corruption.