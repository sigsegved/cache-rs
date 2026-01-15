# cache-rs

[![Build Status](https://github.com/sigsegved/cache-rs/workflows/Rust%20CI/badge.svg)](https://github.com/sigsegved/cache-rs/actions)
[![Codecov](https://codecov.io/gh/sigsegved/cache-rs/branch/main/graph/badge.svg)](https://codecov.io/gh/sigsegved/cache-rs)
[![Crates.io](https://img.shields.io/crates/v/cache-rs.svg)](https://crates.io/crates/cache-rs)
[![Documentation](https://docs.rs/cache-rs/badge.svg)](https://docs.rs/cache-rs)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance, memory-efficient cache library for Rust supporting multiple eviction algorithms with O(1) operations.

## ✨ Features

- **Multiple eviction algorithms**: LRU, LFU, LFUDA, SLRU, GDSF
- **High performance**: All operations are O(1) with optimized data structures
- **Memory efficient**: Minimal overhead with careful memory layout
- **`no_std` compatible**: Works in embedded and resource-constrained environments
- **Thread-safe ready**: Easy to wrap with `Mutex`/`RwLock` for concurrent access
- **Well documented**: Comprehensive documentation with usage examples

## 🚀 Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
cache-rs = "0.2.0"
```

Basic usage:

```rust
use cache_rs::LruCache;
use std::num::NonZeroUsize;

let mut cache = LruCache::new(NonZeroUsize::new(100).unwrap());

cache.put("key", "value");
assert_eq!(cache.get(&"key"), Some(&"value"));
```

## 📖 Algorithm Guide

Choose the right cache algorithm for your use case:

### LRU (Least Recently Used)
**Best for**: General-purpose caching with temporal locality
```rust
use cache_rs::LruCache;
use std::num::NonZeroUsize;

let mut cache = LruCache::new(NonZeroUsize::new(100).unwrap());
cache.put("recent", "data");
```

### SLRU (Segmented LRU)  
**Best for**: Workloads with scan resistance requirements
```rust
use cache_rs::SlruCache;
use std::num::NonZeroUsize;

// Total capacity: 100, Protected segment: 20
let mut cache = SlruCache::new(
    NonZeroUsize::new(100).unwrap(),
    NonZeroUsize::new(20).unwrap()
);
```

### LFU (Least Frequently Used)
**Best for**: Workloads with strong frequency patterns
```rust
use cache_rs::LfuCache;
use std::num::NonZeroUsize;

let mut cache = LfuCache::new(NonZeroUsize::new(100).unwrap());
cache.put("frequent", "data");
```

### LFUDA (LFU with Dynamic Aging)
**Best for**: Long-running applications where access patterns change
```rust
use cache_rs::LfudaCache;
use std::num::NonZeroUsize;

let mut cache = LfudaCache::new(NonZeroUsize::new(100).unwrap());
```

### GDSF (Greedy Dual Size Frequency)
**Best for**: Variable-sized objects (images, files, documents)
```rust
use cache_rs::GdsfCache;
use std::num::NonZeroUsize;

let mut cache = GdsfCache::new(NonZeroUsize::new(1000).unwrap());
cache.put("image.jpg", image_data, 250); // key, value, size
```

## 📊 Performance Comparison

| Algorithm | Get Operation | Use Case | Memory Overhead |
|-----------|--------------|----------|-----------------|
| **LRU**   | ~887ns      | General purpose | Low |
| **SLRU**  | ~983ns      | Scan resistance | Medium |
| **GDSF**  | ~7.5µs      | Size-aware | Medium |
| **LFUDA** | ~20.5µs     | Aging workloads | Medium |
| **LFU**   | ~22.7µs     | Frequency-based | Medium |

*Benchmarks run on mixed workloads with Zipf distribution*

## 🏗️ no_std Support

Works out of the box in `no_std` environments:

```rust
#![no_std]
extern crate alloc;

use cache_rs::LruCache;
use core::num::NonZeroUsize;
use alloc::string::String;

let mut cache = LruCache::new(NonZeroUsize::new(10).unwrap());
cache.put(String::from("key"), "value");
```

## ⚙️ Feature Flags

- `hashbrown` (default): Use hashbrown HashMap for better performance
- `nightly`: Enable nightly-only optimizations  
- `std`: Enable standard library features (disabled by default)
- `concurrent`: Enable thread-safe concurrent cache types (uses `parking_lot`)

```toml
# Default: no_std + hashbrown (recommended for most use cases)
cache-rs = "0.2.0"

# Concurrent caching (recommended for multi-threaded apps)
cache-rs = { version = "0.2.0", features = ["concurrent"] }

# std + hashbrown (recommended for std environments)
cache-rs = { version = "0.2.0", features = ["std"] }

# std + concurrent + nightly optimizations
cache-rs = { version = "0.2.0", features = ["std", "concurrent", "nightly"] }

# no_std + nightly optimizations only
cache-rs = { version = "0.2.0", features = ["nightly"] }

# Only std::HashMap (not recommended - slower than hashbrown)
cache-rs = { version = "0.2.0", default-features = false, features = ["std"] }
```

## 🧵 Concurrent Cache Support

For high-performance multi-threaded scenarios, cache-rs provides dedicated concurrent cache types with the `concurrent` feature:

```toml
[dependencies]
cache-rs = { version = "0.2.0", features = ["concurrent"] }
```

### Available Concurrent Types

| Type | Description |
|------|-------------|
| `ConcurrentLruCache` | Thread-safe LRU with segmented storage |
| `ConcurrentSlruCache` | Thread-safe Segmented LRU |
| `ConcurrentLfuCache` | Thread-safe LFU |
| `ConcurrentLfudaCache` | Thread-safe LFUDA |
| `ConcurrentGdsfCache` | Thread-safe GDSF |

### Usage Example

```rust
use cache_rs::ConcurrentLruCache;
use std::sync::Arc;
use std::thread;

// Create a concurrent cache (default 16 segments)
let cache = Arc::new(ConcurrentLruCache::new(
    std::num::NonZeroUsize::new(10000).unwrap()
));

// Access from multiple threads
let handles: Vec<_> = (0..8).map(|i| {
    let cache = Arc::clone(&cache);
    thread::spawn(move || {
        for j in 0..1000 {
            let key = format!("thread{}-key{}", i, j);
            cache.put(key.clone(), i * 1000 + j);
            cache.get(&key);
        }
    })
}).collect();

for handle in handles {
    handle.join().unwrap();
}
```

### Zero-Copy Access with `get_with`

Avoid cloning large values by processing them in-place:

```rust
use cache_rs::ConcurrentLruCache;
use std::num::NonZeroUsize;

let cache = ConcurrentLruCache::new(NonZeroUsize::new(100).unwrap());
cache.put("large_data".to_string(), vec![1u8; 1024]);

// Process value without cloning
let sum: Option<u8> = cache.get_with(&"large_data".to_string(), |data| {
    data.iter().sum()
});
```

### Segment Tuning

Configure segment count based on your workload:

```rust
use cache_rs::ConcurrentLruCache;
use std::num::NonZeroUsize;

// More segments = better concurrency, higher memory overhead
let cache = ConcurrentLruCache::with_segments(
    NonZeroUsize::new(10000).unwrap(),
    32  // Power of 2 recommended
);
```

### Performance Characteristics

| Segments | 8-Thread Mixed Workload |
|----------|-------------------------|
| 1        | ~464µs |
| 8        | ~441µs |
| 16       | ~379µs |
| 32       | ~334µs (optimal) |
| 64       | ~372µs |

## Thread Safety (Manual Wrapping)

For simpler use cases, you can also wrap single-threaded caches manually:

```rust
use cache_rs::LruCache;
use std::sync::{Arc, Mutex};
use std::num::NonZeroUsize;

let cache = Arc::new(Mutex::new(
    LruCache::new(NonZeroUsize::new(100).unwrap())
));

// Clone Arc for use in other threads
let cache_clone = Arc::clone(&cache);
```

## 🔧 Advanced Usage

### Custom Hash Function

```rust
use cache_rs::LruCache;
use std::collections::hash_map::RandomState;
use std::num::NonZeroUsize;

let cache = LruCache::with_hasher(
    NonZeroUsize::new(100).unwrap(),
    RandomState::new()
);
```

### Size-aware Caching with GDSF

```rust
use cache_rs::GdsfCache;
use std::num::NonZeroUsize;

let mut cache = GdsfCache::new(NonZeroUsize::new(1000).unwrap());

// Cache different sized objects
cache.put("small.txt", "content", 10);
cache.put("medium.jpg", image_bytes, 500);
cache.put("large.mp4", video_bytes, 2000);

// GDSF automatically considers size, frequency, and recency
```

## 🏃‍♂️ Benchmarks

Run the included benchmarks to compare performance:

```bash
cargo bench
```

Example results on modern hardware:
- **LRU**: Fastest for simple use cases (~887ns per operation)
- **SLRU**: Good balance of performance and scan resistance (~983ns)
- **GDSF**: Best for size-aware workloads (~7.5µs)
- **LFUDA/LFU**: Best for frequency-based patterns (~20µs)

## 📚 Documentation

- [API Documentation](https://docs.rs/cache-rs)
- [Examples](examples/)
- [Benchmarks](benches/)

## 🤝 Contributing

Contributions welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development

```bash
# Run all tests
cargo test --all

# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --all-targets -- -D warnings

# Test no_std compatibility
cargo build --target thumbv6m-none-eabi --no-default-features --features hashbrown

# Run Miri for unsafe code validation (detects undefined behavior)
MIRIFLAGS="-Zmiri-ignore-leaks" cargo +nightly miri test --lib
```

See [MIRI_ANALYSIS.md](MIRI_ANALYSIS.md) for a detailed Miri usage guide and analysis of findings.

## 📄 License

Licensed under the [MIT License](LICENSE).

## 🔒 Security

For security concerns, see [SECURITY.md](SECURITY.md).
