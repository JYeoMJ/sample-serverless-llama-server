#!/bin/bash
# This script is the entry point for the AWS Lambda function
# It uses s3mem-run to download the LLM model from S3 directly into memory
# and then launches the llama-server with the model loaded from memory

# Execute s3mem-run with the following parameters:
# --bucket $S3_BUCKET: The S3 bucket containing the model file (from environment variable)
# --key $S3_KEY: The S3 key/path to the model file (from environment variable)
# bin/llama-server: The path to the llama.cpp server binary
# -m {{memfd}}: Use the memory file descriptor for the model (placeholder will be replaced by s3mem-run)
# -fa: Enable Flash Attention for faster processing and better memory efficiency
# -c 32768: Set the context window size to 32768 tokens (supports very long conversations)
# -t 6: Use 6 threads to fully utilize all available vCPUs in a 10GB Lambda
# -b 2048: Set logical batch size to 2048 tokens (maximum tokens processed in a single forward pass)
# -ub 512: Set physical batch size to 512 tokens (actual tokens processed in parallel by hardware)
# --cache-type-k q8_0: Use 8-bit quantization for key cache to reduce memory usage
# --cache-type-v f16: Keep value cache in full precision for better quality output
# --top-k 40 --top-p 0.9 --temp 0.7: Default sampling parameters for text generation
# --repeat-penalty 1.1: Applies a mild penalty to repeated tokens to reduce repetition
#   - These parameters provide sensible defaults for text generation
#   - Client requests can override these values on a per-request basis
#   - Parameters not specified in client requests will use these server defaults

exec /opt/bin/s3mem-run --bucket $S3_BUCKET --key $S3_KEY bin/llama-server -m {{memfd}} -fa -c 32768 -t 6 -b 2048 -ub 512 --cache-type-k q8_0 --cache-type-v f16 --top-k 40 --top-p 0.9 --temp 0.7 --repeat-penalty 1.1