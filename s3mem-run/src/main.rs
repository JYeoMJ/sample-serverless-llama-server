// Import required crates and modules
use anyhow::{Context, Result};                // Error handling with context
use aws_config::BehaviorVersion;              // AWS SDK configuration
use aws_sdk_s3::Client;                       // AWS S3 client
use clap::Parser;                             // Command-line argument parsing
use libc::{ftruncate, memfd_create};          // Linux system calls for memory file operations
use std::env;                                 // Environment variable access
use std::ffi::CString;                        // C-compatible strings for FFI
use std::io::{Seek, SeekFrom, Write};         // I/O operations
use std::os::unix::io::FromRawFd;             // Unix-specific file descriptor handling
use std::os::unix::process::CommandExt;       // Unix-specific process extensions
use std::path::PathBuf;                       // Path manipulation
use std::process::Command;                    // Process execution
use std::sync::Arc;                           // Thread-safe reference counting
use tokio::sync::Semaphore;                   // Async concurrency limiting
use tracing::{debug, error, info, instrument, Level};  // Structured logging
use tracing_subscriber::{EnvFilter, FmtSubscriber};    // Logging configuration

// Default values that can be overridden based on file size
// These constants control the download behavior and are tuned for optimal performance
const MIN_CHUNK_SIZE: i64 = 4 * 1024 * 1024;      // 4MB minimum chunk size
const MAX_CHUNK_SIZE: i64 = 128 * 1024 * 1024;    // 128MB maximum chunk size
const MIN_CONCURRENT_DOWNLOADS: usize = 4;         // Minimum number of parallel downloads
const MAX_CONCURRENT_DOWNLOADS: usize = 16;        // Maximum number of parallel downloads
const TARGET_CHUNKS_PER_FILE: i64 = 75;           // Target ~75 chunks per file for balanced parallelism

#[derive(Parser, Debug)]
#[command(name = "s3mem-run")]
#[command(about = "A Rust utility that downloads large files from Amazon S3 into memory and executes programs with the memory file descriptor")]
struct Args {
    /// S3 bucket containing the file (defaults to S3_BUCKET env var)
    #[arg(long, env = "S3_BUCKET")]
    bucket: Option<String>,

    /// S3 key (defaults to S3_KEY env var)
    #[arg(long, env = "S3_KEY")]
    key: Option<String>,

    /// Placeholder for memfd (defaults to '{{memfd}}')
    /// This string will be replaced with the actual memory file path in command arguments
    #[arg(long, env = "MEMFD_PLACEHOLDER", default_value = "{{memfd}}")]
    memfd_placeholder: String,
    
    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: Level,

    /// Program to execute and its arguments
    /// The first argument is the program path, followed by its arguments
    #[arg(trailing_var_arg = true, required = true)]
    command: Vec<String>,
}

// Calculate optimal chunk size based on file size
// This function determines the best chunk size for downloading based on the total file size
// Larger files use larger chunks to reduce the number of S3 requests
fn calculate_optimal_chunk_size(file_size: i64) -> i64 {
    // Target a reasonable number of chunks based on file size
    let ideal_chunk_size = file_size / TARGET_CHUNKS_PER_FILE;
    
    // Clamp to our min/max boundaries to ensure we don't have too small or too large chunks
    ideal_chunk_size.clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE)
}

// Calculate optimal concurrency based on file size
// This function determines how many parallel downloads to use based on file size
// Larger files benefit from more parallelism up to a point
fn calculate_optimal_concurrency(file_size: i64) -> usize {
    // For smaller files, use fewer concurrent downloads
    // For larger files, scale up to the maximum
    let size_gb = file_size as f64 / (1024.0 * 1024.0 * 1024.0);
    
    // Scale concurrency linearly from MIN to MAX based on file size from 0.5GB to 10GB
    let concurrency = if size_gb <= 0.5 {
        MIN_CONCURRENT_DOWNLOADS
    } else if size_gb >= 10.0 {
        MAX_CONCURRENT_DOWNLOADS
    } else {
        // Linear interpolation between min and max
        let scale_factor = (size_gb - 0.5) / 9.5; // 0.5GB to 10GB range = 9.5GB
        let range = MAX_CONCURRENT_DOWNLOADS - MIN_CONCURRENT_DOWNLOADS;
        MIN_CONCURRENT_DOWNLOADS + (scale_factor * range as f64).round() as usize
    };
    
    concurrency
}

// MemFile represents a file that exists only in memory
// This is the core data structure that allows us to avoid disk I/O
struct MemFile {
    file: std::fs::File,  // Standard file handle for I/O operations
    fd: i32,              // Raw file descriptor for passing to other processes
}

impl MemFile {
    // Create a new memory-backed file using memfd_create
    fn new(name: &str) -> Result<Self> {
        // Convert Rust string to C string for the system call
        let name = CString::new(name)?;
        
        // Create an in-memory file using the Linux-specific memfd_create syscall
        // This creates a file that exists only in memory, not on disk
        let fd = unsafe { memfd_create(name.as_ptr(), 0) };

        if fd == -1 {
            return Err(std::io::Error::last_os_error()).context("Failed to create memfd");
        }

        // Convert the raw file descriptor to a Rust File object for easier handling
        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        Ok(MemFile { file, fd })
    }

    // Write data at a specific offset in the memory file
    // This is used to write downloaded chunks directly to their correct position
    fn write_at(&mut self, data: &[u8], offset: u64) -> Result<()> {
        // Seek to the specified position in the file
        self.file
            .seek(SeekFrom::Start(offset))
            .context("Failed to seek in memfd")?;
            
        // Write the data at that position
        self.file
            .write_all(data)
            .context("Failed to write to memfd")?;
        Ok(())
    }
}

#[instrument(skip(client))]
// Download a single chunk of the file from S3
// This function is called in parallel for different chunks of the file
async fn download_chunk(
    client: &Client,
    bucket: &str,
    key: &str,
    start: i64,
    end: i64,
) -> Result<(Vec<u8>, u64)> {
    // Format the byte range header for the S3 request
    let range = format!("bytes={}-{}", start, end);
    debug!(range, "Downloading chunk");

    // Make the S3 GetObject request with the byte range
    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .range(range)
        .send()
        .await
        .context("Failed to get object from S3")?;

    // Collect the streaming response body into a byte vector
    let data = resp
        .body
        .collect()
        .await
        .context("Failed to collect response body")?;
    
    // Convert to a standard Vec<u8> and log the chunk size
    let bytes = data.to_vec();
    let chunk_size = bytes.len();
    debug!(bytes = chunk_size, offset = start, "Chunk downloaded successfully");
    
    // Return both the data and the offset where it should be written
    Ok((bytes, start as u64))
}

#[instrument(skip(client))]
// Download a file from S3 in parallel chunks directly into memory
// This is the main function that orchestrates the parallel download process
async fn parallel_download_to_memfd(bucket: &str, key: &str, client: &Client) -> Result<MemFile> {
    // First, get the object metadata to determine file size
    info!("Getting object metadata from S3");
    let head_object = client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .context("Failed to get object metadata from S3")?;

    // Extract the total file size from the metadata
    let total_size = head_object
        .content_length
        .context("Content length not available")? as i64;

    // Calculate optimal chunk size based on file size
    let chunk_size = calculate_optimal_chunk_size(total_size);
    
    // Calculate optimal concurrency based on file size
    let concurrent_downloads = calculate_optimal_concurrency(total_size);
    
    // Log the download parameters for monitoring and debugging
    info!(
        file_size_bytes = total_size,
        file_size_mb = total_size / (1024 * 1024),
        chunk_size_bytes = chunk_size,
        chunk_size_mb = chunk_size / (1024 * 1024),
        concurrent_downloads = concurrent_downloads,
        "Download parameters calculated"
    );

    // Create a memory file to hold the downloaded data
    debug!("Creating memory file");
    let mut memfile = MemFile::new("s3_file")?;
    
    // Pre-allocate the full file size in memory to avoid resizing during writes
    if unsafe { ftruncate(memfile.fd, total_size) } == -1 {
        return Err(std::io::Error::last_os_error()).context("Failed to set file size");
    }

    // Create a semaphore to limit concurrent downloads
    let semaphore = Arc::new(Semaphore::new(concurrent_downloads));
    let mut tasks = Vec::new();

    // Calculate chunk boundaries and spawn download tasks
    let mut start = 0i64;
    let total_chunks = (total_size + chunk_size - 1) / chunk_size;
    let mut chunk_count = 0;
    
    info!(total_chunks, "Starting parallel download");
    
    // Spawn tasks for each chunk
    while start < total_size {
        chunk_count += 1;
        // Calculate the end byte for this chunk (inclusive)
        let end = (start + chunk_size - 1).min(total_size - 1);
        
        // Clone references for the async task
        let client = client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        
        // Acquire a permit from the semaphore to limit concurrency
        let permit = semaphore.clone().acquire_owned().await?;
        
        debug!(
            chunk_number = chunk_count,
            total_chunks = total_chunks,
            start_byte = start,
            end_byte = end,
            "Scheduling chunk download"
        );

        // Spawn an async task to download this chunk
        let task = tokio::spawn(async move {
            // Download the chunk and release the semaphore permit when done
            let result = download_chunk(&client, &bucket, &key, start, end).await;
            drop(permit);
            result
        });

        tasks.push(task);
        start = end + 1;  // Move to the next chunk
    }

    info!(total_chunks = tasks.len(), "All chunks scheduled, waiting for completion");
    
    // Wait for all download tasks to complete and write their data to the memory file
    let mut completed_chunks = 0;
    for task in tasks {
        completed_chunks += 1;
        // Await the task completion and extract the data and offset
        let (data, offset) = task
            .await
            .context("Task join failed")?
            .context("Chunk download failed")?;
            
        debug!(
            completed = completed_chunks,
            total = total_chunks,
            progress_percent = (completed_chunks as f64 / total_chunks as f64 * 100.0) as u32,
            "Writing chunk to memory file"
        );
        
        // Write the chunk data to the memory file at the correct offset
        memfile.write_at(&data, offset)?;
        
        // Log progress periodically
        if completed_chunks % 10 == 0 || completed_chunks == total_chunks {
            info!(
                completed_chunks,
                total_chunks,
                progress_percent = (completed_chunks as f64 / total_chunks as f64 * 100.0) as u32,
                "Download progress"
            );
        }
    }

    info!("Download completed successfully");
    Ok(memfile)
}

#[instrument(skip(client))]
// Create a memory file descriptor, download the file, and execute the specified program
// This is the main function that ties everything together
async fn create_memfd_and_exec(
    bucket: &str,
    key: &str,
    client: &Client,
    program: &str,
    args: &[String],
    memfd_placeholder: &str,
) -> Result<()> {
    info!(bucket, key, program, "Starting download and execution process");
    
    // Download the file from S3 into memory
    let memfile = parallel_download_to_memfd(bucket, key, client).await?;
    
    // Get the path to the memory file descriptor
    // This is a special path in /proc that points to the memory file
    let memfd_path = format!("/proc/self/fd/{}", memfile.fd);

    // Set the environment variable with memfd_path for programs that might use it
    env::set_var("MEMFD_PATH", &memfd_path);
    debug!(memfd_path, "Set MEMFD_PATH environment variable");

    // Replace placeholder with actual memfd path in all command arguments
    // This allows the target program to access the memory file
    let final_args: Vec<String> = args
        .iter()
        .map(|arg| arg.replace(memfd_placeholder, &memfd_path))
        .collect();
    
    debug!(
        program,
        args = ?final_args,
        "Preparing to execute program with memory file descriptor"
    );

    // Prevent the memory file from being dropped when this function returns
    // This ensures the file descriptor remains valid for the child process
    std::mem::forget(memfile);
    
    info!("Executing program: {}", program);

    // Create a new command to execute the target program
    let mut cmd = Command::new(program);
    cmd.args(final_args);

    // Execute the command, replacing the current process
    // This will only return if there's an error
    Err(cmd.exec().into())
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = Args::parse();
    
    // Initialize the tracing subscriber for structured logging
    // This sets up the logging system with the specified log level
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_max_level(args.log_level)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
    
    // Log the start of the program with version information
    info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting s3mem-run"
    );

    // Get the S3 bucket name from arguments or environment variables
    let bucket = args.bucket.ok_or_else(|| {
        error!("S3_BUCKET environment variable not set and --bucket not provided");
        anyhow::anyhow!("S3_BUCKET environment variable not set and --bucket not provided")
    })?;

    // Get the S3 key from arguments or environment variables
    let key = args.key.ok_or_else(|| {
        error!("S3_KEY environment variable not set and --key not provided");
        anyhow::anyhow!("S3_KEY environment variable not set and --key not provided")
    })?;

    // Get the program to execute (first element of command vector)
    let program = &args.command[0];
    let program_path = PathBuf::from(program);

    // Verify that the program exists
    if !program_path.exists() {
        error!(program, "Program does not exist");
        return Err(anyhow::anyhow!("Program '{}' does not exist", program));
    }

    // Get program arguments (everything after the program name)
    let program_args: Vec<String> = args.command[1..].to_vec();

    // Log the configuration for debugging
    info!(
        bucket,
        key,
        program,
        args = ?program_args,
        log_level = ?args.log_level,
        "Configuration loaded"
    );

    // Initialize the AWS S3 client
    debug!("Initializing AWS client");
    let config = aws_config::defaults(BehaviorVersion::latest()).load().await;
    let client = Client::new(&config);
    debug!("AWS client initialized");

    // Download the file and execute the program
    create_memfd_and_exec(
        &bucket,
        &key,
        &client,
        program,
        &program_args,
        &args.memfd_placeholder,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_args_parsing() {
        // Test command-line argument parsing
        let args = Args::try_parse_from([
            "s3mem-run",
            "--bucket",
            "test-bucket",
            "--key",
            "test-key",
            "--log-level",
            "debug",
            "program",
            "arg1",
            "arg2",
        ])
        .unwrap();

        assert_eq!(args.bucket.unwrap(), "test-bucket");
        assert_eq!(args.key.unwrap(), "test-key");
        assert_eq!(args.log_level, Level::DEBUG);
        assert_eq!(args.command, vec!["program", "arg1", "arg2"]);
        assert_eq!(args.memfd_placeholder, "{{memfd}}");
    }

    #[test]
    fn test_memfile_creation() {
        let memfile = MemFile::new("test_file").unwrap();
        assert!(memfile.fd > 0);
    }

    #[test]
    fn test_memfile_write() {
        let mut memfile = MemFile::new("test_file").unwrap();
        let test_data = b"Hello, World!";
        memfile.write_at(test_data, 0).unwrap();

        // Verify the write by reading back
        use std::io::Read;
        let mut buffer = Vec::new();
        memfile.file.seek(SeekFrom::Start(0)).unwrap();
        memfile.file.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, test_data);
    }

    #[test]
    fn test_args_missing_required() {
        // Test that required arguments are enforced
        let result = Args::try_parse_from(["s3mem-run"]);
        assert!(result.is_err());
    }

    #[test]
    fn test_memfd_placeholder_replacement() {
        let args = Args::try_parse_from([
            "s3mem-run",
            "--bucket",
            "test-bucket",
            "--key",
            "test-key",
            "--memfd-placeholder",
            "{{custom}}",
            "program",
            "{{custom}}",
            "arg2",
        ])
        .unwrap();

        assert_eq!(args.memfd_placeholder, "{{custom}}");
        let program_args: Vec<String> = args.command[1..].to_vec();
        assert!(program_args.contains(&"{{custom}}".to_string()));
    }
    
    #[test]
    fn test_calculate_optimal_chunk_size() {
        // Test with small file (512MB)
        let small_file_size = 512 * 1024 * 1024;
        let small_chunk_size = calculate_optimal_chunk_size(small_file_size);
        assert!(small_chunk_size >= MIN_CHUNK_SIZE);
        assert!(small_chunk_size <= MAX_CHUNK_SIZE);
        
        // Test with medium file (2GB)
        let medium_file_size = 2 * 1024 * 1024 * 1024;
        let medium_chunk_size = calculate_optimal_chunk_size(medium_file_size);
        assert!(medium_chunk_size >= MIN_CHUNK_SIZE);
        assert!(medium_chunk_size <= MAX_CHUNK_SIZE);
        
        // Test with large file (10GB)
        let large_file_size = 10 * 1024 * 1024 * 1024;
        let large_chunk_size = calculate_optimal_chunk_size(large_file_size);
        assert!(large_chunk_size >= MIN_CHUNK_SIZE);
        assert!(large_chunk_size <= MAX_CHUNK_SIZE);
        
        // Verify that larger files get larger chunks
        assert!(large_chunk_size > small_chunk_size);
    }
    
    #[test]
    fn test_calculate_optimal_concurrency() {
        // Test with small file (512MB)
        let small_file_size = 512 * 1024 * 1024;
        let small_concurrency = calculate_optimal_concurrency(small_file_size);
        assert_eq!(small_concurrency, MIN_CONCURRENT_DOWNLOADS);
        
        // Test with large file (10GB)
        let large_file_size = 10 * 1024 * 1024 * 1024;
        let large_concurrency = calculate_optimal_concurrency(large_file_size);
        assert_eq!(large_concurrency, MAX_CONCURRENT_DOWNLOADS);
        
        // Test with medium file (5GB) - should be somewhere in between
        let medium_file_size = 5 * 1024 * 1024 * 1024;
        let medium_concurrency = calculate_optimal_concurrency(medium_file_size);
        assert!(medium_concurrency > MIN_CONCURRENT_DOWNLOADS);
        assert!(medium_concurrency < MAX_CONCURRENT_DOWNLOADS);
    }
}