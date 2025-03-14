# s3mem-run

A Unix-style utility that downloads files from Amazon S3 directly into memory and executes programs with the memory file descriptor.

## Overview

`s3mem-run` solves a common problem in serverless environments: loading large files (like ML models) without hitting disk space limitations. It downloads files from S3 directly into memory using Linux's `memfd_create` system call and then executes a specified program, passing the memory file descriptor to it.

## Features

- **Direct Memory Loading**: Downloads files from S3 directly into memory without touching disk
- **Adaptive Parallel Downloads**: Optimizes chunk size and concurrency based on file size
  - Chunk sizes from 4MB to 128MB depending on file size
  - Concurrency from 4 to 16 parallel downloads based on file size
- **Structured Logging**: Uses tracing for comprehensive, level-based logging
- **Memory File Descriptor**: Creates a memory-based file descriptor that can be passed to other applications
- **Placeholder Substitution**: Replaces a placeholder in command arguments with the actual memory file path
- **AWS Integration**: Seamlessly works with AWS credentials and configuration

## Installation

### From Source

```bash
# Clone the repository
git clone https://github.com/bnusunny/serverless-llama-cpp.git
cd serverless-llama-cpp/s3mem-run

# Build the binary
cargo build --release

# The binary will be available at target/release/s3mem-run
```

### As AWS Lambda Layer

The tool is designed to be used as an AWS Lambda Layer. See the parent project for deployment instructions.

## Usage

```bash
s3mem-run [OPTIONS] <COMMAND> [ARGS]...
```

### Options

- `--bucket <BUCKET>`: S3 bucket containing the file (defaults to S3_BUCKET env var)
- `--key <KEY>`: S3 key (defaults to S3_KEY env var)
- `--memfd-placeholder <PLACEHOLDER>`: Placeholder for memfd (defaults to '{{memfd}}')
- `--log-level <LEVEL>`: Set logging level (trace, debug, info, warn, error) (defaults to 'info')

### Environment Variables

- `S3_BUCKET`: S3 bucket containing the file
- `S3_KEY`: S3 key for the file
- `MEMFD_PLACEHOLDER`: Placeholder string to be replaced with the memory file path (default: `{{memfd}}`)
- `RUST_LOG`: Control logging verbosity (e.g., `RUST_LOG=debug,s3mem_run=trace`)

### Examples

#### Basic Usage

```bash
s3mem-run --bucket my-bucket --key models/large-model.bin my-program --model {{memfd}} --other-args
```

#### Using Environment Variables

```bash
export S3_BUCKET=my-bucket
export S3_KEY=models/large-model.bin
s3mem-run my-program --model {{memfd}} --other-args
```

#### With Different Log Levels

```bash
# Default logging (info level)
s3mem-run --bucket model-bucket --key llama-7b.gguf llama-server -m {{memfd}} -c 2048

# Debug logging
s3mem-run --bucket model-bucket --key llama-7b.gguf --log-level debug llama-server -m {{memfd}} -c 2048

# Trace logging (most verbose)
s3mem-run --bucket model-bucket --key llama-7b.gguf --log-level trace llama-server -m {{memfd}} -c 2048
```

#### Using Environment Variables for Logging

```bash
RUST_LOG=s3mem_run=debug,aws_sdk_s3=info s3mem-run --bucket my-bucket --key models/large-model.bin my-program --model {{memfd}}
```

## How It Works

1. **Memory File Creation**: Creates an in-memory file using Linux's `memfd_create` system call
2. **Parallel Downloading**: Downloads the file from S3 in parallel chunks
3. **Direct Memory Writing**: Writes the downloaded chunks directly to the memory file descriptor
4. **Placeholder Replacement**: Replaces the placeholder in command arguments with the actual memory file path
5. **Program Execution**: Executes the specified program with the memory file descriptor as input

## Use Cases

- **Serverless ML Inference**: Run large language models in AWS Lambda without disk space limitations
- **Large File Processing**: Process large files in memory-constrained environments
- **Ephemeral Computing**: Work with large files in environments where disk writes are expensive or limited

## Limitations

- Requires Linux with `memfd_create` support (kernel 3.17+)
- The file must fit in available memory
- AWS credentials must be configured for S3 access

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.