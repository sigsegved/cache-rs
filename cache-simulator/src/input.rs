//! Input data handling for cache simulation
//!
//! Provides utilities to parse cache request logs from CSV files
//! and other supported formats. Supports both batch and streaming modes.

use crate::models::Request;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

/// Error types for log parsing
#[derive(Debug)]
pub enum LogParseError {
    #[allow(dead_code)]
    IoError(io::Error), // Keeping io::Error for proper error conversion
    #[allow(dead_code)]
    ParseError(String), // Keeping String for error details
}

impl From<io::Error> for LogParseError {
    fn from(err: io::Error) -> Self {
        LogParseError::IoError(err)
    }
}

/// Reader for cache request logs
pub struct LogReader {
    input_dir: PathBuf,
}

impl LogReader {
    /// Create a new reader for the given input directory
    pub fn new<P: AsRef<Path>>(input_dir: P) -> Self {
        Self {
            input_dir: input_dir.as_ref().to_path_buf(),
        }
    }

    /// Get all log files in the input directory, sorted by name
    pub fn get_log_files(&self) -> io::Result<Vec<PathBuf>> {
        let entries = fs::read_dir(&self.input_dir)?;

        let mut log_files = Vec::new();
        for entry in entries {
            let path = entry?.path();
            if path.is_file() {
                // Consider both .log and .csv files
                if let Some(ext) = path.extension() {
                    if ext == "log" || ext == "csv" || ext == "txt" {
                        log_files.push(path);
                    }
                }
            }
        }

        // Sort files by name for consistent ordering
        log_files.sort();

        Ok(log_files)
    }

    /// Parse a single line into a Request
    /// Optimized to avoid allocations where possible
    fn parse_line(line: &str, line_num: usize) -> Result<Option<Request>, LogParseError> {
        let line = line.trim();

        // Skip empty lines, comments, and header row
        if line.is_empty() || line.starts_with('#') || (line_num == 0 && line.contains("timestamp"))
        {
            return Ok(None);
        }

        // Parse without allocating a Vec - find comma positions directly
        let mut parts = line.splitn(4, ',');

        // Parse timestamp (unix seconds)
        let ts_str = parts.next().ok_or_else(|| {
            LogParseError::ParseError(format!("Line {} missing timestamp", line_num + 1))
        })?;
        let timestamp = ts_str.trim().parse::<u64>().map_err(|_| {
            LogParseError::ParseError(format!(
                "Invalid timestamp in line {}: {}",
                line_num + 1,
                ts_str
            ))
        })?;
        let timestamp = UNIX_EPOCH + Duration::from_secs(timestamp);

        // Parse cache key
        let key_str = parts.next().ok_or_else(|| {
            LogParseError::ParseError(format!("Line {} missing key", line_num + 1))
        })?;
        let key = key_str.trim().to_string();

        // Parse object size
        let size_str = parts.next().ok_or_else(|| {
            LogParseError::ParseError(format!("Line {} missing size", line_num + 1))
        })?;
        let size = size_str.trim().parse::<usize>().map_err(|_| {
            LogParseError::ParseError(format!(
                "Invalid size in line {}: {}",
                line_num + 1,
                size_str
            ))
        })?;

        // Parse TTL (optional, default to 0)
        let ttl = if let Some(ttl_str) = parts.next() {
            ttl_str.trim().parse::<u64>().unwrap_or(0)
        } else {
            0
        };

        Ok(Some(Request::new(timestamp, key, size, ttl)))
    }

    /// Parse a single log file (legacy batch mode)
    #[allow(dead_code)]
    pub fn parse_file<P: AsRef<Path>>(&self, path: P) -> Result<Vec<Request>, LogParseError> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut requests = Vec::new();

        for (line_num, line) in reader.lines().enumerate() {
            let line = line?;
            if let Some(request) = Self::parse_line(&line, line_num)? {
                requests.push(request);
            }
        }

        Ok(requests)
    }

    /// Parse all log files in the input directory (legacy batch mode)
    #[allow(dead_code)]
    pub fn parse_all_files(&self) -> Result<Vec<Request>, LogParseError> {
        let log_files = self.get_log_files()?;
        let mut all_requests = Vec::new();

        for file in log_files {
            let file_requests = self.parse_file(&file)?;
            all_requests.extend(file_requests);
        }

        // Sort all requests by timestamp
        all_requests.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(all_requests)
    }

    /// Create a streaming iterator over all requests in all log files.
    /// This processes one request at a time without loading everything into memory.
    pub fn stream_requests(&self) -> Result<RequestIterator, LogParseError> {
        let log_files = self.get_log_files()?;
        Ok(RequestIterator::new(log_files))
    }
}

/// Iterator that streams requests from multiple log files without loading all into memory
pub struct RequestIterator {
    files: Vec<PathBuf>,
    current_file_index: usize,
    current_reader: Option<BufReader<File>>,
    current_line_num: usize,
    line_buffer: String,
}

impl RequestIterator {
    fn new(files: Vec<PathBuf>) -> Self {
        Self {
            files,
            current_file_index: 0,
            current_reader: None,
            current_line_num: 0,
            line_buffer: String::with_capacity(256),
        }
    }

    /// Open the next file for reading
    fn open_next_file(&mut self) -> io::Result<bool> {
        if self.current_file_index >= self.files.len() {
            return Ok(false);
        }

        let file = File::open(&self.files[self.current_file_index])?;
        // Use 1MB buffer for better I/O performance
        self.current_reader = Some(BufReader::with_capacity(1024 * 1024, file));
        self.current_line_num = 0;
        self.current_file_index += 1;
        Ok(true)
    }

    /// Reset the iterator to start from the beginning
    #[allow(dead_code)]
    pub fn reset(&mut self) -> io::Result<()> {
        self.current_file_index = 0;
        self.current_reader = None;
        self.current_line_num = 0;
        Ok(())
    }
}

impl Iterator for RequestIterator {
    type Item = Result<Request, LogParseError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // If we don't have a reader, try to open the next file
            if self.current_reader.is_none() {
                match self.open_next_file() {
                    Ok(true) => {}
                    Ok(false) => return None, // No more files
                    Err(e) => return Some(Err(LogParseError::IoError(e))),
                }
            }

            // Try to read the next line
            if let Some(reader) = &mut self.current_reader {
                self.line_buffer.clear();
                match reader.read_line(&mut self.line_buffer) {
                    Ok(0) => {
                        // EOF on current file, move to next
                        self.current_reader = None;
                        continue;
                    }
                    Ok(_) => {
                        let line_num = self.current_line_num;
                        self.current_line_num += 1;

                        // Parse the line
                        match LogReader::parse_line(&self.line_buffer, line_num) {
                            Ok(Some(request)) => return Some(Ok(request)),
                            Ok(None) => continue, // Skip empty/header lines
                            Err(e) => return Some(Err(e)),
                        }
                    }
                    Err(e) => return Some(Err(LogParseError::IoError(e))),
                }
            }
        }
    }
}
