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
    /// Memory size in bytes
    pub memory_size: usize,
    /// Disk size in bytes
    #[allow(dead_code)]
    pub disk_size: usize,
    /// Algorithms to simulate
    pub algorithms: Vec<CacheAlgorithm>,
    /// Modes to simulate
    pub modes: Vec<CacheMode>,
    /// Number of segments for concurrent caches (None = auto)
    pub segment_count: Option<usize>,
    /// Override calculated capacity with explicit value
    pub capacity_override: Option<usize>,
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
    /// Simulation time in milliseconds
    pub simulation_time_ms: u64,
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
}
