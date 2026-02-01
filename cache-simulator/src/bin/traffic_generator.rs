use cache_simulator::generator::{TrafficLogConfig, TrafficLogGenerator};
use clap::Parser;
use std::path::PathBuf;

/// Traffic generator for cache simulations
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
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

    /// Minimum object size in MB
    #[arg(long, default_value = "1")]
    min_size: u64,

    /// Maximum object size in MB
    #[arg(long, default_value = "10")]
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
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let args = Args::parse();

    // Convert MB to bytes for sizes
    let min_size_bytes = args.min_size * 1024 * 1024;
    let max_size_bytes = args.max_size * 1024 * 1024;

    // Convert hours to seconds for TTL
    let min_ttl_seconds = args.min_ttl * 3600;
    let max_ttl_seconds = args.max_ttl * 3600;

    // Create traffic generator configuration
    let config = TrafficLogConfig {
        rps: args.rps,
        duration_hours: args.duration,
        unique_objects: args.objects,
        popular_traffic_percent: args.popular_traffic,
        popular_objects_percent: args.popular_objects,
        min_size: min_size_bytes,
        max_size: max_size_bytes,
        min_ttl: min_ttl_seconds,
        max_ttl: max_ttl_seconds,
        output_dir: args.output,
        buffer_size_kb: args.buffer_size,
    };

    println!("Traffic Generator");
    println!("================");

    // Generate traffic logs
    let generator = TrafficLogGenerator::new(config);
    generator.generate()?;

    Ok(())
}
