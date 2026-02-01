extern crate cache_rs;

use std::env;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use cache_simulator::input::LogReader;
use cache_simulator::models::{CacheAlgorithm, CacheMode, SimulationConfig};
use cache_simulator::runner::SimulationRunner;

fn main() -> Result<(), String> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();

    let input_dir = if args.len() < 2 {
        // No input directory provided, create test data
        println!("No input directory provided. Using test data...");
        match create_test_logs() {
            Ok(dir) => dir,
            Err(e) => return Err(format!("Failed to create test data: {e}")),
        }
    } else {
        let dir = PathBuf::from(&args[1]);
        if !dir.exists() || !dir.is_dir() {
            return Err(format!("Input directory does not exist: {dir:?}"));
        }
        dir
    };

    // Parse capacity (default: 10000 entries)
    let capacity = if args.len() > 2 {
        args[2].parse::<usize>().unwrap_or(10000)
    } else {
        10000
    };

    println!("Cache Simulation");
    println!("===============");
    println!("Input directory: {input_dir:?}");
    println!("Capacity: {capacity} entries");

    // Create simulation configuration
    let config = SimulationConfig {
        input_dir,
        capacity,
        max_size: 104857600, // 100MB default
        algorithms: CacheAlgorithm::all(),
        modes: vec![CacheMode::Sequential],
        segment_count: None,
        use_size: false,
    };

    // Count log files
    let log_parser = LogReader::new(&config.input_dir);
    let log_files = match log_parser.get_log_files() {
        Ok(files) => files,
        Err(err) => return Err(format!("Failed to list log files: {err}")),
    };

    println!("Found {} log files", log_files.len());
    for file in &log_files {
        println!("  - {:?}", file.file_name().unwrap_or_default());
    }

    // Run simulation
    println!("\nRunning simulation...");
    let runner = SimulationRunner::new(config);
    let result = runner.run()?;

    println!("\nSimulation completed in {:.2?}", result.duration);
    println!(
        "Processed {} requests ({:.2} MB)",
        result.total_requests,
        result.total_bytes as f64 / (1024.0 * 1024.0)
    );

    // Display algorithm comparison
    println!("\nAlgorithm comparison:");
    println!(
        "| {:<10} | {:<10} | {:<10} | {:<15} | {:<15} |",
        "Algorithm", "Hit Rate", "Byte Rate", "Hits/Misses", "MB Hit/Miss"
    );
    println!(
        "|{:-<12}|{:-<12}|{:-<12}|{:-<17}|{:-<17}|",
        "", "", "", "", ""
    );

    for (key, stats) in &result.stats {
        println!(
            "| {:<10} | {:>8.2}% | {:>8.2}% | {:>6}/{:<6} | {:>6.1}/{:<6.1} |",
            key.algorithm.as_str(),
            stats.hit_rate(),
            stats.byte_hit_rate(),
            stats.hits,
            stats.misses,
            stats.bytes_hit as f64 / 1_048_576.0,
            stats.bytes_miss as f64 / 1_048_576.0
        );
    }

    Ok(())
}

// Create test log files if they don't exist
fn create_test_logs() -> Result<PathBuf, String> {
    use std::fs;

    // Create test directory
    let test_dir = PathBuf::from("./test_data");
    match fs::create_dir_all(&test_dir) {
        Ok(_) => {}
        Err(e) => return Err(format!("Failed to create test directory: {e}")),
    };

    // Create CSV log file
    let csv_path = test_dir.join("requests.csv");
    if !csv_path.exists() {
        let file = match fs::File::create(&csv_path) {
            Ok(file) => file,
            Err(e) => return Err(format!("Failed to create CSV file: {e}")),
        };

        let mut writer = csv::Writer::from_writer(file);

        // Generate synthetic requests
        let mut timestamp = match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => duration.as_secs(),
            Err(_) => return Err("Failed to get system time".to_string()),
        };

        // Generate 10,000 requests with a Zipfian-like distribution
        // 20% of the objects receive 80% of requests (80-20 rule)
        let popular_keys = 200;
        let total_keys = 1000;

        for _ in 0..10_000 {
            // Determine if this is a request for a popular object (80% probability)
            let use_popular = rand::random::<f64>() < 0.8;

            let key_id = if use_popular {
                // Choose from popular keys (0-199)
                (rand::random::<f64>() * popular_keys as f64).floor() as u32
            } else {
                // Choose from less popular keys (200-999)
                popular_keys as u32
                    + (rand::random::<f64>() * (total_keys - popular_keys) as f64).floor() as u32
            };

            // Generate size (between 1KB and 1MB with popular objects tending to be smaller)
            let size = if use_popular {
                1024 + (rand::random::<f64>() * 100_000.0) as usize
            } else {
                50_000 + (rand::random::<f64>() * 950_000.0) as usize
            };

            // TTL between 1 hour and 24 hours
            let ttl = 3600 + (rand::random::<f64>() * 82800.0) as u64;

            // Write record
            if writer
                .write_record(&[
                    timestamp.to_string(),
                    format!("obj_{key_id}"),
                    size.to_string(),
                    ttl.to_string(),
                ])
                .is_err()
            {
                return Err("Failed to write to CSV file".to_string());
            }

            // Advance time by 1-10 seconds
            timestamp += 1 + (rand::random::<f64>() * 9.0) as u64;
        }

        // Flush writer
        if writer.flush().is_err() {
            return Err("Failed to flush CSV writer".to_string());
        }

        println!("Created test data at {csv_path:?}");
    }

    Ok(test_dir)
}
