# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-01-14

### ✨ Added

- **Concurrent Cache Implementations**: New thread-safe cache types with segmented storage for high-performance multi-threaded access:
  - `ConcurrentLruCache`: Thread-safe LRU with configurable segments
  - `ConcurrentSlruCache`: Thread-safe Segmented LRU
  - `ConcurrentLfuCache`: Thread-safe LFU
  - `ConcurrentLfudaCache`: Thread-safe LFUDA
  - `ConcurrentGdsfCache`: Thread-safe GDSF

- **New `concurrent` Feature Flag**: Enable concurrent cache types with `features = ["concurrent"]`. Uses `parking_lot` for efficient locking.

- **Zero-Copy `get_with()` API**: All concurrent caches provide `get_with(key, f)` for processing values without cloning:
  ```rust
  let sum = cache.get_with(&key, |value| value.iter().sum());
  ```

- **Configurable Segment Count**: Tune concurrency vs memory overhead with `with_segments(capacity, segment_count)`:
  ```rust
  let cache = ConcurrentLruCache::with_segments(capacity, 32);
  ```

- **Concurrent Benchmarks**: New benchmark suite in `benches/concurrent_benchmarks.rs` measuring multi-threaded performance

- **Stress Tests**: Comprehensive concurrency stress tests in `tests/concurrent_stress_tests.rs`

### 🔧 Changed

- **Internal Segment Extraction**: Refactored all cache algorithms to extract core logic into reusable `*Segment` types:
  - `LruSegment`, `SlruSegment`, `LfuSegment`, `LfudaSegment`, `GdsfSegment`
  - Single-threaded caches now wrap segments, sharing code with concurrent implementations
  - **No API changes** for existing single-threaded cache users

### 📝 Documentation

- Updated README with Concurrency Support section
- Added performance characteristics table for segment tuning
- Created `examples/concurrent_usage.rs` demonstrating multi-threaded patterns

### 🔒 Backwards Compatibility

- All existing single-threaded cache APIs remain unchanged
- `concurrent` feature is opt-in; disabled by default
- No breaking changes for existing users

### 🎯 Performance

Benchmark results for 8-thread mixed workload (get/put operations):

| Segments | Throughput |
|----------|------------|
| 1        | ~464µs |
| 16       | ~379µs |
| 32       | ~334µs (optimal) |
| 64       | ~372µs |

---

**Full Changelog**: https://github.com/sigsegved/cache-rs/compare/v0.1.1...v0.2.0

## [0.1.1] - 2026-01-05

### 🐛 Fixed

- **Unsafe Code Safety**: Fixed Stacked Borrows violations in unsafe code blocks
  - Resolved undefined behavior detected by Miri in pointer operations
  - Improved safety guarantees across all cache implementations
  - All unsafe code now passes Miri's strict memory model checks

### 🔧 Changed

- **CI/CD Improvements**: Added Miri integration to continuous integration
  - Automated detection of undefined behavior in unsafe code
  - Enhanced test coverage with Miri sanitizer checks
  - Ensures memory safety across all platforms and architectures
- **Dependencies**: Updated `hashbrown` from 0.13.2 to 0.14.5
  - Optimized insertion to perform only a single lookup
  - Improved performance of table resizing
  - Added ARM NEON optimizations for better performance on ARM platforms
  - Fixed custom allocator memory leaks
  - Maintained MSRV compatibility (1.74.0)

### 📝 Documentation

- Added comprehensive Miri analysis documentation (MIRI_ANALYSIS.md)
- Enhanced safety documentation for unsafe code patterns

---

**Full Changelog**: https://github.com/sigsegved/cache-rs/compare/v0.1.0...v0.1.1

## [0.1.0] - 2025-08-04

### ✨ Added

#### **Core Cache Implementations**
- **LRU Cache**: Least Recently Used eviction with O(1) operations
- **LFU Cache**: Least Frequently Used eviction with frequency-based priorities
- **LFUDA Cache**: LFU with Dynamic Aging to prevent stale items from persisting
- **SLRU Cache**: Segmented LRU with probationary and protected segments for scan resistance
- **GDSF Cache**: Greedy Dual-Size Frequency for size-aware caching (ideal for CDNs)

#### **Performance Features**
- All cache operations (get, put, remove) are **O(1) time complexity**
- **hashbrown** integration by default for superior HashMap performance
- Optimized memory layout with doubly-linked lists and hash maps
- Zero-allocation cache hits for maximum performance

#### **no_std Compatibility**
- Full `no_std` support for embedded and resource-constrained environments  
- Uses `alloc` crate for heap allocations only when necessary
- Cross-platform compilation tested on `thumbv6m-none-eabi` target

#### **Comprehensive Metrics System**
- Built-in metrics tracking for all cache implementations
- Algorithm-specific metrics (hit rates, eviction patterns, aging events)
- Deterministic metrics ordering using `BTreeMap` for reproducible analysis
- Integration-ready for performance monitoring and simulation systems

#### **Safety & Reliability**
- **66 comprehensive SAFETY comments** documenting all unsafe code blocks
- Extensive unsafe code for performance with safe public APIs
- Memory safety guaranteed through careful pointer management
- Industry-standard safety practices following Rust unsafe code guidelines

#### **Developer Experience**
- **94 comprehensive tests** covering all algorithms and edge cases
- **35 documentation tests** with working examples
- **6 integration tests** for no_std compatibility
- Extensive benchmarking suite with Criterion.rs
- Algorithm selection guide in documentation

#### **Feature Flags**
- `hashbrown` (default): Use hashbrown for better HashMap performance
- `nightly`: Enable nightly-only optimizations when available  
- `std`: Enable standard library features (opposite of no_std)

#### **Documentation & Examples**
- Complete API documentation with mathematical algorithm descriptions
- Performance characteristics and time/space complexity analysis
- When-to-use guidance for each algorithm
- Working examples: `cache_comparison.rs`, `metrics_demo.rs`
- Comprehensive README with quick start guide

### 🎯 **Use Cases**

- **Web Applications**: HTTP response caching, session storage
- **CDNs**: Size-aware content caching with GDSF
- **Databases**: Buffer pools, query result caching
- **Embedded Systems**: Resource-constrained caching with no_std
- **High-Frequency Trading**: Ultra-low latency data caching
- **Game Development**: Asset caching, world state management

### 📝 **Project Statistics**

- **~7,500 lines** of carefully crafted Rust code
- **5 cache algorithms** with distinct use cases
- **Comprehensive safety documentation** for all unsafe operations
- **MIT licensed** for maximum compatibility
- **Production-ready** architecture following Rust best practices

---

**Full Changelog**: https://github.com/sigsegved/cache-rs/commits/v0.1.0
