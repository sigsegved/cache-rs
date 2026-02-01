use clap::{Parser, Subcommand};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

mod generator;
mod input;
mod models;
mod runner;
mod stats;

/// Cache algorithm simulator CLI
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Directory containing log files (for simulation)
    #[arg(short, long, value_name = "DIR")]
    input_dir: Option<PathBuf>,

    /// Cache capacity (number of entries)
    #[arg(short, long, default_value = "10000")]
    capacity: usize,

    /// Algorithms to simulate (lru, lfu, lfuda, slru, gdsf, moka)
    /// If not provided, all algorithms will be used
    #[arg(short, long, value_name = "ALGOS", num_args = 1.., value_delimiter = ',')]
    algorithms: Option<Vec<String>>,
}

/// Subcommands for the CLI
#[derive(Subcommand, Debug)]
enum Commands {
    /// Run cache simulation
    Simulate {
        /// Directory containing log files
        #[arg(short, long, value_name = "DIR")]
        input_dir: Option<PathBuf>,

        /// Cache capacity (number of entries) - used when --use-size is not enabled
        #[arg(short, long, default_value = "10000")]
        capacity: usize,

        /// Maximum cache size in bytes - used when --use-size is enabled
        /// Example: 104857600 for 100MB, 1073741824 for 1GB
        #[arg(long, default_value = "104857600")]
        max_size: u64,

        /// Algorithms to simulate (lru, lfu, lfuda, slru, gdsf, moka)
        #[arg(short, long, value_name = "ALGOS", num_args = 1.., value_delimiter = ',')]
        algorithms: Option<Vec<String>>,

        /// Cache mode: sequential, concurrent, or both (default: both)
        #[arg(long, default_value = "both")]
        mode: String,

        /// Number of segments for concurrent caches (default: 16)
        #[arg(long)]
        segments: Option<usize>,

        /// Number of worker threads for concurrent benchmarks (default: 1)
        /// Note: Currently only affects concurrent caches. Higher thread counts
        /// stress-test the cache's concurrency handling.
        #[arg(long, default_value = "1")]
        threads: usize,

        /// Export results to CSV file
        #[arg(long, value_name = "PATH")]
        output_csv: Option<PathBuf>,

        /// Use object sizes for cache eviction (simulates disk/size-based caches)
        /// When enabled, caches evict based on --max-size instead of --capacity
        #[arg(long)]
        use_size: bool,
    },

    /// Generate random traffic logs
    Generate {
        /// Requests per second
        #[arg(long, default_value = "100")]
        rps: u32,

        /// Duration in hours
        #[arg(long, default_value = "24")]
        duration: u32,

        /// Number of unique objects
        #[arg(long, default_value = "10000")]
        objects: u32,

        /// Percentage of traffic from popular objects (default: 80%)
        #[arg(long, default_value = "80")]
        popular_traffic: u8,

        /// Percentage of objects that are popular (default: 20%)
        #[arg(long, default_value = "20")]
        popular_objects: u8,

        /// Minimum object size in KB
        #[arg(long, default_value = "1")]
        min_size: u64,

        /// Maximum object size in KB
        #[arg(long, default_value = "10240")]
        max_size: u64,

        /// Minimum TTL in hours
        #[arg(long, default_value = "1")]
        min_ttl: u64,

        /// Maximum TTL in hours
        #[arg(long, default_value = "24")]
        max_ttl: u64,

        /// Output directory
        #[arg(short, long, default_value = "traffic_logs")]
        output: PathBuf,

        /// Write buffer size in KB (default: 8192 = 8 MB)
        #[arg(long, default_value = "8192")]
        buffer_size: u32,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    match args.command {
        Some(Commands::Generate {
            rps,
            duration,
            objects,
            popular_traffic,
            popular_objects,
            min_size,
            max_size,
            min_ttl,
            max_ttl,
            output,
            buffer_size,
        }) => {
            // Convert KB to bytes for sizes
            let min_size_bytes = min_size * 1024;
            let max_size_bytes = max_size * 1024;

            // Convert hours to seconds for TTL
            let min_ttl_seconds = min_ttl * 3600;
            let max_ttl_seconds = max_ttl * 3600;

            // Create traffic generator configuration
            let config = generator::TrafficLogConfig {
                rps,
                duration_hours: duration,
                unique_objects: objects,
                popular_traffic_percent: popular_traffic,
                popular_objects_percent: popular_objects,
                min_size: min_size_bytes,
                max_size: max_size_bytes,
                min_ttl: min_ttl_seconds,
                max_ttl: max_ttl_seconds,
                output_dir: output,
                buffer_size_kb: buffer_size,
            };

            // Generate traffic logs
            let generator = generator::TrafficLogGenerator::new(config);
            generator.generate()?;

            Ok(())
        }

        Some(Commands::Simulate {
            input_dir,
            capacity,
            max_size,
            algorithms,
            mode,
            segments,
            threads,
            output_csv,
            use_size,
        }) => run_simulator(
            input_dir, capacity, max_size, algorithms, mode, segments, threads, output_csv,
            use_size,
        ),

        None => {
            // Legacy mode (no subcommand) - default to both modes for comparison
            run_simulator(
                args.input_dir,
                args.capacity,
                104857600, // 100MB default max_size
                args.algorithms,
                "both".to_string(),
                None,
                1, // default threads
                None,
                false,
            )
        }
    }
}

/// Parse the mode string into CacheMode values
fn parse_modes(mode: &str) -> Vec<models::CacheMode> {
    match mode.to_lowercase().as_str() {
        "sequential" | "seq" => vec![models::CacheMode::Sequential],
        "concurrent" | "conc" => vec![models::CacheMode::Concurrent],
        "both" | "all" => vec![models::CacheMode::Sequential, models::CacheMode::Concurrent],
        _ => {
            println!("Warning: Unknown mode '{mode}', using 'both'");
            vec![models::CacheMode::Sequential, models::CacheMode::Concurrent]
        }
    }
}

/// Run the simulator with the given parameters
#[allow(clippy::too_many_arguments)]
fn run_simulator(
    input_dir: Option<PathBuf>,
    capacity: usize,
    max_size: u64,
    algorithms: Option<Vec<String>>,
    mode: String,
    segments: Option<usize>,
    threads: usize,
    output_csv: Option<PathBuf>,
    use_size: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Determine input directory
    let input_dir = match input_dir {
        Some(dir) => dir,
        None => {
            // Create test data if no input directory provided
            ensure_test_data()?;
            PathBuf::from("test_data")
        }
    };

    // Determine which algorithms to use
    let algorithms = match &algorithms {
        Some(alg_names) if !alg_names.is_empty() => {
            let mut selected_algorithms = Vec::new();
            for name in alg_names {
                match name.to_lowercase().as_str() {
                    "lru" => selected_algorithms.push(models::CacheAlgorithm::Lru),
                    "lfu" => selected_algorithms.push(models::CacheAlgorithm::Lfu),
                    "lfuda" => selected_algorithms.push(models::CacheAlgorithm::Lfuda),
                    "slru" => selected_algorithms.push(models::CacheAlgorithm::Slru),
                    "gdsf" => selected_algorithms.push(models::CacheAlgorithm::Gdsf),
                    "moka" => selected_algorithms.push(models::CacheAlgorithm::Moka),
                    _ => println!("Warning: Unknown algorithm '{name}', skipping"),
                }
            }
            if selected_algorithms.is_empty() {
                println!("No valid algorithms selected, using all available algorithms");
                models::CacheAlgorithm::all()
            } else {
                selected_algorithms
            }
        }
        _ => {
            // None or empty vector - use all algorithms
            models::CacheAlgorithm::all()
        }
    };

    // Parse modes
    let modes = parse_modes(&mode);

    println!("Cache Simulation");
    println!("===============");
    println!("Input directory: {}", input_dir.display());
    if use_size {
        println!("Size-based eviction: enabled");
        println!(
            "Max cache size: {} bytes ({:.2} MB)",
            max_size,
            max_size as f64 / 1_048_576.0
        );
    } else {
        println!("Cache capacity: {} entries", capacity);
    }
    println!(
        "Algorithms: {:?}",
        algorithms.iter().map(|a| a.as_str()).collect::<Vec<_>>()
    );
    println!(
        "Modes: {:?}",
        modes.iter().map(|m| m.as_str()).collect::<Vec<_>>()
    );
    if let Some(seg) = segments {
        println!("Concurrent segments: {seg}");
    }
    if threads > 1 {
        println!("Worker threads: {threads}");
        println!("  Note: Multi-threaded execution is a planned feature.");
        println!("  Currently runs single-threaded but uses thread-safe caches.");
    }
    println!();

    // Create simulation configuration
    let config = models::SimulationConfig {
        input_dir,
        capacity,
        max_size,
        algorithms,
        modes,
        segment_count: segments,
        thread_count: threads,
        use_size,
    };

    run_simulation(config, output_csv)
}

/// Run the simulation with the given configuration
fn run_simulation(
    config: models::SimulationConfig,
    output_csv: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create and run the simulation
    let runner = runner::SimulationRunner::new(config);
    match runner.run() {
        Ok(result) => {
            println!("\nSimulation completed in {:.2?}", result.duration);
            println!("Total requests: {}", result.total_requests);
            println!("Unique objects: {}", result.unique_objects);
            println!(
                "Total bytes: {} ({:.2} MB)",
                result.total_bytes,
                result.total_bytes as f64 / (1024.0 * 1024.0)
            );

            // Export to CSV if requested
            if let Some(csv_path) = output_csv {
                // Create stats from result for CSV export
                let stats = stats::SimulationStats::from_result(&result);
                match stats.export_csv(&csv_path) {
                    Ok(()) => println!("\nResults exported to: {}", csv_path.display()),
                    Err(e) => eprintln!("Failed to export CSV: {e}"),
                }
            }

            Ok(())
        }
        Err(e) => {
            eprintln!("Error running simulation: {e}");
            Err(e.into())
        }
    }
}

/// Ensure test data exists
fn ensure_test_data() -> Result<(), Box<dyn std::error::Error>> {
    let test_dir = Path::new("test_data");
    if !test_dir.exists() {
        std::fs::create_dir_all(test_dir)?;
    }

    let csv_path = test_dir.join("requests.csv");
    if !csv_path.exists() {
        println!("Creating test data...");
        let mut file = File::create(&csv_path)?;

        // Write header
        writeln!(file, "timestamp,key,size,ttl")?;

        // Generate synthetic requests
        let mut timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Generate requests that will show differences between cache algorithms
        let total_keys = 10_000; // Large key space
        let popular_group_size = 500; // Group of currently popular keys
        let max_popular_groups = 8; // Number of different popularity cycles
        let request_count = 50_000; // More requests to show meaningful differences

        // Track the currently popular group
        let mut current_popular_group = 0;
        let mut group_request_count = 0;
        let group_switch_threshold = request_count / max_popular_groups;

        // Zipf distribution constants for key popularity
        let zipf_s = 0.8; // Skewness parameter

        println!(
            "Generating {request_count} requests for {total_keys} unique keys with shifting access patterns..."
        );

        for i in 0..request_count {
            // Switch popular group every group_switch_threshold requests
            if group_request_count >= group_switch_threshold {
                current_popular_group = (current_popular_group + 1) % max_popular_groups;
                group_request_count = 0;
                println!("  Switching popularity to group {current_popular_group} at request {i}");
            }

            group_request_count += 1;

            // Generate key based on current pattern and access frequency
            let key_id = if rand::random::<f64>() < 0.75 {
                // Popular keys from current group (locality favors LRU)
                // Use Zipf-like distribution within popular group
                let base = current_popular_group * popular_group_size;
                let rank = (rand::random::<f64>() * popular_group_size as f64).floor() as u32;
                let zipf_factor = 1.0 / ((rank + 1) as f64).powf(zipf_s);

                // More skewed toward lower ranks (more popular items)
                if rand::random::<f64>() < zipf_factor * 0.3 {
                    base + rank % (popular_group_size / 10) // Very popular items
                } else {
                    base + rank // Normal popularity items
                }
            } else if rand::random::<f64>() < 0.6 {
                // Frequently accessed keys across all groups (favors LFU)
                // These will be accessed regardless of the current popular group
                (rand::random::<f64>() * 200.0).floor() as u32
            } else {
                // Random access to unpopular keys with long tail
                2000 + (rand::random::<f64>() * (total_keys - 2000) as f64).floor() as u32
            };

            // Size distribution to test size-aware algorithms like GDSF
            let size = match key_id % 10 {
                0 | 1 => {
                    // 20% are large objects (favors size-aware eviction)
                    25_000 + (rand::random::<f64>() * 50_000.0).floor() as u64
                }
                2..=4 => {
                    // 30% are medium objects
                    5_000 + (rand::random::<f64>() * 15_000.0).floor() as u64
                }
                _ => {
                    // 50% are small objects
                    500 + (rand::random::<f64>() * 3_000.0).floor() as u64
                }
            };

            // TTL: More varied TTL distribution to test expiration handling
            let ttl = match key_id % 20 {
                0 => {
                    // 5% with very short TTL (1-60 seconds)
                    1 + (rand::random::<f64>() * 59.0) as u64
                }
                1 | 2 => {
                    // 10% with short TTL (1-10 minutes)
                    60 + (rand::random::<f64>() * 540.0) as u64
                }
                3..=5 => {
                    // 15% with medium TTL (10-60 minutes)
                    600 + (rand::random::<f64>() * 3000.0) as u64
                }
                6..=9 => {
                    // 20% with long TTL (1-24 hours)
                    3600 + (rand::random::<f64>() * 82800.0) as u64
                }
                _ => {
                    // 50% with no TTL
                    0
                }
            };

            // Write the request
            writeln!(file, "{timestamp},{key_id},{size},key_{ttl}")?;

            // Advance time by 1-5 seconds
            timestamp += 1 + (rand::random::<f64>() * 4.0) as u64;
        }

        println!("Created test data in {}", csv_path.display());
    }

    Ok(())
}
