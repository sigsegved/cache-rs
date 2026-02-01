// Statistics collection and reporting for cache simulation

use crate::models::{
    AlgorithmStats, CacheAlgorithm, CacheMode, CsvResultRow, SimulationKey, SimulationResult,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Collects and reports statistics from simulation runs
pub struct SimulationStats {
    /// Stats for each algorithm+mode combination
    stats: HashMap<SimulationKey, AlgorithmStats>,
    /// Total number of requests
    #[allow(dead_code)]
    total_requests: usize,
    /// Total bytes requested
    #[allow(dead_code)]
    total_bytes: usize,
    /// Algorithms being tested
    algorithms: Vec<CacheAlgorithm>,
    /// Modes being tested
    #[allow(dead_code)]
    modes: Vec<CacheMode>,
}

impl SimulationStats {
    /// Create a new statistics collector for the given algorithms and modes
    pub fn new(algorithms: &[CacheAlgorithm], modes: &[CacheMode]) -> Self {
        let mut stats = HashMap::new();
        for &algo in algorithms {
            for &mode in modes {
                let key = SimulationKey::new(algo, mode);
                stats.insert(key, AlgorithmStats::new());
            }
        }

        Self {
            stats,
            total_requests: 0,
            total_bytes: 0,
            algorithms: algorithms.to_vec(),
            modes: modes.to_vec(),
        }
    }

    /// Record a cache hit
    pub fn record_hit(&mut self, key: SimulationKey, size: usize) {
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.hits += 1;
            stats.bytes_hit += size;
        }
        // Track total requests only once per unique request, not per algorithm
        // This is handled separately in record_request
    }

    /// Record a cache miss
    pub fn record_miss(&mut self, key: SimulationKey, size: usize) {
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.misses += 1;
            stats.bytes_miss += size;
        }
    }

    /// Record simulation time for an algorithm+mode
    pub fn record_time(&mut self, key: SimulationKey, time_ms: u64) {
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.simulation_time_ms = time_ms;
        }
    }

    /// Record latency statistics for an algorithm+mode
    pub fn record_latency(&mut self, key: SimulationKey, latency: crate::models::LatencyStats) {
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.latency = latency;
        }
    }

    /// Record storage statistics for an algorithm+mode
    pub fn record_storage(
        &mut self,
        key: SimulationKey,
        peak_bytes: usize,
        final_bytes: usize,
        estimated_memory: usize,
    ) {
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.peak_storage_bytes = peak_bytes;
            stats.final_storage_bytes = final_bytes;
            stats.estimated_memory_bytes = estimated_memory;
        }
    }

    /// Record a request (increases total counts) - call once per request
    #[allow(dead_code)]
    pub fn record_request(&mut self, size: usize) {
        self.total_requests += 1;
        self.total_bytes += size;
    }

    /// Get the current result
    pub fn result(
        &self,
        duration: std::time::Duration,
        unique_objects: usize,
        capacity: usize,
    ) -> SimulationResult {
        // Calculate total requests from one of the algorithm stats
        let total_requests = self
            .stats
            .values()
            .next()
            .map(|s| s.hits + s.misses)
            .unwrap_or(0);

        // Calculate total bytes
        let total_bytes = self
            .stats
            .values()
            .next()
            .map(|s| s.bytes_hit + s.bytes_miss)
            .unwrap_or(0);

        SimulationResult {
            stats: self.stats.clone(),
            total_requests,
            total_bytes,
            unique_objects,
            duration,
            capacity,
        }
    }

    /// Print a summary report of the simulation results
    pub fn print_summary(&self) {
        let total_requests = self
            .stats
            .values()
            .next()
            .map(|s| s.hits + s.misses)
            .unwrap_or(0);
        let total_bytes = self
            .stats
            .values()
            .next()
            .map(|s| s.bytes_hit + s.bytes_miss)
            .unwrap_or(0);

        println!("\nCache Simulation Summary");
        println!("========================");
        println!("Total requests: {}", total_requests);
        println!(
            "Total bytes: {} ({:.2} MB)",
            total_bytes,
            total_bytes as f64 / (1024.0 * 1024.0)
        );

        println!("\nResults by Algorithm and Mode:");
        println!(
            "{:<6} {:<10} {:>8} {:>10} {:>12} {:>10} {:>10} {:>10} {:>10} {:>10}",
            "Algo",
            "Mode",
            "HitRate",
            "ByteHit%",
            "TotalOps",
            "Duration",
            "Ops/sec",
            "GetAvg",
            "PutAvg",
            "p99"
        );
        println!("{}", "-".repeat(120));

        // Sort keys for consistent output
        let mut keys: Vec<_> = self.stats.keys().collect();
        keys.sort();

        for key in keys {
            if let Some(stats) = self.stats.get(key) {
                let get_p99 = stats
                    .latency
                    .get_stats
                    .percentiles
                    .as_ref()
                    .map(|p| p.p99_ns)
                    .unwrap_or(0);

                // Determine which put type was used
                let (put_avg, _put_count) = if stats.latency.put_stats.count > 0 {
                    (
                        stats.latency.put_stats.avg_ns(),
                        stats.latency.put_stats.count,
                    )
                } else {
                    (
                        stats.latency.put_with_size_stats.avg_ns(),
                        stats.latency.put_with_size_stats.count,
                    )
                };

                println!(
                    "{:<6} {:<10} {:>7.2}% {:>9.2}% {:>12} {:>9.3}s {:>10.0} {:>9.0}ns {:>9.0}ns {:>9}ns",
                    key.algorithm.as_str(),
                    key.mode.as_str(),
                    stats.hit_rate(),
                    stats.byte_hit_rate(),
                    stats.latency.count,
                    stats.latency.duration_secs(),
                    stats.latency.ops_per_sec(),
                    stats.latency.get_stats.avg_ns(),
                    put_avg,
                    get_p99
                );
            }
        }
    }

    /// Print a comparison between sequential and concurrent modes
    pub fn print_comparison(&self) {
        println!("\n┌─────────────────────────────────────────────────────────────────┐");
        println!("│              Hit-Rate Comparison: Sequential vs Concurrent      │");
        println!("├──────────┬────────────┬────────────┬──────────┬─────────────────┤");
        println!("│ Algorithm│ Sequential │ Concurrent │  Delta   │ Notes           │");
        println!("├──────────┼────────────┼────────────┼──────────┼─────────────────┤");

        for algo in &self.algorithms {
            let seq_key = SimulationKey::new(*algo, CacheMode::Sequential);
            let conc_key = SimulationKey::new(*algo, CacheMode::Concurrent);

            let seq_stats = self.stats.get(&seq_key);
            let conc_stats = self.stats.get(&conc_key);

            match (seq_stats, conc_stats) {
                (Some(seq), Some(conc)) => {
                    let seq_hr = seq.hit_rate();
                    let conc_hr = conc.hit_rate();
                    let delta = conc_hr - seq_hr;

                    let delta_str = if delta >= 0.0 {
                        format!("+{:.2}%", delta)
                    } else {
                        format!("{:.2}%", delta)
                    };

                    let notes = if delta.abs() < 0.1 {
                        "~equal"
                    } else if delta < 0.0 {
                        "seg. overhead"
                    } else {
                        "unexpected"
                    };

                    println!(
                        "│ {:<8} │ {:>9.2}% │ {:>9.2}% │ {:>8} │ {:<15} │",
                        algo.as_str(),
                        seq_hr,
                        conc_hr,
                        delta_str,
                        notes
                    );
                }
                _ => {
                    println!(
                        "│ {:<8} │ {:>10} │ {:>10} │ {:>8} │ {:<15} │",
                        algo.as_str(),
                        "N/A",
                        "N/A",
                        "N/A",
                        "missing data"
                    );
                }
            }
        }

        println!("└──────────┴────────────┴────────────┴──────────┴─────────────────┘");
        println!("\nNote: Concurrent caches use segmented storage. Eviction decisions are");
        println!("per-segment (not global), which may cause slightly lower hit rates.");
    }

    /// Create SimulationStats from a SimulationResult (for CSV export after run)
    pub fn from_result(result: &SimulationResult) -> Self {
        // Extract algorithms and modes from the result keys
        let mut algorithms: Vec<CacheAlgorithm> =
            result.stats.keys().map(|k| k.algorithm).collect();
        let mut modes: Vec<CacheMode> = result.stats.keys().map(|k| k.mode).collect();

        // Deduplicate and sort
        algorithms.sort();
        algorithms.dedup();
        modes.sort();
        modes.dedup();

        Self {
            stats: result.stats.clone(),
            total_requests: result.total_requests,
            total_bytes: result.total_bytes,
            algorithms,
            modes,
        }
    }

    /// Export results to a CSV file
    pub fn export_csv(&self, path: &Path) -> Result<(), std::io::Error> {
        let mut writer = csv::Writer::from_path(path)?;

        // Sort keys for consistent output
        let mut keys: Vec<_> = self.stats.keys().collect();
        keys.sort();

        for key in keys {
            if let Some(stats) = self.stats.get(key) {
                let row = CsvResultRow {
                    algorithm: key.algorithm.as_str().to_string(),
                    mode: key.mode.as_str().to_string(),
                    hits: stats.hits,
                    misses: stats.misses,
                    hit_rate: stats.hit_rate(),
                    byte_hit_rate: stats.byte_hit_rate(),
                    bytes_hit: stats.bytes_hit,
                    bytes_miss: stats.bytes_miss,
                    simulation_time_ms: stats.simulation_time_ms,
                    peak_storage_bytes: stats.peak_storage_bytes,
                    final_storage_bytes: stats.final_storage_bytes,
                    estimated_memory_bytes: stats.estimated_memory_bytes,
                    // Combined stats
                    total_ops: stats.latency.count,
                    total_duration_ns: stats.latency.total_ns,
                    ops_per_sec: stats.latency.ops_per_sec(),
                    avg_latency_ns: stats.latency.avg_ns(),
                    // Get stats
                    get_ops: stats.latency.get_stats.count,
                    get_duration_ns: stats.latency.get_stats.total_ns,
                    get_ops_per_sec: stats.latency.get_stats.ops_per_sec(),
                    get_avg_ns: stats.latency.get_stats.avg_ns(),
                    get_min_ns: stats.latency.get_stats.min_ns,
                    get_max_ns: stats.latency.get_stats.max_ns,
                    get_p50_ns: stats
                        .latency
                        .get_stats
                        .percentiles
                        .as_ref()
                        .map(|p| p.p50_ns)
                        .unwrap_or(0),
                    get_p99_ns: stats
                        .latency
                        .get_stats
                        .percentiles
                        .as_ref()
                        .map(|p| p.p99_ns)
                        .unwrap_or(0),
                    // Put stats
                    put_ops: stats.latency.put_stats.count,
                    put_duration_ns: stats.latency.put_stats.total_ns,
                    put_ops_per_sec: stats.latency.put_stats.ops_per_sec(),
                    put_avg_ns: stats.latency.put_stats.avg_ns(),
                    put_min_ns: stats.latency.put_stats.min_ns,
                    put_max_ns: stats.latency.put_stats.max_ns,
                    put_p50_ns: stats
                        .latency
                        .put_stats
                        .percentiles
                        .as_ref()
                        .map(|p| p.p50_ns)
                        .unwrap_or(0),
                    put_p99_ns: stats
                        .latency
                        .put_stats
                        .percentiles
                        .as_ref()
                        .map(|p| p.p99_ns)
                        .unwrap_or(0),
                    // Put with size stats
                    put_size_ops: stats.latency.put_with_size_stats.count,
                    put_size_duration_ns: stats.latency.put_with_size_stats.total_ns,
                    put_size_ops_per_sec: stats.latency.put_with_size_stats.ops_per_sec(),
                    put_size_avg_ns: stats.latency.put_with_size_stats.avg_ns(),
                    put_size_min_ns: stats.latency.put_with_size_stats.min_ns,
                    put_size_max_ns: stats.latency.put_with_size_stats.max_ns,
                    put_size_p50_ns: stats
                        .latency
                        .put_with_size_stats
                        .percentiles
                        .as_ref()
                        .map(|p| p.p50_ns)
                        .unwrap_or(0),
                    put_size_p99_ns: stats
                        .latency
                        .put_with_size_stats
                        .percentiles
                        .as_ref()
                        .map(|p| p.p99_ns)
                        .unwrap_or(0),
                };
                writer.serialize(row)?;
            }
        }

        writer.flush()?;
        Ok(())
    }

    /// Export comparison results to CSV
    #[allow(dead_code)]
    pub fn export_comparison_csv(&self, path: &Path) -> Result<(), std::io::Error> {
        let mut file = File::create(path)?;

        writeln!(
            file,
            "algorithm,seq_hit_rate,conc_hit_rate,delta,seq_byte_hr,conc_byte_hr,byte_delta"
        )?;

        for algo in &self.algorithms {
            let seq_key = SimulationKey::new(*algo, CacheMode::Sequential);
            let conc_key = SimulationKey::new(*algo, CacheMode::Concurrent);

            if let (Some(seq), Some(conc)) = (self.stats.get(&seq_key), self.stats.get(&conc_key)) {
                let delta = conc.hit_rate() - seq.hit_rate();
                let byte_delta = conc.byte_hit_rate() - seq.byte_hit_rate();

                writeln!(
                    file,
                    "{},{:.4},{:.4},{:.4},{:.4},{:.4},{:.4}",
                    algo.as_str(),
                    seq.hit_rate(),
                    conc.hit_rate(),
                    delta,
                    seq.byte_hit_rate(),
                    conc.byte_hit_rate(),
                    byte_delta
                )?;
            }
        }

        Ok(())
    }
}
