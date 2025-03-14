use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_sdk_s3::Client;
use clap::Parser;
use libc::{ftruncate, memfd_create};
use std::env;
use std::ffi::CString;
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::io::FromRawFd;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, error, info, instrument, Level};
use tracing_subscriber::{EnvFilter, FmtSubscriber};

// Default values that can be overridden based on file size
const MIN_CHUNK_SIZE: i64 = 4 * 1024 * 1024; // 4MB minimum
const MAX_CHUNK_SIZE: i64 = 128 * 1024 * 1024; // 128MB maximum
const MIN_CONCURRENT_DOWNLOADS: usize = 4;
const MAX_CONCURRENT_DOWNLOADS: usize = 16;
const TARGET_CHUNKS_PER_FILE: i64 = 75; // Target ~75 chunks per file for balanced parallelism

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
    #[arg(long, env = "MEMFD_PLACEHOLDER", default_value = "{{memfd}}")]
    memfd_placeholder: String,
    
    /// Log level (trace, debug, info, warn, error)
    #[arg(long, default_value = "info")]
    log_level: Level,

    /// Program to execute and its arguments
    #[arg(trailing_var_arg = true, required = true)]
    command: Vec<String>,
}

// Calculate optimal chunk size based on file size
fn calculate_optimal_chunk_size(file_size: i64) -> i64 {
    // Target a reasonable number of chunks based on file size
    let ideal_chunk_size = file_size / TARGET_CHUNKS_PER_FILE;
    
    // Clamp to our min/max boundaries
    ideal_chunk_size.clamp(MIN_CHUNK_SIZE, MAX_CHUNK_SIZE)
}

// Calculate optimal concurrency based on file size
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

struct MemFile {
    file: std::fs::File,
    fd: i32,
}

impl MemFile {
    fn new(name: &str) -> Result<Self> {
        let name = CString::new(name)?;
        let fd = unsafe { memfd_create(name.as_ptr(), 0) };

        if fd == -1 {
            return Err(std::io::Error::last_os_error()).context("Failed to create memfd");
        }

        let file = unsafe { std::fs::File::from_raw_fd(fd) };
        Ok(MemFile { file, fd })
    }

    fn write_at(&mut self, data: &[u8], offset: u64) -> Result<()> {
        self.file
            .seek(SeekFrom::Start(offset))
            .context("Failed to seek in memfd")?;
        self.file
            .write_all(data)
            .context("Failed to write to memfd")?;
        Ok(())
    }
}

#[instrument(skip(client))]
async fn download_chunk(
    client: &Client,
    bucket: &str,
    key: &str,
    start: i64,
    end: i64,
) -> Result<(Vec<u8>, u64)> {
    let range = format!("bytes={}-{}", start, end);
    debug!(range, "Downloading chunk");

    let resp = client
        .get_object()
        .bucket(bucket)
        .key(key)
        .range(range)
        .send()
        .await
        .context("Failed to get object from S3")?;

    let data = resp
        .body
        .collect()
        .await
        .context("Failed to collect response body")?;
    
    let bytes = data.to_vec();
    let chunk_size = bytes.len();
    debug!(bytes = chunk_size, offset = start, "Chunk downloaded successfully");
    Ok((bytes, start as u64))
}

#[instrument(skip(client))]
async fn parallel_download_to_memfd(bucket: &str, key: &str, client: &Client) -> Result<MemFile> {
    info!("Getting object metadata from S3");
    let head_object = client
        .head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await
        .context("Failed to get object metadata from S3")?;

    let total_size = head_object
        .content_length
        .context("Content length not available")? as i64;

    // Calculate optimal chunk size based on file size
    let chunk_size = calculate_optimal_chunk_size(total_size);
    
    // Calculate optimal concurrency based on file size
    let concurrent_downloads = calculate_optimal_concurrency(total_size);
    
    info!(
        file_size_bytes = total_size,
        file_size_mb = total_size / (1024 * 1024),
        chunk_size_bytes = chunk_size,
        chunk_size_mb = chunk_size / (1024 * 1024),
        concurrent_downloads = concurrent_downloads,
        "Download parameters calculated"
    );

    debug!("Creating memory file");
    let mut memfile = MemFile::new("s3_file")?;
    if unsafe { ftruncate(memfile.fd, total_size) } == -1 {
        return Err(std::io::Error::last_os_error()).context("Failed to set file size");
    }

    let semaphore = Arc::new(Semaphore::new(concurrent_downloads));
    let mut tasks = Vec::new();

    let mut start = 0i64;
    let total_chunks = (total_size + chunk_size - 1) / chunk_size;
    let mut chunk_count = 0;
    
    info!(total_chunks, "Starting parallel download");
    
    while start < total_size {
        chunk_count += 1;
        let end = (start + chunk_size - 1).min(total_size - 1);
        let client = client.clone();
        let bucket = bucket.to_string();
        let key = key.to_string();
        let permit = semaphore.clone().acquire_owned().await?;
        
        debug!(
            chunk_number = chunk_count,
            total_chunks = total_chunks,
            start_byte = start,
            end_byte = end,
            "Scheduling chunk download"
        );

        let task = tokio::spawn(async move {
            let result = download_chunk(&client, &bucket, &key, start, end).await;
            drop(permit);
            result
        });

        tasks.push(task);
        start = end + 1;
    }

    info!(total_chunks = tasks.len(), "All chunks scheduled, waiting for completion");
    
    let mut completed_chunks = 0;
    for task in tasks {
        completed_chunks += 1;
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
        
        memfile.write_at(&data, offset)?;
        
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
async fn create_memfd_and_exec(
    bucket: &str,
    key: &str,
    client: &Client,
    program: &str,
    args: &[String],
    memfd_placeholder: &str,
) -> Result<()> {
    info!(bucket, key, program, "Starting download and execution process");
    
    let memfile = parallel_download_to_memfd(bucket, key, client).await?;
    let memfd_path = format!("/proc/self/fd/{}", memfile.fd);

    // Set the environment variable with memfd_path
    env::set_var("MEMFD_PATH", &memfd_path);
    debug!(memfd_path, "Set MEMFD_PATH environment variable");

    // Replace placeholder with actual memfd path in arguments
    let final_args: Vec<String> = args
        .iter()
        .map(|arg| arg.replace(memfd_placeholder, &memfd_path))
        .collect();
    
    debug!(
        program,
        args = ?final_args,
        "Preparing to execute program with memory file descriptor"
    );

    std::mem::forget(memfile);
    info!("Executing program: {}", program);

    let mut cmd = Command::new(program);
    cmd.args(final_args);

    Err(cmd.exec().into())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    // Initialize the tracing subscriber
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(EnvFilter::from_default_env())
        .with_max_level(args.log_level)
        .finish();
    
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");
    
    info!(
        version = env!("CARGO_PKG_VERSION"),
        "Starting s3mem-run"
    );

    let bucket = args.bucket.ok_or_else(|| {
        error!("S3_BUCKET environment variable not set and --bucket not provided");
        anyhow::anyhow!("S3_BUCKET environment variable not set and --bucket not provided")
    })?;

    let key = args.key.ok_or_else(|| {
        error!("S3_KEY environment variable not set and --key not provided");
        anyhow::anyhow!("S3_KEY environment variable not set and --key not provided")
    })?;

    let program = &args.command[0];
    let program_path = PathBuf::from(program);

    if !program_path.exists() {
        error!(program, "Program does not exist");
        return Err(anyhow::anyhow!("Program '{}' does not exist", program));
    }

    // Get program arguments
    let program_args: Vec<String> = args.command[1..].to_vec();

    info!(
        bucket,
        key,
        program,
        args = ?program_args,
        log_level = ?args.log_level,
        "Configuration loaded"
    );

    debug!("Initializing AWS client");
    let config = aws_config::defaults(BehaviorVersion::latest()).load().await;
    let client = Client::new(&config);
    debug!("AWS client initialized");

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