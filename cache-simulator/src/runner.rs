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

// External cache for comparison
use moka::sync::Cache as MokaCache;

// Use ahash for faster hashing with Moka (same as our internal caches use)
use ahash::RandomState as AHashRandomState;

/// Default number of segments for concurrent caches
const DEFAULT_SEGMENT_COUNT: usize = 16;

/// Wrapper enum for all cache implementations
/// This allows us to handle both sequential and concurrent caches uniformly
///
/// The second field in each variant stores the capacity (entry count limit).
/// This is used for storage estimation during simulation.
#[allow(dead_code)]
enum CacheWrapper {
    // Sequential variants
    LruSeq(LruCache<String, u32>, usize),
    LfuSeq(LfuCache<String, u32>, usize),
    LfudaSeq(LfudaCache<String, u32>, usize),
    SlruSeq(SlruCache<String, u32>, usize),
    GdsfSeq(GdsfCache<String, u32>, usize),
    // Concurrent variants
    LruConc(ConcurrentLruCache<String, u32>, usize),
    LfuConc(ConcurrentLfuCache<String, u32>, usize),
    LfudaConc(ConcurrentLfudaCache<String, u32>, usize),
    SlruConc(ConcurrentSlruCache<String, u32>, usize),
    GdsfConc(ConcurrentGdsfCache<String, u32>, usize),
    // External caches for comparison
    /// Moka cache - uses TinyLFU admission policy with LRU eviction
    /// Configured with AHash for faster hashing and optimal initial capacity
    Moka(MokaCache<String, u32, AHashRandomState>, usize),
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
            // Sequential - take &mut self
            CacheWrapper::LruSeq(c, _) => c.get(&key.to_string()).is_some(),
            CacheWrapper::LfuSeq(c, _) => c.get(&key.to_string()).is_some(),
            CacheWrapper::LfudaSeq(c, _) => c.get(&key.to_string()).is_some(),
            CacheWrapper::SlruSeq(c, _) => c.get(&key.to_string()).is_some(),
            CacheWrapper::GdsfSeq(c, _) => c.get(&key.to_string()).is_some(),
            // Concurrent - take &self (but we have &mut self which is fine)
            CacheWrapper::LruConc(c, _) => c.get(&key.to_string()).is_some(),
            CacheWrapper::LfuConc(c, _) => c.get(&key.to_string()).is_some(),
            CacheWrapper::LfudaConc(c, _) => c.get(&key.to_string()).is_some(),
            CacheWrapper::SlruConc(c, _) => c.get(&key.to_string()).is_some(),
            CacheWrapper::GdsfConc(c, _) => c.get(&key.to_string()).is_some(),
            // External caches
            CacheWrapper::Moka(c, _) => c.get(&key.to_string()).is_some(),
        }
    }

    /// Insert a value into the cache
    fn put(&mut self, key: String, value: u32, size: usize) {
        match self {
            // Sequential
            CacheWrapper::LruSeq(c, _) => {
                c.put(key, value);
            }
            CacheWrapper::LfuSeq(c, _) => {
                c.put(key, value);
            }
            CacheWrapper::LfudaSeq(c, _) => {
                c.put(key, value);
            }
            CacheWrapper::SlruSeq(c, _) => {
                c.put(key, value);
            }
            CacheWrapper::GdsfSeq(c, _) => {
                let safe_size = size.max(1) as u64;
                c.put(key, value, safe_size);
            }
            // Concurrent
            CacheWrapper::LruConc(c, _) => {
                c.put(key, value);
            }
            CacheWrapper::LfuConc(c, _) => {
                c.put(key, value);
            }
            CacheWrapper::LfudaConc(c, _) => {
                c.put(key, value);
            }
            CacheWrapper::SlruConc(c, _) => {
                c.put(key, value);
            }
            CacheWrapper::GdsfConc(c, _) => {
                let safe_size = size.max(1) as u64;
                c.put(key, value, safe_size);
            }
            // External caches
            CacheWrapper::Moka(c, _) => {
                c.insert(key, value);
            }
        }
    }

    /// Get the current number of entries in the cache
    fn len(&self) -> usize {
        match self {
            // Sequential
            CacheWrapper::LruSeq(c, _) => c.len(),
            CacheWrapper::LfuSeq(c, _) => c.len(),
            CacheWrapper::LfudaSeq(c, _) => c.len(),
            CacheWrapper::SlruSeq(c, _) => c.len(),
            CacheWrapper::GdsfSeq(c, _) => c.len(),
            // Concurrent
            CacheWrapper::LruConc(c, _) => c.len(),
            CacheWrapper::LfuConc(c, _) => c.len(),
            CacheWrapper::LfudaConc(c, _) => c.len(),
            CacheWrapper::SlruConc(c, _) => c.len(),
            CacheWrapper::GdsfConc(c, _) => c.len(),
            // External caches
            CacheWrapper::Moka(c, _) => c.entry_count() as usize,
        }
    }

    /// Get the capacity of the cache
    fn capacity(&self) -> usize {
        match self {
            // Sequential
            CacheWrapper::LruSeq(_, cap) => *cap,
            CacheWrapper::LfuSeq(_, cap) => *cap,
            CacheWrapper::LfudaSeq(_, cap) => *cap,
            CacheWrapper::SlruSeq(_, cap) => *cap,
            CacheWrapper::GdsfSeq(_, cap) => *cap,
            // Concurrent
            CacheWrapper::LruConc(_, cap) => *cap,
            CacheWrapper::LfuConc(_, cap) => *cap,
            CacheWrapper::LfudaConc(_, cap) => *cap,
            CacheWrapper::SlruConc(_, cap) => *cap,
            CacheWrapper::GdsfConc(_, cap) => *cap,
            // External caches
            CacheWrapper::Moka(_, cap) => *cap,
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
        let cap_nz = NonZeroUsize::new(capacity).unwrap_or(NonZeroUsize::new(1).unwrap());
        let segments = segment_count.unwrap_or(DEFAULT_SEGMENT_COUNT);

        match (algorithm, mode) {
            // Sequential caches
            (CacheAlgorithm::Lru, CacheMode::Sequential) => {
                CacheWrapper::LruSeq(LruCache::new(cap_nz), capacity)
            }
            (CacheAlgorithm::Slru, CacheMode::Sequential) => {
                let protected_capacity = NonZeroUsize::new(cap_nz.get() * 2 / 3)
                    .unwrap_or(NonZeroUsize::new(1).unwrap());
                CacheWrapper::SlruSeq(SlruCache::new(cap_nz, protected_capacity), capacity)
            }
            (CacheAlgorithm::Lfu, CacheMode::Sequential) => {
                CacheWrapper::LfuSeq(LfuCache::new(cap_nz), capacity)
            }
            (CacheAlgorithm::Lfuda, CacheMode::Sequential) => {
                CacheWrapper::LfudaSeq(LfudaCache::new(cap_nz), capacity)
            }
            (CacheAlgorithm::Gdsf, CacheMode::Sequential) => {
                CacheWrapper::GdsfSeq(GdsfCache::new(cap_nz), capacity)
            }
            // Concurrent caches
            (CacheAlgorithm::Lru, CacheMode::Concurrent) => CacheWrapper::LruConc(
                ConcurrentLruCache::with_segments(cap_nz, segments),
                capacity,
            ),
            (CacheAlgorithm::Slru, CacheMode::Concurrent) => {
                let protected_capacity = NonZeroUsize::new(cap_nz.get() * 2 / 3)
                    .unwrap_or(NonZeroUsize::new(1).unwrap());
                CacheWrapper::SlruConc(
                    ConcurrentSlruCache::with_segments(cap_nz, protected_capacity, segments),
                    capacity,
                )
            }
            (CacheAlgorithm::Lfu, CacheMode::Concurrent) => CacheWrapper::LfuConc(
                ConcurrentLfuCache::with_segments(cap_nz, segments),
                capacity,
            ),
            (CacheAlgorithm::Lfuda, CacheMode::Concurrent) => CacheWrapper::LfudaConc(
                ConcurrentLfudaCache::with_segments(cap_nz, segments),
                capacity,
            ),
            (CacheAlgorithm::Gdsf, CacheMode::Concurrent) => CacheWrapper::GdsfConc(
                ConcurrentGdsfCache::with_segments(cap_nz, segments),
                capacity,
            ),
            // Moka cache - optimally configured for fair comparison:
            // - Uses AHash for faster hashing (similar to our hashbrown-based caches)
            // - Pre-allocates initial capacity to avoid rehashing during warmup
            // - Uses default TinyLFU eviction policy (Moka's strength)
            (CacheAlgorithm::Moka, CacheMode::Sequential | CacheMode::Concurrent) => {
                let cap = cap_nz.get() as u64;
                let cache = MokaCache::builder()
                    .max_capacity(cap)
                    .initial_capacity(cap_nz.get())
                    .build_with_hasher(AHashRandomState::default());
                CacheWrapper::Moka(cache, capacity)
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
        let capacity = self.config.capacity;

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
        println!("  Cache capacity: {} entries", capacity);
        println!(
            "  Est. max storage: ~{:.2} MB (capacity × avg object size)",
            (capacity * avg_object_size) as f64 / (1024.0 * 1024.0)
        );
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

                // Track storage usage during simulation
                let mut storage_tracker = StorageTracker::new();

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
                        cache.put(request.key.clone(), 1, request.size);
                        storage_tracker.add(request.size);
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

                stats.record_time(key, algo_duration.as_millis() as u64);
                stats.record_storage(key, storage_tracker.peak(), final_storage, estimated_memory);

                println!(
                    "  Completed in {:.2?} ({:.0} req/s)",
                    algo_duration,
                    processed as f64 / algo_duration.as_secs_f64()
                );
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

        Ok(stats.result(duration, unique_objects_count, capacity))
    }
}
