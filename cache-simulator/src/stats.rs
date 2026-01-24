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

    /// Record a request (increases total counts) - call once per request
    #[allow(dead_code)]
    pub fn record_request(&mut self, size: usize) {
        self.total_requests += 1;
        self.total_bytes += size;
    }

    /// Get the current result
    pub fn result(&self, duration: std::time::Duration, unique_objects: usize) -> SimulationResult {
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
            "{:<6} {:<12} {:>10} {:>10} {:>15} {:>8}",
            "Algo", "Mode", "Hit Rate", "Byte HR", "Hits/Misses", "Time(ms)"
        );
        println!("{}", "-".repeat(70));

        // Sort keys for consistent output
        let mut keys: Vec<_> = self.stats.keys().collect();
        keys.sort();

        for key in keys {
            if let Some(stats) = self.stats.get(key) {
                println!(
                    "{:<6} {:<12} {:>9.2}% {:>9.2}% {:>7}/{:<7} {:>8}",
                    key.algorithm.as_str(),
                    key.mode.as_str(),
                    stats.hit_rate(),
                    stats.byte_hit_rate(),
                    stats.hits,
                    stats.misses,
                    stats.simulation_time_ms
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
