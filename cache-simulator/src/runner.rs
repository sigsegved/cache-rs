//! Simulation runner for cache algorithms
//!
//! This module provides functionality to run simulations with multiple
//! cache algorithms in parallel on the same input data, supporting both
//! sequential and concurrent cache implementations.
//!
//! The runner uses a streaming approach to process requests, keeping memory
//! usage proportional to cache size rather than input data size.

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

// External cache for comparison
use moka::sync::Cache as MokaCache;

// Use ahash for faster hashing with Moka (same as our internal caches use)
use ahash::RandomState as AHashRandomState;

/// Default number of segments for concurrent caches
const DEFAULT_SEGMENT_COUNT: usize = 16;

/// Wrapper enum for all cache implementations
/// This allows us to handle both sequential and concurrent caches uniformly
enum CacheWrapper {
    // Sequential variants
    LruSeq(LruCache<String, u32>),
    LfuSeq(LfuCache<String, u32>),
    LfudaSeq(LfudaCache<String, u32>),
    SlruSeq(SlruCache<String, u32>),
    GdsfSeq(GdsfCache<String, u32>),
    // Concurrent variants
    LruConc(ConcurrentLruCache<String, u32>),
    LfuConc(ConcurrentLfuCache<String, u32>),
    LfudaConc(ConcurrentLfudaCache<String, u32>),
    SlruConc(ConcurrentSlruCache<String, u32>),
    GdsfConc(ConcurrentGdsfCache<String, u32>),
    // External caches for comparison
    /// Moka cache - uses TinyLFU admission policy with LRU eviction
    /// Configured with AHash for faster hashing and optimal initial capacity
    Moka(MokaCache<String, u32, AHashRandomState>),
}

impl CacheWrapper {
    /// Attempt to get a value from the cache
    fn get(&mut self, key: &str) -> bool {
        match self {
            // Sequential - take &mut self
            CacheWrapper::LruSeq(c) => c.get(&key.to_string()).is_some(),
            CacheWrapper::LfuSeq(c) => c.get(&key.to_string()).is_some(),
            CacheWrapper::LfudaSeq(c) => c.get(&key.to_string()).is_some(),
            CacheWrapper::SlruSeq(c) => c.get(&key.to_string()).is_some(),
            CacheWrapper::GdsfSeq(c) => c.get(&key.to_string()).is_some(),
            // Concurrent - take &self (but we have &mut self which is fine)
            CacheWrapper::LruConc(c) => c.get(&key.to_string()).is_some(),
            CacheWrapper::LfuConc(c) => c.get(&key.to_string()).is_some(),
            CacheWrapper::LfudaConc(c) => c.get(&key.to_string()).is_some(),
            CacheWrapper::SlruConc(c) => c.get(&key.to_string()).is_some(),
            CacheWrapper::GdsfConc(c) => c.get(&key.to_string()).is_some(),
            // External caches
            CacheWrapper::Moka(c) => c.get(&key.to_string()).is_some(),
        }
    }

    /// Insert a value into the cache
    fn put(&mut self, key: String, value: u32, size: usize) {
        match self {
            // Sequential
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
                let safe_size = size.max(1) as u64;
                c.put(key, value, safe_size);
            }
            // Concurrent
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
                let safe_size = size.max(1) as u64;
                c.put(key, value, safe_size);
            }
            // External caches
            CacheWrapper::Moka(c) => {
                c.insert(key, value);
            }
        }
    }
}

/// Cache factory for creating different cache types
struct CacheFactory;

impl CacheFactory {
    /// Create a new cache instance based on algorithm, mode, and capacity
    fn create_cache(
        algorithm: CacheAlgorithm,
        mode: CacheMode,
        capacity: usize,
        segment_count: Option<usize>,
    ) -> CacheWrapper {
        let capacity = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap());
        let segments = segment_count.unwrap_or(DEFAULT_SEGMENT_COUNT);

        match (algorithm, mode) {
            // Sequential caches
            (CacheAlgorithm::Lru, CacheMode::Sequential) => {
                CacheWrapper::LruSeq(LruCache::new(capacity))
            }
            (CacheAlgorithm::Slru, CacheMode::Sequential) => {
                let protected_capacity = NonZeroUsize::new(capacity.get() * 2 / 3)
                    .unwrap_or(NonZeroUsize::new(1).unwrap());
                CacheWrapper::SlruSeq(SlruCache::new(capacity, protected_capacity))
            }
            (CacheAlgorithm::Lfu, CacheMode::Sequential) => {
                CacheWrapper::LfuSeq(LfuCache::new(capacity))
            }
            (CacheAlgorithm::Lfuda, CacheMode::Sequential) => {
                CacheWrapper::LfudaSeq(LfudaCache::new(capacity))
            }
            (CacheAlgorithm::Gdsf, CacheMode::Sequential) => {
                CacheWrapper::GdsfSeq(GdsfCache::new(capacity))
            }
            // Concurrent caches
            (CacheAlgorithm::Lru, CacheMode::Concurrent) => {
                CacheWrapper::LruConc(ConcurrentLruCache::with_segments(capacity, segments))
            }
            (CacheAlgorithm::Slru, CacheMode::Concurrent) => {
                let protected_capacity = NonZeroUsize::new(capacity.get() * 2 / 3)
                    .unwrap_or(NonZeroUsize::new(1).unwrap());
                CacheWrapper::SlruConc(ConcurrentSlruCache::with_segments(
                    capacity,
                    protected_capacity,
                    segments,
                ))
            }
            (CacheAlgorithm::Lfu, CacheMode::Concurrent) => {
                CacheWrapper::LfuConc(ConcurrentLfuCache::with_segments(capacity, segments))
            }
            (CacheAlgorithm::Lfuda, CacheMode::Concurrent) => {
                CacheWrapper::LfudaConc(ConcurrentLfudaCache::with_segments(capacity, segments))
            }
            (CacheAlgorithm::Gdsf, CacheMode::Concurrent) => {
                CacheWrapper::GdsfConc(ConcurrentGdsfCache::with_segments(capacity, segments))
            }
            // Moka cache - optimally configured for fair comparison:
            // - Uses AHash for faster hashing (similar to our hashbrown-based caches)
            // - Pre-allocates initial capacity to avoid rehashing during warmup
            // - Uses default TinyLFU eviction policy (Moka's strength)
            (CacheAlgorithm::Moka, CacheMode::Sequential | CacheMode::Concurrent) => {
                let cap = capacity.get() as u64;
                let cache = MokaCache::builder()
                    .max_capacity(cap)
                    .initial_capacity(capacity.get())
                    .build_with_hasher(AHashRandomState::default());
                CacheWrapper::Moka(cache)
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
    pub fn run(&self) -> Result<SimulationResult, String> {
        let log_reader = LogReader::new(&self.config.input_dir);

        // First pass: gather statistics about the dataset (streaming)
        println!("Scanning dataset for statistics...");
        let scan_start = Instant::now();

        let mut total_requests: usize = 0;
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

        let avg_requests_per_object = if !unique_objects.is_empty() {
            total_requests as f64 / unique_objects.len() as f64
        } else {
            1.0
        };

        let total_unique_size: usize = unique_objects.values().map(|(size, _)| *size).sum();

        const BYTES_PER_CACHE_ENTRY: usize = 64;
        let memory_capacity = self.config.memory_size / BYTES_PER_CACHE_ENTRY;

        // Calculate disk capacity
        let mut sorted_objects: Vec<(usize, u64)> = unique_objects.values().copied().collect();
        sorted_objects.sort_by(|a, b| a.0.cmp(&b.0));

        let mut used_disk = 0;
        let mut disk_capacity = 0;
        let disk_size_bytes = self.config.disk_size;

        for (size, _) in &sorted_objects {
            if used_disk + size <= disk_size_bytes {
                used_disk += size;
                disk_capacity += 1;
            } else {
                break;
            }
        }

        // Drop sorted_objects to free memory before simulation
        drop(sorted_objects);

        let calculated_capacity = if disk_size_bytes < usize::MAX {
            std::cmp::min(memory_capacity, disk_capacity)
        } else {
            memory_capacity
        };

        let capacity = self.config.capacity_override.unwrap_or(calculated_capacity);

        // Print dataset statistics
        println!("\nDataset statistics:");
        println!("  Total requests: {}", total_requests);
        println!("  Unique objects: {}", unique_objects.len());
        println!("  Avg requests per object: {avg_requests_per_object:.2}");
        println!("  Avg object size: {avg_object_size} bytes");
        println!(
            "  Total unique objects size: {} bytes ({:.2} MB)",
            total_unique_size,
            total_unique_size as f64 / (1024.0 * 1024.0)
        );
        println!(
            "  Memory capacity: {} bytes ({} MB)",
            self.config.memory_size,
            self.config.memory_size / (1024 * 1024)
        );
        println!(
            "  Disk capacity: {} bytes ({} MB)",
            self.config.disk_size,
            self.config.disk_size / (1024 * 1024)
        );
        println!("  Estimated bytes per cache entry: {BYTES_PER_CACHE_ENTRY} bytes");
        println!("  Max objects in memory: {memory_capacity}");
        println!(
            "  Max objects on disk: {}",
            if disk_size_bytes < usize::MAX {
                disk_capacity
            } else {
                unique_objects.len()
            }
        );

        let unique_objects_count = unique_objects.len();

        // Drop unique_objects to free memory before creating caches
        drop(unique_objects);

        if self.config.capacity_override.is_some() {
            println!(
                "Simulating with OVERRIDE capacity: {capacity} objects (calculated was {calculated_capacity})"
            );
        } else {
            println!("Simulating with capacity: {capacity} objects");
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
                let mut cache =
                    CacheFactory::create_cache(algo, mode, capacity, self.config.segment_count);

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

                    // Try to get from cache
                    if cache.get(&request.key) {
                        stats.record_hit(key, request.size);
                    } else {
                        stats.record_miss(key, request.size);
                        cache.put(request.key, 1, request.size);
                    }

                    processed += 1;
                    if processed % 50_000_000 == 0 {
                        let elapsed = algo_start.elapsed();
                        let rate = processed as f64 / elapsed.as_secs_f64();
                        println!(
                            "  Processed {} million requests ({:.0} req/s)...",
                            processed / 1_000_000,
                            rate
                        );
                    }
                }

                let algo_duration = algo_start.elapsed();
                stats.record_time(key, algo_duration.as_millis() as u64);

                println!(
                    "  Completed in {:.2?} ({:.0} req/s)",
                    algo_duration,
                    processed as f64 / algo_duration.as_secs_f64()
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

        Ok(stats.result(duration, unique_objects_count))
    }
}
