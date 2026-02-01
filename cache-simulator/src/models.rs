// Data models for cache simulation

use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Represents a single cache request
#[derive(Debug, Clone)]
pub struct Request {
    /// Timestamp of the request
    pub timestamp: SystemTime,
    /// Cache key
    pub key: String,
    /// Size of the object in bytes
    pub size: usize,
    /// Time-to-live in seconds (0 means no TTL)
    #[allow(dead_code)]
    pub ttl: u64,
}

impl Request {
    /// Create a new request
    pub fn new(timestamp: SystemTime, key: String, size: usize, ttl: u64) -> Self {
        Self {
            timestamp,
            key,
            size,
            ttl,
        }
    }
}

/// Cache algorithm types supported for simulation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CacheAlgorithm {
    Lru,
    Slru,
    Lfu,
    Lfuda,
    Gdsf,
    /// Moka cache (external crate for comparison)
    Moka,
}

impl CacheAlgorithm {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheAlgorithm::Lru => "LRU",
            CacheAlgorithm::Slru => "SLRU",
            CacheAlgorithm::Lfu => "LFU",
            CacheAlgorithm::Lfuda => "LFUDA",
            CacheAlgorithm::Gdsf => "GDSF",
            CacheAlgorithm::Moka => "Moka",
        }
    }

    /// Get all available algorithms
    pub fn all() -> Vec<CacheAlgorithm> {
        vec![
            CacheAlgorithm::Lru,
            CacheAlgorithm::Slru,
            CacheAlgorithm::Lfu,
            CacheAlgorithm::Lfuda,
            CacheAlgorithm::Gdsf,
            CacheAlgorithm::Moka,
        ]
    }
}

/// Cache execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CacheMode {
    /// Single-threaded cache (e.g., LruCache)
    Sequential,
    /// Thread-safe segmented cache (e.g., ConcurrentLruCache)
    Concurrent,
}

impl CacheMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheMode::Sequential => "Sequential",
            CacheMode::Concurrent => "Concurrent",
        }
    }

    /// Get all available modes
    #[allow(dead_code)]
    pub fn all() -> Vec<CacheMode> {
        vec![CacheMode::Sequential, CacheMode::Concurrent]
    }
}

impl fmt::Display for CacheMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Unique identifier for a simulation run combining algorithm and mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SimulationKey {
    pub algorithm: CacheAlgorithm,
    pub mode: CacheMode,
}

impl SimulationKey {
    pub fn new(algorithm: CacheAlgorithm, mode: CacheMode) -> Self {
        Self { algorithm, mode }
    }
}

impl fmt::Display for SimulationKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.algorithm.as_str(), self.mode.as_str())
    }
}

/// Configuration for a simulation run
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    /// Directory containing input log files
    pub input_dir: PathBuf,
    /// Cache capacity in number of entries (used when use_size is false)
    pub capacity: usize,
    /// Maximum cache size in bytes (used when use_size is true)
    pub max_size: u64,
    /// Algorithms to simulate
    pub algorithms: Vec<CacheAlgorithm>,
    /// Modes to simulate
    pub modes: Vec<CacheMode>,
    /// Number of segments for concurrent caches (None = auto)
    pub segment_count: Option<usize>,
    /// Number of worker threads for concurrent benchmarks
    /// Note: Multi-threaded execution is a planned feature.
    #[allow(dead_code)]
    pub thread_count: usize,
    /// Use size-based eviction (uses max_size instead of capacity)
    pub use_size: bool,
}

/// Results of a simulation run
#[derive(Debug)]
pub struct SimulationResult {
    /// Statistics for each algorithm+mode combination
    pub stats: HashMap<SimulationKey, AlgorithmStats>,
    /// Total number of requests processed
    pub total_requests: usize,
    /// Total bytes requested
    pub total_bytes: usize,
    /// Number of unique objects in the dataset
    pub unique_objects: usize,
    /// Duration of the simulation
    pub duration: Duration,
    /// Cache capacity used
    #[allow(dead_code)]
    pub capacity: usize,
}

/// Statistics for a single algorithm
#[derive(Debug, Default, Clone)]
pub struct AlgorithmStats {
    /// Number of cache hits
    pub hits: usize,
    /// Number of cache misses
    pub misses: usize,
    /// Bytes served from cache (hits)
    pub bytes_hit: usize,
    /// Bytes served from backend (misses)
    pub bytes_miss: usize,
    /// Simulation time in milliseconds (includes I/O - legacy field)
    pub simulation_time_ms: u64,
    /// Peak storage usage in bytes (sum of object sizes in cache)
    pub peak_storage_bytes: usize,
    /// Final storage usage in bytes
    pub final_storage_bytes: usize,
    /// Estimated memory overhead in bytes (cache data structure overhead)
    pub estimated_memory_bytes: usize,
    /// Latency statistics for cache operations (excludes I/O)
    pub latency: LatencyStats,
}

/// Latency statistics for a single operation type (get, put, put_with_size)
#[derive(Debug, Clone, Default)]
pub struct OpLatencyStats {
    /// Total time spent (nanoseconds)
    pub total_ns: u64,
    /// Number of operations
    pub count: u64,
    /// Minimum latency (nanoseconds)
    pub min_ns: u64,
    /// Maximum latency (nanoseconds)
    pub max_ns: u64,
    /// Latency percentiles
    pub percentiles: Option<LatencyPercentiles>,
}

impl OpLatencyStats {
    /// Calculate average latency in nanoseconds
    pub fn avg_ns(&self) -> f64 {
        if self.count > 0 {
            self.total_ns as f64 / self.count as f64
        } else {
            0.0
        }
    }

    /// Calculate throughput in operations per second
    pub fn ops_per_sec(&self) -> f64 {
        if self.total_ns > 0 {
            (self.count as f64 * 1_000_000_000.0) / self.total_ns as f64
        } else {
            0.0
        }
    }

    /// Get total duration in seconds
    pub fn duration_secs(&self) -> f64 {
        self.total_ns as f64 / 1_000_000_000.0
    }
}

/// Latency statistics for all cache operations
#[derive(Debug, Clone, Default)]
pub struct LatencyStats {
    /// Total time spent in all cache operations (nanoseconds)
    pub total_ns: u64,
    /// Total number of operations
    pub count: u64,
    /// Get operation stats
    pub get_stats: OpLatencyStats,
    /// Put operation stats (regular put without size)
    pub put_stats: OpLatencyStats,
    /// Put with size operation stats
    pub put_with_size_stats: OpLatencyStats,
}

/// Latency percentiles
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]  // Fields are used for metrics/analysis, not all consumed in every code path
pub struct LatencyPercentiles {
    pub p50_ns: u64,
    pub p90_ns: u64,
    pub p99_ns: u64,
    pub p999_ns: u64,
}

impl LatencyStats {
    /// Calculate average latency in nanoseconds (across all operations)
    pub fn avg_ns(&self) -> f64 {
        if self.count > 0 {
            self.total_ns as f64 / self.count as f64
        } else {
            0.0
        }
    }

    /// Calculate throughput in operations per second (all operations)
    pub fn ops_per_sec(&self) -> f64 {
        if self.total_ns > 0 {
            (self.count as f64 * 1_000_000_000.0) / self.total_ns as f64
        } else {
            0.0
        }
    }

    /// Get total duration in seconds
    pub fn duration_secs(&self) -> f64 {
        self.total_ns as f64 / 1_000_000_000.0
    }
}

impl AlgorithmStats {
    /// Create new empty statistics
    pub fn new() -> Self {
        Self::default()
    }

    /// Calculate hit rate as percentage
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total > 0 {
            (self.hits as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// Calculate byte hit rate as percentage
    pub fn byte_hit_rate(&self) -> f64 {
        let total = self.bytes_hit + self.bytes_miss;
        if total > 0 {
            (self.bytes_hit as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }
}

/// CSV export row for simulation results
#[derive(Debug, Serialize)]
pub struct CsvResultRow {
    pub algorithm: String,
    pub mode: String,
    pub hits: usize,
    pub misses: usize,
    pub hit_rate: f64,
    pub byte_hit_rate: f64,
    pub bytes_hit: usize,
    pub bytes_miss: usize,
    pub simulation_time_ms: u64,
    pub peak_storage_bytes: usize,
    pub final_storage_bytes: usize,
    pub estimated_memory_bytes: usize,
    // Combined stats
    /// Total operations (get + put + put_with_size)
    pub total_ops: u64,
    /// Total cache operation time in nanoseconds
    pub total_duration_ns: u64,
    /// Combined ops per second
    pub ops_per_sec: f64,
    /// Combined average latency in nanoseconds
    pub avg_latency_ns: f64,
    // Get operation stats
    pub get_ops: u64,
    pub get_duration_ns: u64,
    pub get_ops_per_sec: f64,
    pub get_avg_ns: f64,
    pub get_min_ns: u64,
    pub get_max_ns: u64,
    pub get_p50_ns: u64,
    pub get_p99_ns: u64,
    // Put operation stats
    pub put_ops: u64,
    pub put_duration_ns: u64,
    pub put_ops_per_sec: f64,
    pub put_avg_ns: f64,
    pub put_min_ns: u64,
    pub put_max_ns: u64,
    pub put_p50_ns: u64,
    pub put_p99_ns: u64,
    // Put with size operation stats
    pub put_size_ops: u64,
    pub put_size_duration_ns: u64,
    pub put_size_ops_per_sec: f64,
    pub put_size_avg_ns: f64,
    pub put_size_min_ns: u64,
    pub put_size_max_ns: u64,
    pub put_size_p50_ns: u64,
    pub put_size_p99_ns: u64,
}
