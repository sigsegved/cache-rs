use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

/// Parameters for generating random traffic logs
/// Parameters for generating traffic for a single hour
struct HourGenerationParams<'a> {
    hour: u32,
    start_time: u64,
    rps: u32,
    popular_objects: u32,
    regular_objects: u32,
    popular_traffic_percent: u8,
    min_size: u64,
    max_size: u64,
    min_ttl: u64,
    max_ttl: u64,
    output_dir: &'a Path,
    progress: &'a Arc<Mutex<(u64, u64)>>,
}

pub struct TrafficLogConfig {
    /// Requests per second
    pub rps: u32,
    /// Total duration in hours
    pub duration_hours: u32,
    /// Number of unique objects
    pub unique_objects: u32,
    /// Traffic distribution: percentage of traffic from popular objects
    pub popular_traffic_percent: u8,
    /// Percentage of objects considered "popular"
    pub popular_objects_percent: u8,
    /// Minimum object size in bytes
    pub min_size: u64,
    /// Maximum object size in bytes
    pub max_size: u64,
    /// Minimum TTL in seconds
    pub min_ttl: u64,
    /// Maximum TTL in seconds
    pub max_ttl: u64,
    /// Output directory
    pub output_dir: PathBuf,
}

impl Default for TrafficLogConfig {
    fn default() -> Self {
        Self {
            rps: 100,
            duration_hours: 24,
            unique_objects: 10_000,
            popular_traffic_percent: 80,
            popular_objects_percent: 20,
            min_size: 1024,        // 1KB
            max_size: 1024 * 1024, // 1MB
            min_ttl: 3600,         // 1 hour
            max_ttl: 86400,        // 24 hours
            output_dir: PathBuf::from("traffic_logs"),
        }
    }
}

/// Generator for random traffic logs
pub struct TrafficLogGenerator {
    config: TrafficLogConfig,
}

impl TrafficLogGenerator {
    /// Create a new generator with the given configuration
    pub fn new(config: TrafficLogConfig) -> Self {
        Self { config }
    }

    /// Generate random traffic logs according to the configuration
    pub fn generate(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure output directory exists
        fs::create_dir_all(&self.config.output_dir)?;

        let start_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let popular_objects = (self.config.unique_objects as f64
            * self.config.popular_objects_percent as f64
            / 100.0) as u32;
        let regular_objects = self.config.unique_objects - popular_objects;

        println!("Generating traffic logs with the following parameters:");
        println!("  Requests per second: {}", self.config.rps);
        println!("  Duration: {} hours", self.config.duration_hours);
        println!("  Unique objects: {}", self.config.unique_objects);
        println!(
            "  Popular objects: {} ({}%)",
            popular_objects, self.config.popular_objects_percent
        );
        println!("  Regular objects: {regular_objects}");
        println!(
            "  Traffic distribution: {}% from {}% of objects",
            self.config.popular_traffic_percent, self.config.popular_objects_percent
        );
        println!(
            "  Size range: {} - {} bytes",
            self.config.min_size, self.config.max_size
        );
        println!(
            "  TTL range: {} - {} seconds",
            self.config.min_ttl, self.config.max_ttl
        );
        println!("  Output directory: {}", self.config.output_dir.display());

        let requests_per_hour = self.config.rps * 3600;
        let total_requests = requests_per_hour * self.config.duration_hours;

        println!(
            "Generating {requests_per_hour} requests per hour, {total_requests} total requests"
        );

        // Create a progress tracker for all threads
        let progress = Arc::new(Mutex::new((0u64, total_requests as u64)));

        // Create one thread per hour
        let mut handles = Vec::new();

        for hour in 0..self.config.duration_hours {
            let config = self.config.clone();
            let hour_start_time = start_time + hour as u64 * 3600;
            let output_dir = self.config.output_dir.clone();
            let progress = Arc::clone(&progress);

            let handle = thread::spawn(move || {
                let params = HourGenerationParams {
                    hour,
                    start_time: hour_start_time,
                    rps: config.rps,
                    popular_objects,
                    regular_objects,
                    popular_traffic_percent: config.popular_traffic_percent,
                    min_size: config.min_size,
                    max_size: config.max_size,
                    min_ttl: config.min_ttl,
                    max_ttl: config.max_ttl,
                    output_dir: &output_dir,
                    progress: &progress,
                };

                let result = Self::generate_hour(params);

                if let Err(e) = result {
                    eprintln!("Error generating traffic for hour {hour}: {e}");
                }
            });

            handles.push(handle);
        }

        // Wait for all threads to complete
        for (i, handle) in handles.into_iter().enumerate() {
            if let Err(e) = handle.join() {
                eprintln!("Thread for hour {i} panicked: {e:?}");
            }
        }

        println!("Traffic log generation complete");
        Ok(())
    }

    /// Generate traffic for a single hour
    fn generate_hour(params: HourGenerationParams<'_>) -> Result<(), Box<dyn std::error::Error>> {
        let hour = params.hour;
        let start_time = params.start_time;
        let hour_file = params
            .output_dir
            .join(format!("traffic_hour_{hour:02}.csv"));
        let mut file = File::create(hour_file)?;

        // Write header
        writeln!(file, "timestamp,key,size,ttl")?;

        let requests_this_hour = params.rps * 3600;

        // Zipf distribution for popular objects
        let zipf_s = 0.9; // Skewness parameter

        // Object set for this hour (simulate shifting popularity)
        let hour_shift = hour % 12; // Cycle through objects over time
        let popular_base = hour_shift * (params.popular_objects / 12).max(1);
        let regular_base = hour_shift * (params.regular_objects / 24).max(1);

        let mut timestamp = start_time;
        let time_step: f64 = 3600.0 / requests_this_hour as f64;

        // Calculate the probability threshold for generating a popular vs regular request
        // This ensures requests are interleaved throughout the hour based on the traffic distribution
        let popular_probability = params.popular_traffic_percent as f64 / 100.0;

        // Generate all requests interleaved - each request randomly decides if it's popular or regular
        // based on the configured traffic distribution percentage
        for _ in 0..requests_this_hour {
            let is_popular_request = rand::random::<f64>() < popular_probability;

            let (key, size) = if is_popular_request {
                // Generate a popular request using Zipf-like distribution
                let rank = (rand::random::<f64>() * params.popular_objects as f64).floor() as u32;
                let zipf_factor = 1.0 / ((rank + 1) as f64).powf(zipf_s);

                // Higher probability for more popular items
                let object_index = if rand::random::<f64>() < zipf_factor * 0.8 {
                    // Very popular items (top 10%)
                    (popular_base + rank % (params.popular_objects / 10)) % params.popular_objects
                } else {
                    // Regular popular items
                    (popular_base + rank) % params.popular_objects
                };

                let key = format!("popular_obj_{object_index}");

                // Generate object size with some correlation to object popularity
                // More popular objects tend to be smaller (e.g., thumbnails vs full videos)
                let popularity_factor = 1.0 - (zipf_factor * 0.5); // 0.5 to 1.0
                let size_range = params.max_size - params.min_size;
                let size = params.min_size
                    + (rand::random::<f64>() * popularity_factor * size_range as f64) as u64;

                (key, size)
            } else {
                // Generate a regular request with uniform distribution
                let object_index = regular_base
                    + (rand::random::<f64>() * params.regular_objects as f64).floor() as u32
                        % params.regular_objects;
                let key = format!("regular_obj_{object_index}");

                // Regular objects have a more uniform size distribution
                let size = params.min_size
                    + (rand::random::<f64>() * (params.max_size - params.min_size) as f64) as u64;

                (key, size)
            };

            // TTL with some variability (same for both types)
            let ttl = params.min_ttl
                + (rand::random::<f64>() * (params.max_ttl - params.min_ttl) as f64) as u64;

            // Write the request
            writeln!(file, "{timestamp},{key},{size},{ttl}")?;

            // Advance time
            timestamp += time_step.round() as u64;
        }

        // Update progress
        let mut progress_data = params.progress.lock().unwrap();
        progress_data.0 += requests_this_hour as u64;
        let (completed, total) = *progress_data;
        let percent = (completed as f64 / total as f64 * 100.0) as u32;

        println!("Hour {hour:02} complete: {completed}/{total} requests ({percent}%)");

        Ok(())
    }
}

impl Clone for TrafficLogConfig {
    fn clone(&self) -> Self {
        Self {
            rps: self.rps,
            duration_hours: self.duration_hours,
            unique_objects: self.unique_objects,
            popular_traffic_percent: self.popular_traffic_percent,
            popular_objects_percent: self.popular_objects_percent,
            min_size: self.min_size,
            max_size: self.max_size,
            min_ttl: self.min_ttl,
            max_ttl: self.max_ttl,
            output_dir: self.output_dir.clone(),
        }
    }
}
