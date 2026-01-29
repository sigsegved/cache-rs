# cache-rs

[![Crates.io](https://img.shields.io/crates/v/cache-rs.svg)](https://crates.io/crates/cache-rs)
[![Documentation](https://docs.rs/cache-rs/badge.svg)](https://docs.rs/cache-rs)
[![Build Status](https://github.com/sigsegved/cache-rs/workflows/Rust%20CI/badge.svg)](https://github.com/sigsegved/cache-rs/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance cache library for Rust with multiple eviction algorithms, `no_std` support, and thread-safe concurrent variants.

## Overview

cache-rs provides the cache metadata layer for both in-memory and disk-persisted content systems. Rather than storing actual content in memory, production systems typically use cache-rs to track *what* is cached and *where*—enabling intelligent eviction decisions while the content itself lives on disk, in object storage, or elsewhere.

This separation of concerns is critical for real-world caching infrastructure:

- **CDN edge servers** store content on disk but need in-memory metadata to decide which files to keep
- **Database buffer pools** track page locations and access patterns without duplicating page data
- **Object storage gateways** maintain cache indexes mapping keys to storage locations

cache-rs gives you the eviction intelligence without dictating your storage architecture.

### Key Capabilities

**Multiple Eviction Algorithms** — Five algorithms (LRU, SLRU, LFU, LFUDA, GDSF) cover different access patterns from simple recency-based eviction to sophisticated size-aware frequency tracking.

**Concurrent Cache Support** — Thread-safe variants of all algorithms use lock striping to minimize contention. Enable with the `concurrent` feature flag.

**`no_std` Compatible** — Works in embedded and resource-constrained environments. The default build uses `hashbrown` and requires only `alloc`.

## Quick Start

```toml
[dependencies]
cache-rs = "0.2.0"
```

All caches follow a unified initialization pattern: create a config struct for your chosen algorithm, then call `init(config, hasher)`. The second parameter accepts an optional custom hasher—pass `None` to use the default.

```rust
use cache_rs::LruCache;
use cache_rs::config::LruCacheConfig;
use std::num::NonZeroUsize;

let config = LruCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    max_size: u64::MAX,
};
let mut cache = LruCache::init(config, None);
```

### Common API

All cache types support these core operations:

| Method | Description |
|--------|-------------|
| `put(key, value)` | Insert or update an entry. Returns evicted entry if capacity exceeded. |
| `put_with_size(key, value, size)` | Insert with explicit size for size-limited caches. |
| `get(&key)` | Retrieve a reference to the value. Updates access metadata (e.g., moves to front in LRU). |
| `get_mut(&key)` | Retrieve a mutable reference. Updates access metadata. |
| `peek(&key)` | Retrieve without updating access metadata. |
| `remove(&key)` | Remove and return an entry. |
| `contains(&key)` | Check if key exists. |
| `len()` | Number of entries. |
| `is_empty()` | Whether cache is empty. |
| `clear()` | Remove all entries. |
| `capacity()` | Maximum number of entries. |

**GDSF note**: Use `put(key, value, size)` — GDSF requires size for priority calculation.

### Example: Basic Usage

```rust
use cache_rs::LruCache;
use cache_rs::config::LruCacheConfig;
use std::num::NonZeroUsize;

let config = LruCacheConfig {
    capacity: NonZeroUsize::new(100).unwrap(),
    max_size: u64::MAX,
};
let mut cache = LruCache::init(config, None);

// Insert entries
cache.put("user:1001", "Alice");
cache.put("user:1002", "Bob");

// Retrieve (updates LRU position)
assert_eq!(cache.get(&"user:1001"), Some(&"Alice"));

// Peek (does not update LRU position)
assert_eq!(cache.peek(&"user:1002"), Some(&"Bob"));

// Check existence
assert!(cache.contains(&"user:1001"));
assert!(!cache.contains(&"user:9999"));

// Remove
cache.remove(&"user:1001");
assert_eq!(cache.get(&"user:1001"), None);
```

---

## Eviction Algorithms

Each algorithm optimizes for different access patterns. See [ALGORITHM_GUIDE.md](ALGORITHM_GUIDE.md) for detailed use cases and examples.

### LRU (Least Recently Used)

Evicts the entry that hasn't been accessed for the longest time. This is the simplest and most widely applicable algorithm.

**Eviction policy**: Maintain a doubly-linked list ordered by access time. On access, move the entry to the front. On eviction, remove from the back.

**When to use**: General-purpose caching where recent access is a good predictor of future access. Web sessions, query results, configuration data.

**Time complexity**: O(1) for all operations.

```rust
use cache_rs::LruCache;
use cache_rs::config::LruCacheConfig;
use std::num::NonZeroUsize;

let config = LruCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    max_size: u64::MAX,
};
let mut cache = LruCache::init(config, None);
```

### SLRU (Segmented LRU)

Divides the cache into two segments: **probationary** and **protected**. New entries start in probationary. A second access promotes them to protected. Eviction happens from probationary first.

**Eviction policy**: Entries must prove their worth by being accessed twice before gaining protection. This provides scan resistance—a sequential read of many items won't evict your hot working set.

**When to use**: Database buffer pools, file system caches, any workload mixing random access with sequential scans.

**Time complexity**: O(1) for all operations.

```rust
use cache_rs::SlruCache;
use cache_rs::config::SlruCacheConfig;
use std::num::NonZeroUsize;

let config = SlruCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    protected_capacity: NonZeroUsize::new(200).unwrap(),  // 20% protected
    max_size: u64::MAX,
};
let mut cache = SlruCache::init(config, None);
```

### LFU (Least Frequently Used)

Tracks how many times each entry has been accessed. Evicts the entry with the lowest frequency count.

**Eviction policy**: Maintain frequency counters for each entry. Group entries by frequency in a priority structure. Evict from the lowest frequency group.

**When to use**: Workloads where some items are consistently more popular than others. API caching, CDN content, static asset serving.

**Caveat**: Items that were popular historically but aren't anymore can "stick" in cache. Use LFUDA if popularity changes over time.

**Time complexity**: O(log F) where F = distinct frequency values. Since frequencies are small integers (1, 2, 3, ...), F is bounded and operations are effectively O(1).

```rust
use cache_rs::LfuCache;
use cache_rs::config::LfuCacheConfig;
use std::num::NonZeroUsize;

let config = LfuCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    max_size: u64::MAX,
};
let mut cache = LfuCache::init(config, None);
```

### LFUDA (LFU with Dynamic Aging)

Extends LFU with a global age counter that increases on each eviction. New entries start with priority equal to the current age plus their frequency, allowing them to eventually compete with historically popular items.

**Eviction policy**: Priority = frequency + age_at_insertion. The global age increases each eviction. This prevents old popular items from permanently occupying cache space.

**When to use**: Long-running services where popularity changes—news feeds, social media, e-commerce with seasonal trends.

**Time complexity**: O(log P) where P = distinct priority values. Priority = frequency + age, so P can grow with cache size.

```rust
use cache_rs::LfudaCache;
use cache_rs::config::LfudaCacheConfig;
use std::num::NonZeroUsize;

let config = LfudaCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    initial_age: 0,
    max_size: u64::MAX,
};
let mut cache = LfudaCache::init(config, None);
```

### GDSF (Greedy Dual-Size Frequency)

Designed for variable-sized objects. Considers frequency, size, and age when making eviction decisions. Smaller popular items get higher priority than large rarely-accessed items.

**Eviction policy**: Priority = (frequency / size) + age. This maximizes hit rate (not byte hit rate) by preferring many small popular items over few large ones.

**When to use**: CDN metadata caches, file caches, any workload where object sizes vary significantly.

**Note**: The `put` method requires a `size` parameter.

**Time complexity**: O(log P) where P = distinct priority buckets. Priority = (frequency/size) + age.

```rust
use cache_rs::GdsfCache;
use cache_rs::config::GdsfCacheConfig;
use std::num::NonZeroUsize;

let config = GdsfCacheConfig {
    capacity: NonZeroUsize::new(10000).unwrap(),
    initial_age: 0.0,
    max_size: 100 * 1024 * 1024,  // 100 MB
};
let mut cache = GdsfCache::init(config, None);

// put() requires size parameter
cache.put("small.txt", "content", 1024);        // 1 KB
cache.put("large.bin", "content", 10_000_000);  // 10 MB
```

---

## Concurrent Cache Support

For multi-threaded workloads, enable the `concurrent` feature:

```toml
[dependencies]
cache-rs = { version = "0.2.0", features = ["concurrent"] }
```

Concurrent caches use **lock striping**—data is partitioned across multiple segments, each with its own lock. Threads accessing different segments proceed in parallel; only threads hitting the same segment contend.

### Available Types

| Type | Base Algorithm |
|------|----------------|
| `ConcurrentLruCache` | LRU |
| `ConcurrentSlruCache` | SLRU |
| `ConcurrentLfuCache` | LFU |
| `ConcurrentLfudaCache` | LFUDA |
| `ConcurrentGdsfCache` | GDSF |

### Example

```rust
use cache_rs::concurrent::ConcurrentLruCache;
use cache_rs::config::{ConcurrentCacheConfig, LruCacheConfig};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::thread;

let config = ConcurrentCacheConfig {
    base: LruCacheConfig {
        capacity: NonZeroUsize::new(10_000).unwrap(),
        max_size: u64::MAX,
    },
    segments: 16,  // Power of 2 recommended
};
let cache = Arc::new(ConcurrentLruCache::init(config, None));

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

### Zero-Copy Access

Use `get_with` to process values without cloning:

```rust
use cache_rs::concurrent::ConcurrentLruCache;
use cache_rs::config::{ConcurrentCacheConfig, LruCacheConfig};
use std::num::NonZeroUsize;

let config = ConcurrentCacheConfig {
    base: LruCacheConfig {
        capacity: NonZeroUsize::new(100).unwrap(),
        max_size: u64::MAX,
    },
    segments: 16,
};
let cache = ConcurrentLruCache::init(config, None);
cache.put("data".to_string(), vec![1u8; 1024]);

// Process in-place without cloning
let sum: Option<u8> = cache.get_with(&"data".to_string(), |bytes| {
    bytes.iter().copied().sum()
});
```

---

## Managing Disk-Persisted Content

cache-rs excels as the metadata layer for disk-backed caching systems. The pattern: store content on disk, use cache-rs to track what's cached and make eviction decisions.

### Two-Tier Cache Pattern

```rust
use cache_rs::LruCache;
use cache_rs::config::LruCacheConfig;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::fs;

/// Metadata for disk-cached content
#[derive(Clone)]
struct DiskEntry {
    path: PathBuf,
    size: u64,
}

struct TieredCache {
    index: LruCache<String, DiskEntry>,  // In-memory index
    cache_dir: PathBuf,                   // Disk storage
}

impl TieredCache {
    fn new(capacity: usize, cache_dir: PathBuf) -> Self {
        let config = LruCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            max_size: u64::MAX,
        };
        fs::create_dir_all(&cache_dir).unwrap();
        TieredCache {
            index: LruCache::init(config, None),
            cache_dir,
        }
    }

    fn get(&mut self, key: &str) -> Option<Vec<u8>> {
        // Check index for disk location
        let entry = self.index.get(&key.to_string())?;
        
        // Read from disk
        fs::read(&entry.path).ok()
    }

    fn put(&mut self, key: &str, content: &[u8]) {
        let path = self.cache_dir.join(format!("{:x}.cache", self.hash(key)));
        
        // Write to disk
        fs::write(&path, content).unwrap();
        
        // Update index (may evict old entry)
        if let Some((_, evicted)) = self.index.put(
            key.to_string(),
            DiskEntry { path: path.clone(), size: content.len() as u64 }
        ) {
            // Clean up evicted file from disk
            let _ = fs::remove_file(&evicted.path);
        }
    }

    fn hash(&self, s: &str) -> u64 {
        s.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
    }
}
```

---

## Configuration Reference

### LruCacheConfig

```rust
LruCacheConfig {
    capacity: NonZeroUsize,  // Maximum number of entries
    max_size: u64,           // Maximum total size in bytes (use u64::MAX for unlimited)
}
```

### SlruCacheConfig

```rust
SlruCacheConfig {
    capacity: NonZeroUsize,           // Total capacity (probationary + protected)
    protected_capacity: NonZeroUsize, // Size of protected segment
    max_size: u64,
}
```

### LfuCacheConfig

```rust
LfuCacheConfig {
    capacity: NonZeroUsize,
    max_size: u64,
}
```

### LfudaCacheConfig

```rust
LfudaCacheConfig {
    capacity: NonZeroUsize,
    initial_age: u64,  // Starting value for global age counter
    max_size: u64,
}
```

### GdsfCacheConfig

```rust
GdsfCacheConfig {
    capacity: NonZeroUsize,
    initial_age: f64,  // Starting value for global age
    max_size: u64,     // Important for GDSF—limits total cached size
}
```

### ConcurrentCacheConfig

```rust
ConcurrentCacheConfig {
    base: /* any of the above configs */,
    segments: usize,  // Number of segments (power of 2 recommended)
}
```

---

## Performance

| Algorithm | Get | Put | Memory Overhead |
|-----------|-----|-----|-----------------|
| LRU | ~887ns | ~850ns | ~80 bytes/entry |
| SLRU | ~983ns | ~950ns | ~90 bytes/entry |
| LFU | ~22.7µs | ~22µs | ~100 bytes/entry |
| LFUDA | ~20.5µs | ~21µs | ~110 bytes/entry |
| GDSF | ~7.5µs | ~8µs | ~120 bytes/entry |

Run benchmarks: `cargo bench`

---

## `no_std` Support

cache-rs works out of the box in `no_std` environments:

```rust
#![no_std]
extern crate alloc;

use cache_rs::LruCache;
use cache_rs::config::LruCacheConfig;
use core::num::NonZeroUsize;
use alloc::string::String;

let config = LruCacheConfig {
    capacity: NonZeroUsize::new(10).unwrap(),
    max_size: u64::MAX,
};
let mut cache = LruCache::init(config, None);
cache.put(String::from("key"), "value");
```

### Feature Flags

| Feature | Description |
|---------|-------------|
| (default) | `no_std` + `hashbrown` |
| `std` | Standard library support |
| `concurrent` | Thread-safe caches (requires `std`) |
| `nightly` | Nightly optimizations |

---

## Documentation

- [API Documentation](https://docs.rs/cache-rs)
- [Algorithm Guide](ALGORITHM_GUIDE.md)
- [Examples](examples/)
- [Benchmarks](benches/)
- [Miri Analysis](MIRI_ANALYSIS.md)

## Contributing

```bash
cargo test --features "std,concurrent"
cargo fmt --all -- --check
cargo clippy --features "std,concurrent" -- -D warnings
cargo doc --no-deps --document-private-items
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

[MIT](LICENSE)

## Security

See [SECURITY.md](SECURITY.md).
