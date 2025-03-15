# Llama Server for AWS Lambda

This directory contains the core components for running the Llama.cpp server on AWS Lambda:

## Components

- **Dockerfile**: Defines the container image used to build the Llama.cpp server binary
- **run.sh**: The Lambda handler script that starts the Llama.cpp server using s3mem-run
- **Makefile**: Build instructions for AWS SAM

## How It Works

When deployed, the Lambda function:

1. Uses [s3mem-run](../s3mem-run/) to load the model file directly from S3 into memory
2. Starts the Llama.cpp server with the loaded model
3. Handles HTTP requests via [Lambda Web Adapter](https://github.com/awslabs/aws-lambda-web-adapter)
4. Provides an API compatible with standard LLM interfaces

## Configuration

The server behavior can be customized by modifying the `run.sh` script. Key parameters include:

- `-m {{memfd}}`: Uses the memory file descriptor created by s3mem-run
- `-c 2048`: Sets the context window size
- `-t 6`: Number of threads to use
- `-fa`: Enable Flash Attention

For more configuration options, refer to the [Llama.cpp server documentation](https://github.com/ggml-org/llama.cpp/blob/master/examples/server/README.md).