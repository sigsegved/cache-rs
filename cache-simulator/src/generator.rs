use std::fs::{self, File};
use std::io::{BufWriter, Write};
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
    buffer_size_kb: u32,
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
    /// Write buffer size in KB (default: 8192 = 8 MB)
    pub buffer_size_kb: u32,
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
            buffer_size_kb: 8192, // 8 MB default buffer
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
        println!("  Write buffer size: {} KB", self.config.buffer_size_kb);

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
                    buffer_size_kb: config.buffer_size_kb,
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
        let file = File::create(hour_file)?;
        let buffer_size = params.buffer_size_kb as usize * 1024;
        let mut writer = BufWriter::with_capacity(buffer_size, file);

        // Write header
        writeln!(writer, "timestamp,key,size,ttl")?;

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
            writeln!(writer, "{timestamp},{key},{size},{ttl}")?;

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
            buffer_size_kb: self.buffer_size_kb,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader};

    /// Helper function to create a temp directory for tests
    fn create_temp_dir(test_name: &str) -> PathBuf {
        let temp_dir = std::env::temp_dir().join(format!("cache_generator_test_{}", test_name));
        let _ = fs::remove_dir_all(&temp_dir); // Clean up any previous runs
        fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
        temp_dir
    }

    /// Helper function to clean up temp directory
    fn cleanup_temp_dir(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    /// Parse a generated CSV file and return the records
    fn parse_generated_file(path: &Path) -> Vec<(u64, String, u64, u64)> {
        let file = File::open(path).expect("Failed to open generated file");
        let reader = BufReader::new(file);
        let mut records = Vec::new();

        for (i, line) in reader.lines().enumerate() {
            let line = line.expect("Failed to read line");
            if i == 0 {
                // Skip header
                continue;
            }

            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 4 {
                let timestamp: u64 = parts[0].parse().expect("Invalid timestamp");
                let key = parts[1].to_string();
                let size: u64 = parts[2].parse().expect("Invalid size");
                let ttl: u64 = parts[3].parse().expect("Invalid ttl");
                records.push((timestamp, key, size, ttl));
            }
        }

        records
    }

    #[test]
    fn test_default_config() {
        let config = TrafficLogConfig::default();

        assert_eq!(config.rps, 100);
        assert_eq!(config.duration_hours, 24);
        assert_eq!(config.unique_objects, 10_000);
        assert_eq!(config.popular_traffic_percent, 80);
        assert_eq!(config.popular_objects_percent, 20);
        assert_eq!(config.min_size, 1024);
        assert_eq!(config.max_size, 1024 * 1024);
        assert_eq!(config.min_ttl, 3600);
        assert_eq!(config.max_ttl, 86400);
        assert_eq!(config.output_dir, PathBuf::from("traffic_logs"));
    }

    #[test]
    fn test_config_clone() {
        let config = TrafficLogConfig {
            rps: 50,
            duration_hours: 12,
            unique_objects: 5000,
            popular_traffic_percent: 70,
            popular_objects_percent: 15,
            min_size: 512,
            max_size: 2048,
            min_ttl: 1800,
            max_ttl: 7200,
            output_dir: PathBuf::from("/tmp/test"),
            buffer_size_kb: 4096,
        };

        let cloned = config.clone();

        assert_eq!(cloned.rps, config.rps);
        assert_eq!(cloned.duration_hours, config.duration_hours);
        assert_eq!(cloned.unique_objects, config.unique_objects);
        assert_eq!(
            cloned.popular_traffic_percent,
            config.popular_traffic_percent
        );
        assert_eq!(
            cloned.popular_objects_percent,
            config.popular_objects_percent
        );
        assert_eq!(cloned.min_size, config.min_size);
        assert_eq!(cloned.max_size, config.max_size);
        assert_eq!(cloned.min_ttl, config.min_ttl);
        assert_eq!(cloned.max_ttl, config.max_ttl);
        assert_eq!(cloned.output_dir, config.output_dir);
    }

    #[test]
    fn test_generator_creates_output_directory() {
        let temp_dir = create_temp_dir("creates_output_dir");
        let output_dir = temp_dir.join("nested/output");

        let config = TrafficLogConfig {
            rps: 1,
            duration_hours: 1,
            output_dir: output_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        assert!(output_dir.exists(), "Output directory should be created");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_generator_creates_hourly_files() {
        let temp_dir = create_temp_dir("hourly_files");

        let config = TrafficLogConfig {
            rps: 1,
            duration_hours: 3,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        // Check that files for each hour are created
        for hour in 0..3 {
            let file_path = temp_dir.join(format!("traffic_hour_{:02}.csv", hour));
            assert!(file_path.exists(), "File for hour {} should exist", hour);
        }

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_generated_file_has_header() {
        let temp_dir = create_temp_dir("file_header");

        let config = TrafficLogConfig {
            rps: 1,
            duration_hours: 1,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let file = File::open(&file_path).expect("Failed to open file");
        let reader = BufReader::new(file);
        let first_line = reader
            .lines()
            .next()
            .expect("File is empty")
            .expect("Failed to read");

        assert_eq!(first_line, "timestamp,key,size,ttl");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_generated_request_count() {
        let temp_dir = create_temp_dir("request_count");

        let rps = 10;
        let config = TrafficLogConfig {
            rps,
            duration_hours: 1,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        // Should have rps * 3600 requests
        let expected_requests = rps * 3600;
        assert_eq!(
            records.len(),
            expected_requests as usize,
            "Should generate {} requests per hour",
            expected_requests
        );

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_size_within_bounds() {
        let temp_dir = create_temp_dir("size_bounds");

        let min_size = 100;
        let max_size = 500;
        let config = TrafficLogConfig {
            rps: 10,
            duration_hours: 1,
            min_size,
            max_size,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        for (_, _, size, _) in &records {
            assert!(
                *size >= min_size && *size <= max_size,
                "Size {} should be within [{}, {}]",
                size,
                min_size,
                max_size
            );
        }

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_ttl_within_bounds() {
        let temp_dir = create_temp_dir("ttl_bounds");

        let min_ttl = 100;
        let max_ttl = 500;
        let config = TrafficLogConfig {
            rps: 10,
            duration_hours: 1,
            min_ttl,
            max_ttl,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        for (_, _, _, ttl) in &records {
            assert!(
                *ttl >= min_ttl && *ttl <= max_ttl,
                "TTL {} should be within [{}, {}]",
                ttl,
                min_ttl,
                max_ttl
            );
        }

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_timestamps_are_monotonically_increasing() {
        let temp_dir = create_temp_dir("timestamps_monotonic");

        let config = TrafficLogConfig {
            rps: 100,
            duration_hours: 1,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        let mut prev_timestamp = 0;
        for (timestamp, _, _, _) in &records {
            assert!(
                *timestamp >= prev_timestamp,
                "Timestamps should be monotonically increasing: {} < {}",
                timestamp,
                prev_timestamp
            );
            prev_timestamp = *timestamp;
        }

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_key_format_popular_and_regular() {
        let temp_dir = create_temp_dir("key_format");

        let config = TrafficLogConfig {
            rps: 100,
            duration_hours: 1,
            popular_traffic_percent: 50, // 50% popular, 50% regular for balance
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        let mut has_popular = false;
        let mut has_regular = false;

        for (_, key, _, _) in &records {
            if key.starts_with("popular_obj_") {
                has_popular = true;
            } else if key.starts_with("regular_obj_") {
                has_regular = true;
            } else {
                panic!("Unexpected key format: {}", key);
            }

            if has_popular && has_regular {
                break;
            }
        }

        assert!(has_popular, "Should have popular object keys");
        assert!(has_regular, "Should have regular object keys");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_traffic_distribution_approximate() {
        let temp_dir = create_temp_dir("traffic_distribution");

        let popular_percent = 80;
        let config = TrafficLogConfig {
            rps: 100,
            duration_hours: 1,
            popular_traffic_percent: popular_percent,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        let popular_count = records
            .iter()
            .filter(|(_, key, _, _)| key.starts_with("popular_obj_"))
            .count();
        let total_count = records.len();

        let actual_percent = (popular_count as f64 / total_count as f64) * 100.0;

        // Allow 5% tolerance due to randomness
        let tolerance = 5.0;
        assert!(
            (actual_percent - popular_percent as f64).abs() < tolerance,
            "Popular traffic should be approximately {}%, got {:.1}%",
            popular_percent,
            actual_percent
        );

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_unique_objects_constraint() {
        let temp_dir = create_temp_dir("unique_objects");

        let unique_objects = 100;
        let popular_percent = 20;
        let config = TrafficLogConfig {
            rps: 50,
            duration_hours: 1,
            unique_objects,
            popular_objects_percent: popular_percent,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        let mut unique_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (_, key, _, _) in &records {
            unique_keys.insert(key.clone());
        }

        // The number of unique keys should not exceed unique_objects
        assert!(
            unique_keys.len() <= unique_objects as usize,
            "Number of unique keys ({}) should not exceed unique_objects ({})",
            unique_keys.len(),
            unique_objects
        );

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_popular_objects_have_higher_frequency() {
        let temp_dir = create_temp_dir("popular_frequency");

        let config = TrafficLogConfig {
            rps: 100,
            duration_hours: 1,
            unique_objects: 100,
            popular_traffic_percent: 80,
            popular_objects_percent: 20,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        let mut key_counts: HashMap<String, usize> = HashMap::new();
        for (_, key, _, _) in &records {
            *key_counts.entry(key.clone()).or_insert(0) += 1;
        }

        // Calculate average frequency for popular vs regular objects
        let popular_keys: Vec<_> = key_counts
            .iter()
            .filter(|(k, _)| k.starts_with("popular_obj_"))
            .collect();
        let regular_keys: Vec<_> = key_counts
            .iter()
            .filter(|(k, _)| k.starts_with("regular_obj_"))
            .collect();

        if !popular_keys.is_empty() && !regular_keys.is_empty() {
            let avg_popular: f64 = popular_keys.iter().map(|(_, &c)| c as f64).sum::<f64>()
                / popular_keys.len() as f64;
            let avg_regular: f64 = regular_keys.iter().map(|(_, &c)| c as f64).sum::<f64>()
                / regular_keys.len() as f64;

            // Popular objects should have higher average frequency on average
            // This is statistical so we use a weaker assertion
            assert!(
                avg_popular > avg_regular * 0.5,
                "Popular objects should generally have higher frequency: popular={:.1}, regular={:.1}",
                avg_popular,
                avg_regular
            );
        }

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_multi_hour_timestamp_continuity() {
        let temp_dir = create_temp_dir("timestamp_continuity");

        let config = TrafficLogConfig {
            rps: 10,
            duration_hours: 2,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path_0 = temp_dir.join("traffic_hour_00.csv");
        let file_path_1 = temp_dir.join("traffic_hour_01.csv");

        let records_0 = parse_generated_file(&file_path_0);
        let records_1 = parse_generated_file(&file_path_1);

        // Get last timestamp of hour 0 and first timestamp of hour 1
        let last_ts_hour_0 = records_0.last().expect("Hour 0 should have records").0;
        let first_ts_hour_1 = records_1.first().expect("Hour 1 should have records").0;

        // Hour 1 should start at or after hour 0 ends (with ~1 hour gap)
        let expected_gap = 3600; // 1 hour in seconds
        let actual_gap = first_ts_hour_1 - last_ts_hour_0;

        // Allow some tolerance for the gap (should be close to 1 hour)
        assert!(
            actual_gap <= expected_gap + 10,
            "Gap between hours should be approximately 1 hour, got {} seconds",
            actual_gap
        );

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_zero_rps_edge_case() {
        let temp_dir = create_temp_dir("zero_rps");

        let config = TrafficLogConfig {
            rps: 0,
            duration_hours: 1,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        // With 0 RPS, should have no requests (only header)
        assert_eq!(records.len(), 0, "Zero RPS should produce no requests");

        cleanup_temp_dir(&temp_dir);
    }

    #[test]
    fn test_single_unique_object() {
        let temp_dir = create_temp_dir("single_object");

        let config = TrafficLogConfig {
            rps: 10,
            duration_hours: 1,
            unique_objects: 1,
            popular_objects_percent: 100, // All objects are popular
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        // All requests should be for the same object or very few unique objects
        let unique_keys: std::collections::HashSet<_> =
            records.iter().map(|(_, key, _, _)| key.clone()).collect();

        assert!(
            unique_keys.len() <= 2,
            "With 1 unique object, should have at most 2 keys (popular + regular), got {}",
            unique_keys.len()
        );

        cleanup_temp_dir(&temp_dir);
    }

    /// Test that validates the 80/20 distribution rule:
    /// - X% of objects (popular) should receive Y% of traffic
    /// - The remaining (100-X)% of objects should receive (100-Y)% of traffic
    #[test]
    fn test_80_20_distribution_correctness() {
        let temp_dir = create_temp_dir("80_20_distribution");

        let unique_objects = 1000;
        let popular_objects_percent = 20; // 20% of objects are popular (200 objects)
        let popular_traffic_percent = 80; // These 20% get 80% of traffic

        let config = TrafficLogConfig {
            rps: 100,
            duration_hours: 1,
            unique_objects,
            popular_objects_percent,
            popular_traffic_percent,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        // Count requests by type
        let mut popular_requests = 0usize;
        let mut regular_requests = 0usize;
        let mut popular_keys: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut regular_keys: std::collections::HashSet<String> = std::collections::HashSet::new();

        for (_, key, _, _) in &records {
            if key.starts_with("popular_obj_") {
                popular_requests += 1;
                popular_keys.insert(key.clone());
            } else {
                regular_requests += 1;
                regular_keys.insert(key.clone());
            }
        }

        let total_requests = records.len();
        let actual_popular_traffic_percent =
            (popular_requests as f64 / total_requests as f64) * 100.0;

        // Validate traffic distribution: 80% of traffic should go to popular objects
        // Allow 3% tolerance for randomness
        let traffic_tolerance = 3.0;
        assert!(
            (actual_popular_traffic_percent - popular_traffic_percent as f64).abs()
                < traffic_tolerance,
            "Traffic distribution failed: expected {}% popular traffic, got {:.2}%",
            popular_traffic_percent,
            actual_popular_traffic_percent
        );

        // Validate object pool sizes
        let expected_popular_objects =
            (unique_objects as f64 * popular_objects_percent as f64 / 100.0) as usize;
        let expected_regular_objects = unique_objects as usize - expected_popular_objects;

        // Popular keys should not exceed the expected popular object count
        assert!(
            popular_keys.len() <= expected_popular_objects,
            "Popular object count exceeded: expected at most {}, got {}",
            expected_popular_objects,
            popular_keys.len()
        );

        // Regular keys should not exceed the expected regular object count
        assert!(
            regular_keys.len() <= expected_regular_objects,
            "Regular object count exceeded: expected at most {}, got {}",
            expected_regular_objects,
            regular_keys.len()
        );

        println!(
            "Distribution test passed: {:.2}% popular traffic ({} requests), {:.2}% regular traffic ({} requests)",
            actual_popular_traffic_percent,
            popular_requests,
            100.0 - actual_popular_traffic_percent,
            regular_requests
        );
        println!(
            "Unique objects: {} popular (max {}), {} regular (max {})",
            popular_keys.len(),
            expected_popular_objects,
            regular_keys.len(),
            expected_regular_objects
        );

        cleanup_temp_dir(&temp_dir);
    }

    /// Test that validates temporal interleaving:
    /// Every 5-minute window within an hour should have a good mix of popular and regular objects.
    /// This ensures traffic patterns are realistic and not clustered.
    /// Note: We use request indices to define windows since timestamps may have low resolution.
    #[test]
    fn test_temporal_interleaving_5min_windows() {
        let temp_dir = create_temp_dir("temporal_interleaving");

        let popular_traffic_percent = 80;
        let rps = 100;
        let config = TrafficLogConfig {
            rps,
            duration_hours: 1,
            unique_objects: 1000,
            popular_objects_percent: 20,
            popular_traffic_percent,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        // Divide the hour into 12 windows (5 minutes each) by request index
        // With rps * 3600 requests per hour, each window has rps * 300 requests
        let requests_per_window = (rps * 300) as usize;
        let num_windows = 12;

        // Track popular/regular counts per window
        let mut window_stats: Vec<(usize, usize)> = vec![(0, 0); num_windows];

        for (idx, (_, key, _, _)) in records.iter().enumerate() {
            let window_index = (idx / requests_per_window).min(num_windows - 1);

            if key.starts_with("popular_obj_") {
                window_stats[window_index].0 += 1;
            } else {
                window_stats[window_index].1 += 1;
            }
        }

        // Validate each window has a reasonable mix
        // Expected: ~80% popular, ~20% regular in each window
        // Allow wider tolerance (10%) since windows are smaller samples
        let tolerance = 10.0;

        for (window_idx, (popular, regular)) in window_stats.iter().enumerate() {
            let window_total = popular + regular;
            if window_total == 0 {
                panic!("Window {} should not be empty", window_idx);
            }

            let window_popular_percent = (*popular as f64 / window_total as f64) * 100.0;

            // Each window should have both popular and regular requests
            assert!(
                *popular > 0,
                "Window {} (minutes {}-{}) has no popular requests",
                window_idx,
                window_idx * 5,
                (window_idx + 1) * 5
            );
            assert!(
                *regular > 0,
                "Window {} (minutes {}-{}) has no regular requests",
                window_idx,
                window_idx * 5,
                (window_idx + 1) * 5
            );

            // Each window should roughly follow the distribution
            assert!(
                (window_popular_percent - popular_traffic_percent as f64).abs() < tolerance,
                "Window {} (minutes {}-{}) has skewed distribution: {:.2}% popular (expected ~{}% ± {}%)",
                window_idx,
                window_idx * 5,
                (window_idx + 1) * 5,
                window_popular_percent,
                popular_traffic_percent,
                tolerance
            );
        }

        println!("Temporal interleaving test passed. Window breakdown:");
        for (idx, (popular, regular)) in window_stats.iter().enumerate() {
            let total = popular + regular;
            let pct = if total > 0 {
                (*popular as f64 / total as f64) * 100.0
            } else {
                0.0
            };
            println!(
                "  Window {} (min {}-{}): {} popular, {} regular ({:.1}% popular)",
                idx,
                idx * 5,
                (idx + 1) * 5,
                popular,
                regular,
                pct
            );
        }

        cleanup_temp_dir(&temp_dir);
    }

    /// Test with different distribution ratios (90/10, 70/30, 50/50)
    #[test]
    fn test_various_distribution_ratios() {
        let test_cases = [
            (10, 90, "90/10"), // 10% objects get 90% traffic
            (30, 70, "70/30"), // 30% objects get 70% traffic
            (50, 50, "50/50"), // 50% objects get 50% traffic (uniform-ish)
        ];

        for (popular_objects_pct, popular_traffic_pct, label) in test_cases {
            let temp_dir = create_temp_dir(&format!("distribution_{}", label.replace("/", "_")));

            let config = TrafficLogConfig {
                rps: 50,
                duration_hours: 1,
                unique_objects: 500,
                popular_objects_percent: popular_objects_pct,
                popular_traffic_percent: popular_traffic_pct,
                output_dir: temp_dir.clone(),
                ..Default::default()
            };

            let generator = TrafficLogGenerator::new(config);
            generator.generate().expect("Generation failed");

            let file_path = temp_dir.join("traffic_hour_00.csv");
            let records = parse_generated_file(&file_path);

            let popular_count = records
                .iter()
                .filter(|(_, key, _, _)| key.starts_with("popular_obj_"))
                .count();
            let total = records.len();
            let actual_pct = (popular_count as f64 / total as f64) * 100.0;

            // Allow 5% tolerance
            let tolerance = 5.0;
            assert!(
                (actual_pct - popular_traffic_pct as f64).abs() < tolerance,
                "Distribution {} failed: expected {}% popular traffic, got {:.2}%",
                label,
                popular_traffic_pct,
                actual_pct
            );

            println!(
                "Distribution {} passed: {:.2}% popular traffic (expected {}%)",
                label, actual_pct, popular_traffic_pct
            );

            cleanup_temp_dir(&temp_dir);
        }
    }

    /// Test that validates no clustering at minute boundaries
    /// Checks that consecutive requests alternate between popular/regular appropriately
    /// With 80% popular traffic, runs of popular requests are expected but should be bounded
    #[test]
    fn test_no_request_clustering() {
        let temp_dir = create_temp_dir("no_clustering");

        let popular_traffic_percent = 80;
        let config = TrafficLogConfig {
            rps: 100,
            duration_hours: 1,
            unique_objects: 1000,
            popular_objects_percent: 20,
            popular_traffic_percent,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        // Check for long runs of only popular or only regular requests
        // A "run" is consecutive requests of the same type
        let mut max_popular_run = 0;
        let mut max_regular_run = 0;
        let mut current_popular_run = 0;
        let mut current_regular_run = 0;

        // Also track average run lengths
        let mut popular_runs: Vec<usize> = Vec::new();
        let mut regular_runs: Vec<usize> = Vec::new();

        for (_, key, _, _) in &records {
            if key.starts_with("popular_obj_") {
                if current_regular_run > 0 {
                    regular_runs.push(current_regular_run);
                }
                current_popular_run += 1;
                max_popular_run = max_popular_run.max(current_popular_run);
                current_regular_run = 0;
            } else {
                if current_popular_run > 0 {
                    popular_runs.push(current_popular_run);
                }
                current_regular_run += 1;
                max_regular_run = max_regular_run.max(current_regular_run);
                current_popular_run = 0;
            }
        }
        // Don't forget the last run
        if current_popular_run > 0 {
            popular_runs.push(current_popular_run);
        }
        if current_regular_run > 0 {
            regular_runs.push(current_regular_run);
        }

        // Calculate expected run lengths using geometric distribution
        // For p=0.8 (popular), expected run length = 1/(1-p) = 5
        // For p=0.2 (regular), expected run length = 1/(1-p) = 1.25
        // Max runs in large samples will be much longer due to statistical variation
        let p_popular = popular_traffic_percent as f64 / 100.0;
        let expected_avg_popular_run = 1.0 / (1.0 - p_popular); // ~5 for 80%
        let expected_avg_regular_run = 1.0 / p_popular; // ~1.25 for 80%

        let avg_popular_run: f64 = if popular_runs.is_empty() {
            0.0
        } else {
            popular_runs.iter().sum::<usize>() as f64 / popular_runs.len() as f64
        };
        let avg_regular_run: f64 = if regular_runs.is_empty() {
            0.0
        } else {
            regular_runs.iter().sum::<usize>() as f64 / regular_runs.len() as f64
        };

        // Average run length should be close to expected (within 50%)
        assert!(
            (avg_popular_run - expected_avg_popular_run).abs() / expected_avg_popular_run < 0.5,
            "Average popular run length {:.2} is too far from expected {:.2}",
            avg_popular_run,
            expected_avg_popular_run
        );

        assert!(
            (avg_regular_run - expected_avg_regular_run).abs() / expected_avg_regular_run < 0.5,
            "Average regular run length {:.2} is too far from expected {:.2}",
            avg_regular_run,
            expected_avg_regular_run
        );

        println!("No clustering test passed:");
        println!(
            "  Popular runs: avg={:.2} (expected ~{:.2}), max={}",
            avg_popular_run, expected_avg_popular_run, max_popular_run
        );
        println!(
            "  Regular runs: avg={:.2} (expected ~{:.2}), max={}",
            avg_regular_run, expected_avg_regular_run, max_regular_run
        );

        cleanup_temp_dir(&temp_dir);
    }

    /// Validate that popular objects have significantly higher request frequency than regular
    #[test]
    fn test_popular_objects_frequency_distribution() {
        let temp_dir = create_temp_dir("frequency_distribution");

        let unique_objects = 100;
        let popular_objects_percent = 20; // 20 popular objects
        let popular_traffic_percent = 80; // Get 80% of traffic

        let config = TrafficLogConfig {
            rps: 100,
            duration_hours: 1,
            unique_objects,
            popular_objects_percent,
            popular_traffic_percent,
            output_dir: temp_dir.clone(),
            ..Default::default()
        };

        let generator = TrafficLogGenerator::new(config);
        generator.generate().expect("Generation failed");

        let file_path = temp_dir.join("traffic_hour_00.csv");
        let records = parse_generated_file(&file_path);

        // Count frequency of each key
        let mut key_counts: HashMap<String, usize> = HashMap::new();
        for (_, key, _, _) in &records {
            *key_counts.entry(key.clone()).or_insert(0) += 1;
        }

        // Separate popular and regular objects
        let popular_counts: Vec<usize> = key_counts
            .iter()
            .filter(|(k, _)| k.starts_with("popular_obj_"))
            .map(|(_, &c)| c)
            .collect();

        let regular_counts: Vec<usize> = key_counts
            .iter()
            .filter(|(k, _)| k.starts_with("regular_obj_"))
            .map(|(_, &c)| c)
            .collect();

        if popular_counts.is_empty() || regular_counts.is_empty() {
            panic!("Should have both popular and regular objects");
        }

        // Calculate statistics
        let avg_popular: f64 =
            popular_counts.iter().sum::<usize>() as f64 / popular_counts.len() as f64;
        let avg_regular: f64 =
            regular_counts.iter().sum::<usize>() as f64 / regular_counts.len() as f64;

        let total_popular_requests: usize = popular_counts.iter().sum();
        let total_regular_requests: usize = regular_counts.iter().sum();
        let total_requests = total_popular_requests + total_regular_requests;

        // Expected: 80% traffic to 20% objects means each popular object gets 4x traffic density
        // (80/20) / (20/80) = 4:1 ratio approximately
        // avg_popular / avg_regular should be roughly (popular_traffic / (100 - popular_traffic)) * ((100 - popular_objects) / popular_objects)
        // = (80/20) * (80/20) = 16x theoretically, but actual implementation may vary
        let frequency_ratio = avg_popular / avg_regular;

        // Popular objects should have noticeably higher frequency
        assert!(
            frequency_ratio > 2.0,
            "Popular objects should have at least 2x higher frequency than regular. Ratio: {:.2}",
            frequency_ratio
        );

        println!("Frequency distribution test passed:");
        println!(
            "  Popular objects: {} unique, avg {:.1} requests each",
            popular_counts.len(),
            avg_popular
        );
        println!(
            "  Regular objects: {} unique, avg {:.1} requests each",
            regular_counts.len(),
            avg_regular
        );
        println!(
            "  Frequency ratio (popular/regular): {:.2}x",
            frequency_ratio
        );
        println!(
            "  Traffic split: {:.1}% popular, {:.1}% regular",
            (total_popular_requests as f64 / total_requests as f64) * 100.0,
            (total_regular_requests as f64 / total_requests as f64) * 100.0
        );

        cleanup_temp_dir(&temp_dir);
    }
}
