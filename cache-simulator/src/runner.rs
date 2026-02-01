//! Simulation runner for cache algorithms
//!
//! This module provides functionality to run simulations with multiple
//! cache algorithms in parallel on the same input data, supporting both
//! sequential and concurrent cache implementations.
//!
//! The runner uses a streaming approach to process requests, keeping memory
//! usage proportional to cache size rather than input data size.
//!
//! ## Storage Tracking
//!
//! The simulator tracks cumulative "storage" (sum of object sizes) during
//! simulation. This represents the logical data size stored in the cache,
//! not the actual memory used by the cache data structures. Memory overhead
//! is estimated separately based on entry count and average key size.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Instant;

use crate::input::LogReader;
use crate::models::{CacheAlgorithm, CacheMode, SimulationConfig, SimulationKey, SimulationResult};
use crate::stats::SimulationStats;

// Sequential cache imports
use cache_rs::GdsfCache;
use cache_rs::LfuCache;
use cache_rs::LfudaCache;
use cache_rs::LruCache;
use cache_rs::SlruCache;

// Concurrent cache imports
use cache_rs::ConcurrentGdsfCache;
use cache_rs::ConcurrentLfuCache;
use cache_rs::ConcurrentLfudaCache;
use cache_rs::ConcurrentLruCache;
use cache_rs::ConcurrentSlruCache;

// Configuration imports
use cache_rs::config::{
    ConcurrentCacheConfig, ConcurrentGdsfCacheConfig, ConcurrentLfuCacheConfig,
    ConcurrentLfudaCacheConfig, ConcurrentLruCacheConfig, ConcurrentSlruCacheConfig,
    GdsfCacheConfig, LfuCacheConfig, LfudaCacheConfig, LruCacheConfig, SlruCacheConfig,
};

// External cache for comparison
use moka::sync::Cache as MokaCache;

// Use ahash for faster hashing with Moka (same as our internal caches use)
use ahash::RandomState as AHashRandomState;

/// Default number of segments for concurrent caches
const DEFAULT_SEGMENT_COUNT: usize = 16;

/// Wrapper enum for all cache implementations
/// This allows us to handle both sequential and concurrent caches uniformly
///
/// Variants ending in `Size` use `put_with_size` for size-based eviction.
/// This avoids per-request `if` checks by encoding the mode in the variant.
#[allow(dead_code)]
enum CacheWrapper {
    // Sequential variants - entry count mode
    LruSeq(LruCache<String, u32>),
    LfuSeq(LfuCache<String, u32>),
    LfudaSeq(LfudaCache<String, u32>),
    SlruSeq(SlruCache<String, u32>),
    GdsfSeq(GdsfCache<String, u32>),
    // Sequential variants - size-based mode
    LruSeqSize(LruCache<String, u32>),
    LfuSeqSize(LfuCache<String, u32>),
    LfudaSeqSize(LfudaCache<String, u32>),
    SlruSeqSize(SlruCache<String, u32>),
    // Concurrent variants - entry count mode
    LruConc(ConcurrentLruCache<String, u32>),
    LfuConc(ConcurrentLfuCache<String, u32>),
    LfudaConc(ConcurrentLfudaCache<String, u32>),
    SlruConc(ConcurrentSlruCache<String, u32>),
    GdsfConc(ConcurrentGdsfCache<String, u32>),
    // Concurrent variants - size-based mode
    LruConcSize(ConcurrentLruCache<String, u32>),
    LfuConcSize(ConcurrentLfuCache<String, u32>),
    LfudaConcSize(ConcurrentLfudaCache<String, u32>),
    SlruConcSize(ConcurrentSlruCache<String, u32>),
    // External caches for comparison
    Moka(MokaCache<String, u32, AHashRandomState>),
}

/// Tracks storage usage during simulation
/// Uses a simple approximation: tracks total bytes added and estimates
/// current storage based on cache fill level and average object size.
#[derive(Debug, Default)]
struct StorageTracker {
    /// Total bytes added to cache (including duplicates/updates)
    total_bytes_added: usize,
    /// Number of unique items added
    items_added: usize,
    /// Peak storage observed (estimated)
    peak_bytes: usize,
}

/// Tracks latency for a single operation type
#[derive(Debug)]
struct OpLatencyTracker {
    /// Total time spent (nanoseconds)
    total_ns: u64,
    /// Number of operations
    count: u64,
    /// Minimum latency (nanoseconds)
    min_ns: u64,
    /// Maximum latency (nanoseconds)
    max_ns: u64,
    /// Sample reservoir for percentile calculation
    samples: Vec<u64>,
    /// Maximum samples to keep
    max_samples: usize,
}

impl OpLatencyTracker {
    fn new() -> Self {
        Self {
            total_ns: 0,
            count: 0,
            min_ns: u64::MAX,
            max_ns: 0,
            samples: Vec::with_capacity(5000),
            max_samples: 5000,
        }
    }

    #[inline]
    fn record(&mut self, latency_ns: u64) {
        self.total_ns += latency_ns;
        self.count += 1;
        self.min_ns = self.min_ns.min(latency_ns);
        self.max_ns = self.max_ns.max(latency_ns);

        // Reservoir sampling for percentiles
        if self.samples.len() < self.max_samples {
            self.samples.push(latency_ns);
        } else {
            let idx = (self.count as usize) % self.max_samples;
            if rand::random::<usize>() % (self.count as usize) < self.max_samples {
                self.samples[idx] = latency_ns;
            }
        }
    }

    fn percentiles(&mut self) -> crate::models::LatencyPercentiles {
        use crate::models::LatencyPercentiles;

        if self.samples.is_empty() {
            return LatencyPercentiles::default();
        }

        self.samples.sort_unstable();
        let len = self.samples.len();

        LatencyPercentiles {
            p50_ns: self.samples[len * 50 / 100],
            p90_ns: self.samples[len * 90 / 100],
            p99_ns: self.samples[len * 99 / 100],
            p999_ns: self.samples[len.saturating_sub(1).min(len * 999 / 1000)],
        }
    }

    fn finalize_op_stats(&mut self) -> crate::models::OpLatencyStats {
        crate::models::OpLatencyStats {
            total_ns: self.total_ns,
            count: self.count,
            min_ns: if self.min_ns == u64::MAX {
                0
            } else {
                self.min_ns
            },
            max_ns: self.max_ns,
            percentiles: Some(self.percentiles()),
        }
    }
}

/// Tracks latency of cache operations (get/put) excluding I/O time
#[derive(Debug)]
struct LatencyTracker {
    /// Get operation latencies
    get_tracker: OpLatencyTracker,
    /// Put operation latencies (regular put)
    put_tracker: OpLatencyTracker,
    /// Put with size operation latencies
    put_with_size_tracker: OpLatencyTracker,
}

impl LatencyTracker {
    fn new() -> Self {
        Self {
            get_tracker: OpLatencyTracker::new(),
            put_tracker: OpLatencyTracker::new(),
            put_with_size_tracker: OpLatencyTracker::new(),
        }
    }

    /// Record a get operation latency
    #[inline]
    fn record_get(&mut self, latency_ns: u64) {
        self.get_tracker.record(latency_ns);
    }

    /// Record a put operation latency
    #[inline]
    fn record_put(&mut self, latency_ns: u64) {
        self.put_tracker.record(latency_ns);
    }

    /// Record a put_with_size operation latency
    #[inline]
    fn record_put_with_size(&mut self, latency_ns: u64) {
        self.put_with_size_tracker.record(latency_ns);
    }

    /// Convert to LatencyStats (consumes internal state)
    fn finalize_stats(&mut self) -> crate::models::LatencyStats {
        use crate::models::LatencyStats;

        // Calculate combined stats
        let total_ns = self.get_tracker.total_ns
            + self.put_tracker.total_ns
            + self.put_with_size_tracker.total_ns;
        let total_count =
            self.get_tracker.count + self.put_tracker.count + self.put_with_size_tracker.count;

        LatencyStats {
            total_ns,
            count: total_count,
            get_stats: self.get_tracker.finalize_op_stats(),
            put_stats: self.put_tracker.finalize_op_stats(),
            put_with_size_stats: self.put_with_size_tracker.finalize_op_stats(),
        }
    }
}

impl StorageTracker {
    fn new() -> Self {
        Self::default()
    }

    /// Record that an item was added to the cache
    fn add(&mut self, size: usize) {
        self.total_bytes_added += size;
        self.items_added += 1;
    }

    /// Calculate current estimated storage based on cache fill level
    fn estimate_storage(&self, current_entries: usize) -> usize {
        if self.items_added == 0 {
            return 0;
        }
        // Average size of items we've added
        let avg_size = self.total_bytes_added / self.items_added;
        current_entries * avg_size
    }

    /// Update peak storage based on current cache state
    fn update_peak(&mut self, current_entries: usize) {
        let current = self.estimate_storage(current_entries);
        self.peak_bytes = self.peak_bytes.max(current);
    }

    /// Get peak storage in bytes
    fn peak(&self) -> usize {
        self.peak_bytes
    }
}

impl CacheWrapper {
    /// Attempt to get a value from the cache
    fn get(&mut self, key: &str) -> bool {
        match self {
            // Sequential - entry count mode
            CacheWrapper::LruSeq(c) => c.get(key).is_some(),
            CacheWrapper::LfuSeq(c) => c.get(key).is_some(),
            CacheWrapper::LfudaSeq(c) => c.get(key).is_some(),
            CacheWrapper::SlruSeq(c) => c.get(key).is_some(),
            CacheWrapper::GdsfSeq(c) => c.get(key).is_some(),
            // Sequential - size-based mode
            CacheWrapper::LruSeqSize(c) => c.get(key).is_some(),
            CacheWrapper::LfuSeqSize(c) => c.get(key).is_some(),
            CacheWrapper::LfudaSeqSize(c) => c.get(key).is_some(),
            CacheWrapper::SlruSeqSize(c) => c.get(key).is_some(),
            // Concurrent - entry count mode
            CacheWrapper::LruConc(c) => c.get(key).is_some(),
            CacheWrapper::LfuConc(c) => c.get(key).is_some(),
            CacheWrapper::LfudaConc(c) => c.get(key).is_some(),
            CacheWrapper::SlruConc(c) => c.get(key).is_some(),
            CacheWrapper::GdsfConc(c) => c.get(key).is_some(),
            // Concurrent - size-based mode
            CacheWrapper::LruConcSize(c) => c.get(key).is_some(),
            CacheWrapper::LfuConcSize(c) => c.get(key).is_some(),
            CacheWrapper::LfudaConcSize(c) => c.get(key).is_some(),
            CacheWrapper::SlruConcSize(c) => c.get(key).is_some(),
            // External caches
            CacheWrapper::Moka(c) => c.get(key).is_some(),
        }
    }

    /// Returns true if this cache uses put_with_size (size-based eviction)
    fn uses_size_based_put(&self) -> bool {
        matches!(
            self,
            CacheWrapper::LruSeqSize(_)
                | CacheWrapper::LfuSeqSize(_)
                | CacheWrapper::LfudaSeqSize(_)
                | CacheWrapper::SlruSeqSize(_)
                | CacheWrapper::LruConcSize(_)
                | CacheWrapper::LfuConcSize(_)
                | CacheWrapper::LfudaConcSize(_)
                | CacheWrapper::SlruConcSize(_)
                | CacheWrapper::GdsfSeq(_)
                | CacheWrapper::GdsfConc(_)
        )
    }

    /// Insert a value into the cache
    /// Size-based variants use put_with_size, entry-count variants use put
    fn put(&mut self, key: String, size: usize) {
        let safe_size = size.max(1) as u64;
        let value = size as u32;

        match self {
            // Sequential - entry count mode (no size tracking)
            CacheWrapper::LruSeq(c) => {
                c.put(key, value);
            }
            CacheWrapper::LfuSeq(c) => {
                c.put(key, value);
            }
            CacheWrapper::LfudaSeq(c) => {
                c.put(key, value);
            }
            CacheWrapper::SlruSeq(c) => {
                c.put(key, value);
            }
            CacheWrapper::GdsfSeq(c) => {
                c.put(key, value, safe_size);
            } // GDSF always uses size
            // Sequential - size-based mode
            CacheWrapper::LruSeqSize(c) => {
                c.put_with_size(key, value, safe_size);
            }
            CacheWrapper::LfuSeqSize(c) => {
                c.put_with_size(key, value, safe_size);
            }
            CacheWrapper::LfudaSeqSize(c) => {
                c.put_with_size(key, value, safe_size);
            }
            CacheWrapper::SlruSeqSize(c) => {
                c.put_with_size(key, value, safe_size);
            }
            // Concurrent - entry count mode
            CacheWrapper::LruConc(c) => {
                c.put(key, value);
            }
            CacheWrapper::LfuConc(c) => {
                c.put(key, value);
            }
            CacheWrapper::LfudaConc(c) => {
                c.put(key, value);
            }
            CacheWrapper::SlruConc(c) => {
                c.put(key, value);
            }
            CacheWrapper::GdsfConc(c) => {
                c.put(key, value, safe_size);
            } // GDSF always uses size
            // Concurrent - size-based mode
            CacheWrapper::LruConcSize(c) => {
                c.put_with_size(key, value, safe_size);
            }
            CacheWrapper::LfuConcSize(c) => {
                c.put_with_size(key, value, safe_size);
            }
            CacheWrapper::LfudaConcSize(c) => {
                c.put_with_size(key, value, safe_size);
            }
            CacheWrapper::SlruConcSize(c) => {
                c.put_with_size(key, value, safe_size);
            }
            // External caches (Moka handles size via weigher at build time)
            CacheWrapper::Moka(c) => {
                c.insert(key, value);
            }
        }
    }

    /// Get the current number of entries in the cache
    fn len(&self) -> usize {
        match self {
            // Sequential
            CacheWrapper::LruSeq(c) | CacheWrapper::LruSeqSize(c) => c.len(),
            CacheWrapper::LfuSeq(c) | CacheWrapper::LfuSeqSize(c) => c.len(),
            CacheWrapper::LfudaSeq(c) | CacheWrapper::LfudaSeqSize(c) => c.len(),
            CacheWrapper::SlruSeq(c) | CacheWrapper::SlruSeqSize(c) => c.len(),
            CacheWrapper::GdsfSeq(c) => c.len(),
            // Concurrent
            CacheWrapper::LruConc(c) | CacheWrapper::LruConcSize(c) => c.len(),
            CacheWrapper::LfuConc(c) | CacheWrapper::LfuConcSize(c) => c.len(),
            CacheWrapper::LfudaConc(c) | CacheWrapper::LfudaConcSize(c) => c.len(),
            CacheWrapper::SlruConc(c) | CacheWrapper::SlruConcSize(c) => c.len(),
            CacheWrapper::GdsfConc(c) => c.len(),
            // External caches
            CacheWrapper::Moka(c) => c.entry_count() as usize,
        }
    }
}

/// Cache factory for creating different cache types
struct CacheFactory;

impl CacheFactory {
    /// Create a new cache instance based on algorithm, mode, capacity, max_size, and use_size flag
    /// - capacity: maximum number of entries (always used)
    /// - max_size: maximum size in bytes (only used when use_size is true)
    fn create_cache(
        algorithm: CacheAlgorithm,
        mode: CacheMode,
        capacity: usize,
        max_size_bytes: u64,
        segment_count: Option<usize>,
        use_size: bool,
    ) -> CacheWrapper {
        let cap_nz = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap());
        let max_size = if use_size { max_size_bytes } else { u64::MAX };
        let segments = segment_count.unwrap_or(DEFAULT_SEGMENT_COUNT);

        match (algorithm, mode, use_size) {
            // Sequential caches - entry count mode
            (CacheAlgorithm::Lru, CacheMode::Sequential, false) => {
                let config = LruCacheConfig {
                    capacity: cap_nz,
                    max_size,
                };
                CacheWrapper::LruSeq(LruCache::init(config, None))
            }
            (CacheAlgorithm::Slru, CacheMode::Sequential, false) => {
                let protected = NonZeroUsize::new(cap_nz.get() * 2 / 3)
                    .unwrap_or(NonZeroUsize::new(1).unwrap());
                let config = SlruCacheConfig {
                    capacity: cap_nz,
                    protected_capacity: protected,
                    max_size,
                };
                CacheWrapper::SlruSeq(SlruCache::init(config, None))
            }
            (CacheAlgorithm::Lfu, CacheMode::Sequential, false) => {
                let config = LfuCacheConfig {
                    capacity: cap_nz,
                    max_size,
                };
                CacheWrapper::LfuSeq(LfuCache::init(config, None))
            }
            (CacheAlgorithm::Lfuda, CacheMode::Sequential, false) => {
                let config = LfudaCacheConfig {
                    capacity: cap_nz,
                    initial_age: 0,
                    max_size,
                };
                CacheWrapper::LfudaSeq(LfudaCache::init(config, None))
            }
            (CacheAlgorithm::Gdsf, CacheMode::Sequential, _) => {
                // GDSF always uses size internally
                let config = GdsfCacheConfig {
                    capacity: cap_nz,
                    initial_age: 0.0,
                    max_size,
                };
                CacheWrapper::GdsfSeq(GdsfCache::init(config, None))
            }
            // Sequential caches - size-based mode
            (CacheAlgorithm::Lru, CacheMode::Sequential, true) => {
                let config = LruCacheConfig {
                    capacity: cap_nz,
                    max_size,
                };
                CacheWrapper::LruSeqSize(LruCache::init(config, None))
            }
            (CacheAlgorithm::Slru, CacheMode::Sequential, true) => {
                let protected = NonZeroUsize::new(cap_nz.get() * 2 / 3)
                    .unwrap_or(NonZeroUsize::new(1).unwrap());
                let config = SlruCacheConfig {
                    capacity: cap_nz,
                    protected_capacity: protected,
                    max_size,
                };
                CacheWrapper::SlruSeqSize(SlruCache::init(config, None))
            }
            (CacheAlgorithm::Lfu, CacheMode::Sequential, true) => {
                let config = LfuCacheConfig {
                    capacity: cap_nz,
                    max_size,
                };
                CacheWrapper::LfuSeqSize(LfuCache::init(config, None))
            }
            (CacheAlgorithm::Lfuda, CacheMode::Sequential, true) => {
                let config = LfudaCacheConfig {
                    capacity: cap_nz,
                    initial_age: 0,
                    max_size,
                };
                CacheWrapper::LfudaSeqSize(LfudaCache::init(config, None))
            }
            // Concurrent caches - entry count mode
            (CacheAlgorithm::Lru, CacheMode::Concurrent, false) => {
                let config: ConcurrentLruCacheConfig = ConcurrentCacheConfig {
                    base: LruCacheConfig {
                        capacity: cap_nz,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::LruConc(ConcurrentLruCache::init(config, None))
            }
            (CacheAlgorithm::Slru, CacheMode::Concurrent, false) => {
                let protected = NonZeroUsize::new(cap_nz.get() * 2 / 3)
                    .unwrap_or(NonZeroUsize::new(1).unwrap());
                let config: ConcurrentSlruCacheConfig = ConcurrentCacheConfig {
                    base: SlruCacheConfig {
                        capacity: cap_nz,
                        protected_capacity: protected,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::SlruConc(ConcurrentSlruCache::init(config, None))
            }
            (CacheAlgorithm::Lfu, CacheMode::Concurrent, false) => {
                let config: ConcurrentLfuCacheConfig = ConcurrentCacheConfig {
                    base: LfuCacheConfig {
                        capacity: cap_nz,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::LfuConc(ConcurrentLfuCache::init(config, None))
            }
            (CacheAlgorithm::Lfuda, CacheMode::Concurrent, false) => {
                let config: ConcurrentLfudaCacheConfig = ConcurrentCacheConfig {
                    base: LfudaCacheConfig {
                        capacity: cap_nz,
                        initial_age: 0,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::LfudaConc(ConcurrentLfudaCache::init(config, None))
            }
            (CacheAlgorithm::Gdsf, CacheMode::Concurrent, _) => {
                // GDSF always uses size internally
                let config: ConcurrentGdsfCacheConfig = ConcurrentCacheConfig {
                    base: GdsfCacheConfig {
                        capacity: cap_nz,
                        initial_age: 0.0,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::GdsfConc(ConcurrentGdsfCache::init(config, None))
            }
            // Concurrent caches - size-based mode
            (CacheAlgorithm::Lru, CacheMode::Concurrent, true) => {
                let config: ConcurrentLruCacheConfig = ConcurrentCacheConfig {
                    base: LruCacheConfig {
                        capacity: cap_nz,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::LruConcSize(ConcurrentLruCache::init(config, None))
            }
            (CacheAlgorithm::Slru, CacheMode::Concurrent, true) => {
                let protected = NonZeroUsize::new(cap_nz.get() * 2 / 3)
                    .unwrap_or(NonZeroUsize::new(1).unwrap());
                let config: ConcurrentSlruCacheConfig = ConcurrentCacheConfig {
                    base: SlruCacheConfig {
                        capacity: cap_nz,
                        protected_capacity: protected,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::SlruConcSize(ConcurrentSlruCache::init(config, None))
            }
            (CacheAlgorithm::Lfu, CacheMode::Concurrent, true) => {
                let config: ConcurrentLfuCacheConfig = ConcurrentCacheConfig {
                    base: LfuCacheConfig {
                        capacity: cap_nz,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::LfuConcSize(ConcurrentLfuCache::init(config, None))
            }
            (CacheAlgorithm::Lfuda, CacheMode::Concurrent, true) => {
                let config: ConcurrentLfudaCacheConfig = ConcurrentCacheConfig {
                    base: LfudaCacheConfig {
                        capacity: cap_nz,
                        initial_age: 0,
                        max_size,
                    },
                    segments,
                };
                CacheWrapper::LfudaConcSize(ConcurrentLfudaCache::init(config, None))
            }
            // Moka cache - mode is encoded at build time via weigher
            (CacheAlgorithm::Moka, _, use_size) => {
                if use_size {
                    let cache = MokaCache::builder()
                        .max_capacity(max_size_bytes)
                        .weigher(|_key: &String, &value: &u32| value)
                        .build_with_hasher(AHashRandomState::default());
                    CacheWrapper::Moka(cache)
                } else {
                    let cache = MokaCache::builder()
                        .max_capacity(capacity as u64)
                        .initial_capacity(capacity)
                        .build_with_hasher(AHashRandomState::default());
                    CacheWrapper::Moka(cache)
                }
            }
        }
    }
}

/// Runner for cache simulations
pub struct SimulationRunner {
    config: SimulationConfig,
}

impl SimulationRunner {
    /// Create a new simulation runner
    pub fn new(config: SimulationConfig) -> Self {
        Self { config }
    }

    /// Run the simulation using streaming to minimize memory usage.
    /// Only the cache data structures consume significant memory.
    ///
    /// Storage tracking:
    /// - Tracks cumulative object size as items are added to cache
    /// - Reports peak and final storage usage
    /// - Estimates memory overhead based on entry count and key sizes
    pub fn run(&self) -> Result<SimulationResult, String> {
        let log_reader = LogReader::new(&self.config.input_dir);

        // First pass: gather statistics about the dataset (streaming)
        println!("Scanning dataset for statistics...");
        let scan_start = Instant::now();

        let mut total_requests: usize = 0;
        let mut total_key_bytes: usize = 0;
        let mut unique_objects: HashMap<String, (usize, u64)> = HashMap::new();

        {
            let mut request_iter = match log_reader.stream_requests() {
                Ok(iter) => iter,
                Err(err) => return Err(format!("Failed to open log files: {err:?}")),
            };

            for result in &mut request_iter {
                let request = match result {
                    Ok(req) => req,
                    Err(err) => return Err(format!("Failed to parse request: {err:?}")),
                };

                total_requests += 1;
                total_key_bytes += request.key.len();
                unique_objects
                    .entry(request.key)
                    .and_modify(|(_, count)| {
                        *count += 1;
                    })
                    .or_insert((request.size, 1));

                // Progress indicator every 10M requests
                if total_requests % 10_000_000 == 0 {
                    println!(
                        "  Scanned {} million requests...",
                        total_requests / 1_000_000
                    );
                }
            }
        }

        let scan_duration = scan_start.elapsed();
        println!("Scan completed in {:.2?}", scan_duration);

        if total_requests == 0 {
            return Err("No requests found in log files".to_string());
        }

        // Calculate statistics
        let avg_object_size = if !unique_objects.is_empty() {
            unique_objects
                .values()
                .map(|(size, _)| *size)
                .sum::<usize>()
                / unique_objects.len()
        } else {
            1024
        };

        let avg_key_size = total_key_bytes / total_requests;

        let avg_requests_per_object = if !unique_objects.is_empty() {
            total_requests as f64 / unique_objects.len() as f64
        } else {
            1.0
        };

        let total_unique_size: usize = unique_objects.values().map(|(size, _)| *size).sum();
        let unique_objects_count = unique_objects.len();

        // Cache configuration:
        // - capacity: always used for entry count limit
        // - max_size: only used when use_size is true for size-based eviction
        let cache_capacity = self.config.capacity;
        let cache_max_size = self.config.max_size;

        // Print dataset statistics
        println!("\nDataset statistics:");
        println!("  Total requests: {}", total_requests);
        println!("  Unique objects: {}", unique_objects_count);
        println!("  Avg requests per object: {avg_requests_per_object:.2}");
        println!("  Avg object size: {avg_object_size} bytes");
        println!("  Avg key size: {avg_key_size} bytes");
        println!(
            "  Total unique objects size: {} bytes ({:.2} MB)",
            total_unique_size,
            total_unique_size as f64 / (1024.0 * 1024.0)
        );

        // Drop unique_objects to free memory before creating caches
        drop(unique_objects);

        println!("\nSimulation configuration:");
        println!("  Cache capacity: {} entries", cache_capacity);
        if self.config.use_size {
            println!(
                "  Max size: {} bytes ({:.2} MB)",
                cache_max_size,
                cache_max_size as f64 / (1024.0 * 1024.0)
            );
            println!("  Size-based eviction: ENABLED");
        } else {
            println!(
                "  Est. max storage: ~{:.2} MB (capacity × avg object size)",
                (cache_capacity * avg_object_size) as f64 / (1024.0 * 1024.0)
            );
        }
        if self.config.modes.contains(&CacheMode::Concurrent) {
            println!(
                "  Concurrent segments: {}",
                self.config.segment_count.unwrap_or(DEFAULT_SEGMENT_COUNT)
            );
        }
        println!(
            "  Modes: {:?}",
            self.config
                .modes
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>()
        );

        // Set up statistics
        let mut stats = SimulationStats::new(&self.config.algorithms, &self.config.modes);

        // Start timing
        let start_time = Instant::now();

        // Process each algorithm+mode combination separately with streaming
        // This way we only hold one cache in memory at a time (plus a small read buffer)
        for &algo in &self.config.algorithms {
            for &mode in &self.config.modes {
                let key = SimulationKey::new(algo, mode);

                println!("\nRunning {}-{}...", algo.as_str(), mode.as_str());

                // Create cache for this algorithm+mode
                let mut cache = CacheFactory::create_cache(
                    algo,
                    mode,
                    cache_capacity,
                    cache_max_size,
                    self.config.segment_count,
                    self.config.use_size,
                );

                // Track storage usage during simulation
                let mut storage_tracker = StorageTracker::new();

                // Track latency of cache operations (separate from I/O)
                let mut latency_tracker = LatencyTracker::new();

                // Check if this cache uses put_with_size
                let uses_size_put = cache.uses_size_based_put();

                // Stream through all requests
                let mut request_iter = match log_reader.stream_requests() {
                    Ok(iter) => iter,
                    Err(err) => return Err(format!("Failed to open log files: {err:?}")),
                };

                let algo_start = Instant::now();
                let mut processed = 0usize;

                for result in &mut request_iter {
                    let request = match result {
                        Ok(req) => req,
                        Err(err) => return Err(format!("Failed to parse request: {err:?}")),
                    };

                    // Measure get operation separately
                    let get_start = Instant::now();
                    let hit = cache.get(&request.key);
                    let get_duration = get_start.elapsed().as_nanos() as u64;
                    latency_tracker.record_get(get_duration);

                    // On miss, measure put operation separately
                    if !hit {
                        let put_start = Instant::now();
                        cache.put(request.key.clone(), request.size);
                        let put_duration = put_start.elapsed().as_nanos() as u64;

                        // Record to appropriate tracker based on put type
                        if uses_size_put {
                            latency_tracker.record_put_with_size(put_duration);
                        } else {
                            latency_tracker.record_put(put_duration);
                        }

                        stats.record_miss(key, request.size);
                        storage_tracker.add(request.size);
                    } else {
                        stats.record_hit(key, request.size);
                    }

                    // Update peak storage periodically
                    storage_tracker.update_peak(cache.len());

                    processed += 1;
                    if processed % 50_000_000 == 0 {
                        let elapsed = algo_start.elapsed();
                        let rate = processed as f64 / elapsed.as_secs_f64();
                        let current_storage = storage_tracker.estimate_storage(cache.len());
                        println!(
                            "  Processed {} million requests ({:.0} req/s), storage: {:.2} MB...",
                            processed / 1_000_000,
                            rate,
                            current_storage as f64 / (1024.0 * 1024.0)
                        );
                    }
                }

                let algo_duration = algo_start.elapsed();
                let final_entry_count = cache.len();

                // Final storage estimate
                let final_storage = storage_tracker.estimate_storage(final_entry_count);
                storage_tracker.update_peak(final_entry_count);

                // Estimate memory overhead:
                // - HashMap overhead: ~48 bytes per entry (bucket + metadata)
                // - String key: ~24 bytes (String struct) + key length
                // - Value storage: ~4 bytes (u32)
                // - List node (for LRU/LFU): ~48 bytes (prev/next pointers + data)
                // Total estimate: ~120 bytes + avg_key_size per entry
                let memory_overhead_per_entry = 120 + avg_key_size;
                let estimated_memory = final_entry_count * memory_overhead_per_entry;

                // Get latency stats
                let latency_stats = latency_tracker.finalize_stats();

                stats.record_time(key, algo_duration.as_millis() as u64);
                stats.record_storage(key, storage_tracker.peak(), final_storage, estimated_memory);
                stats.record_latency(key, latency_stats.clone());

                // Print wall-clock time (includes I/O)
                println!(
                    "  Wall time: {:.2?} ({:.0} req/s including I/O)",
                    algo_duration,
                    processed as f64 / algo_duration.as_secs_f64()
                );

                // Print combined cache operation stats
                println!(
                    "  Total ops: {} in {:.3}s = {:.0} ops/s (avg {:.0}ns)",
                    latency_stats.count,
                    latency_stats.duration_secs(),
                    latency_stats.ops_per_sec(),
                    latency_stats.avg_ns()
                );

                // Print GET stats
                let get = &latency_stats.get_stats;
                let get_pct = get.percentiles.as_ref();
                println!(
                    "    GET:  {} ops in {:.3}s = {:.0} ops/s | avg={:.0}ns min={} max={} p50={} p99={}",
                    get.count,
                    get.duration_secs(),
                    get.ops_per_sec(),
                    get.avg_ns(),
                    get.min_ns,
                    get.max_ns,
                    get_pct.map(|p| p.p50_ns).unwrap_or(0),
                    get_pct.map(|p| p.p99_ns).unwrap_or(0)
                );

                // Print PUT stats (either put or put_with_size, whichever was used)
                let put = &latency_stats.put_stats;
                let put_size = &latency_stats.put_with_size_stats;

                if put.count > 0 {
                    let put_pct = put.percentiles.as_ref();
                    println!(
                        "    PUT:  {} ops in {:.3}s = {:.0} ops/s | avg={:.0}ns min={} max={} p50={} p99={}",
                        put.count,
                        put.duration_secs(),
                        put.ops_per_sec(),
                        put.avg_ns(),
                        put.min_ns,
                        put.max_ns,
                        put_pct.map(|p| p.p50_ns).unwrap_or(0),
                        put_pct.map(|p| p.p99_ns).unwrap_or(0)
                    );
                }

                if put_size.count > 0 {
                    let put_size_pct = put_size.percentiles.as_ref();
                    println!(
                        "    PUT+SIZE: {} ops in {:.3}s = {:.0} ops/s | avg={:.0}ns min={} max={} p50={} p99={}",
                        put_size.count,
                        put_size.duration_secs(),
                        put_size.ops_per_sec(),
                        put_size.avg_ns(),
                        put_size.min_ns,
                        put_size.max_ns,
                        put_size_pct.map(|p| p.p50_ns).unwrap_or(0),
                        put_size_pct.map(|p| p.p99_ns).unwrap_or(0)
                    );
                }

                println!(
                    "  Storage: peak={:.2} MB, final={:.2} MB, est. memory={:.2} MB",
                    storage_tracker.peak() as f64 / (1024.0 * 1024.0),
                    final_storage as f64 / (1024.0 * 1024.0),
                    estimated_memory as f64 / (1024.0 * 1024.0)
                );

                // Cache is dropped here, freeing memory before next iteration
            }
        }

        let duration = start_time.elapsed();

        // Print summary
        stats.print_summary();

        if self.config.modes.len() > 1 {
            stats.print_comparison();
        }

        Ok(stats.result(duration, unique_objects_count, cache_capacity))
    }
}
