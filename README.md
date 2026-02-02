# cache-rs

[![Crates.io](https://img.shields.io/crates/v/cache-rs.svg)](https://crates.io/crates/cache-rs)
[![Documentation](https://docs.rs/cache-rs/badge.svg)](https://docs.rs/cache-rs)
[![Build Status](https://github.com/sigsegved/cache-rs/workflows/Rust%20CI/badge.svg)](https://github.com/sigsegved/cache-rs/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance cache library for Rust with multiple eviction algorithms, `no_std` support, and thread-safe concurrent variants.

> **⚠️ Pre-1.0 Notice:** cache-rs is under active development and the API may change between minor versions. We're rapidly iterating toward a stable 1.0.0 release. Pin your dependency version if you need stability, and check the [CHANGELOG](CHANGELOG.md) when upgrading.

## The Problem with "One Size Fits All" Caching

There is no universally optimal cache eviction algorithm. LRU works beautifully for session stores where recent activity predicts future access, but deploy it on a CDN and a single sequential scan can flush your entire hot working set. LFU excels when popularity is stable, keeping your most-requested assets warm, until yesterday's viral content blocks today's trending items from ever getting cached. GDSF maximizes hit rates for variable-sized objects by favoring small popular files, but adds complexity you don't need when all your cached items are the same size.

Real-world caching infrastructure demands the right algorithm for the job. A database buffer pool needs scan resistance (SLRU). A news feed cache needs frequency tracking with aging (LFUDA). A CDN metadata index needs size-aware eviction (GDSF). Benchmarks across 47 million requests show that algorithm selection can mean the difference between a 30% hit rate and a 90% hit rate. Size-aware algorithms provide **50-90 percentage point improvements** when storage constraints dominate.

cache-rs gives you the full toolkit: five battle-tested eviction algorithms with a unified API, so you can choose the right strategy for your workload without rewriting your caching layer.

## Why cache-rs?

cache-rs is a high-performance in-memory cache library that gives you control over how your cache behaves. Instead of a one-size-fits-all eviction policy, you choose from five algorithms (LRU, SLRU, LFU, LFUDA, and GDSF) behind a unified API. Start with LRU for simplicity and speed, swap in SLRU if sequential scans are polluting your cache, or GDSF if your objects vary in size. The API remains the same; only the eviction behavior changes.

The library fits into multiple architectural patterns. Use it as a straightforward in-memory cache for database query results, API responses, or computed values. Use it as a metadata index for disk-backed CDN caches, where you store file locations and headers in cache-rs while the actual content lives on disk. Use it as a cache lookup layer for shared memory systems, where cache-rs tracks keys and offsets while another process or subsystem manages the raw data. The eviction logic stays the same regardless of where your data actually lives, be it in-memory local to cache-rs, or on disk or on shared-memory.

The core design prioritizes predictable, low-latency operations. LRU and SLRU run in O(1) time. The frequency-based algorithms (LFU, LFUDA, GDSF) run in O(log P) where P is the number of distinct priority buckets, though in practice this is often small. The cache stores pointers in a HashMap while values live in cache-friendly linked structures, minimizing memory overhead and cache misses.

For multi-threaded applications, every algorithm has a concurrent counterpart. These use segmented locking rather than a single global lock: keys hash to independent segments, so threads accessing different parts of the cache proceed in parallel. Enable concurrent caches with the `concurrent` feature flag and wrap in `Arc` for shared ownership across threads.

You can set dual capacity limits: one for entry count (to bound memory overhead from metadata) and one for total content size (to bound actual data storage). Eviction triggers when either limit is exceeded, giving you precise control over memory consumption.

The library works in `no_std` environments out of the box. The default build uses `hashbrown` and requires only `alloc`, making it suitable for embedded systems and kernel development. If you need standard library features or concurrent caches, opt in with the `std` or `concurrent` feature flags.

cache-rs is production-hardened: extensive test coverage, Miri-verified for memory safety, and benchmarked across realistic workloads. It serves as the foundation for cache simulation research, so the algorithms have been validated against real-world access patterns. For a deep dive into how each algorithm performs under different workloads, see our [Analysis Report](ANALYSIS_REPORT.md), which covers 47 million cache operations across video streaming, social media, web content, and on-disk caching scenarios.

## Quick Start

```toml
[dependencies]
cache-rs = "0.3.0"
```

All caches follow a unified initialization pattern: create a config struct for your chosen algorithm, then call `init(config, hasher)`. The second parameter accepts an optional custom hasher; pass `None` to use the default.

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
| `remove(&key)` | Remove and return an entry. |
| `len()` | Number of entries. |
| `is_empty()` | Whether cache is empty. |
| `clear()` | Remove all entries. |
| `cap()` | Maximum capacity (LRU/LFU/LFUDA/SLRU). |

**Algorithm-specific methods:**
- **GDSF**: Use `put(key, value, size)` (requires size for priority calculation) and `contains_key(&key)` to check existence.
- **GDSF**: Use `pop(&key)` instead of `remove(&key)` to remove entries.

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

// Check existence (also updates LRU position if present)
assert!(cache.get(&"user:1001").is_some());
assert!(cache.get(&"user:9999").is_none());

// Remove
cache.remove(&"user:1001");
assert_eq!(cache.get(&"user:1001"), None);
```

---

### Quick Reference

| Scenario | Algorithm | Why |
|----------|-----------|-----|
| General purpose, simple workload | **LRU** | Fastest, lowest overhead, predictable behavior |
| Database buffer pool, file cache | **SLRU** | Scan resistance protects hot working set |
| CDN with stable popular content | **LFU** | Frequency tracking keeps popular items cached |
| Long-running service, trends change | **LFUDA** | Aging prevents stale popular items from persisting |
| Variable-sized objects (images, files) | **GDSF** | Size-aware eviction maximizes hit rate |

### Test with Your Own Traffic

Not sure which algorithm fits your workload? The repository includes a **cache-simulator** tool that replays your traffic logs against all five algorithms and reports hit rates, byte hit rates, and latency statistics. Feed it your production access patterns and let the data guide your decision.

```bash
cd cache-simulator
cargo run --release -- --input your-traffic.log --cache-size 1000
```

See [cache-simulator/README.md](cache-simulator/README.md) for input format and options.

See [ALGORITHM_GUIDE.md](ALGORITHM_GUIDE.md) for detailed use cases and code examples.

---

## Eviction Algorithms

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

**Eviction policy**: Entries must prove their worth by being accessed twice before gaining protection. This provides scan resistance: a sequential read of many items won't evict your hot working set.

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

**When to use**: Long-running services where popularity changes (news feeds, social media, e-commerce with seasonal trends).

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
cache-rs = { version = "0.3.0", features = ["concurrent"] }
```

### Why Mutex, Not RwLock?

You might expect a cache to use `RwLock` so multiple readers can proceed in parallel. However, cache algorithms like LRU, LFU, and SLRU require **mutable access even for reads**:

- **LRU**: `get()` moves the accessed item to the front of the recency list
- **LFU**: `get()` increments the frequency counter and may move items between buckets
- **SLRU**: `get()` may promote items from probationary to protected segment
- **LFUDA/GDSF**: `get()` updates priority calculations

Since every `get()` mutates internal state, `RwLock` would provide no benefit; all operations need exclusive access anyway. cache-rs uses `parking_lot::Mutex` for lower overhead and achieves concurrency through **segmentation**: different keys hash to different segments and can be accessed in parallel.

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

## Advanced: Disk-Backed and Tiered Caches

While cache-rs works great as a standalone in-memory cache, it also excels as the metadata layer for disk-backed caching systems. In this pattern, you store actual content on disk (or in object storage, or any external system), while cache-rs tracks *what* is cached and makes eviction decisions. When cache-rs evicts an entry, you use that signal to delete the corresponding file from disk.

This separation is common in production infrastructure: CDN edge servers keep files on disk but need in-memory metadata to decide which files to keep. Database buffer pools track page locations without duplicating page data. Object storage gateways maintain indexes mapping keys to storage backends. The example below shows this two-tier pattern in action.

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
    max_size: u64,     // Important for GDSF: limits total cached size
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

## Roadmap & Contributing

cache-rs provides a solid foundation for cache eviction, but several features commonly needed in production systems are not yet implemented. **Contributions are welcome!**

### Planned Features

| Feature | Description | Status |
|---------|-------------|--------|
| **TTL Support** | Time-based expiration for cache entries | Not started |
| **Eviction Callbacks** | Hooks to notify when entries are evicted (for cleanup, persistence, metrics) | Not started |
| **Admission Policies** | Decide whether to cache an item at all (e.g., TinyLFU admission) | Not started |
| **Enhanced Metrics** | Hit/miss counters, eviction counts, latency histograms | Basic |
| **Async Support** | `async`-compatible concurrent caches | Not started |
| **Weighted Entries** | Custom cost functions beyond simple size | Not started |

### How to Contribute

```bash
# Run the full validation suite before submitting
cargo test --features "std,concurrent"
cargo fmt --all -- --check
cargo clippy --features "std,concurrent" -- -D warnings
cargo doc --no-deps --document-private-items
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

---

## Documentation

- [API Documentation](https://docs.rs/cache-rs)
- [Algorithm Guide](ALGORITHM_GUIDE.md): Detailed examples and use cases for each algorithm
- [Analysis Report](ANALYSIS_REPORT.md): Empirical evaluation across 47M requests
- [Examples](examples/)
- [Benchmarks](benches/)
- [Miri Analysis](MIRI_ANALYSIS.md): Memory safety verification

## License

[MIT](LICENSE)

## Security

See [SECURITY.md](SECURITY.md).
