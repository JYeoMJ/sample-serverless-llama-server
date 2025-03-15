# Serverless Llama Server

Run large language models on AWS Lambda using [LLaMA.cpp HTTP Server](https://github.com/ggml-org/llama.cpp/blob/master/examples/server/README.md) and serverless architecture.

## Overview

This project demonstrates how to deploy and run Llama.cpp-based language models on AWS Lambda, providing a cost-effective, scalable solution for AI inference without managing infrastructure. It uses AWS Serverless Application Model (SAM) to deploy a Lambda function that runs the Llama.cpp server with models loaded directly from Amazon S3.

### Key Features

- **Serverless LLM Inference**: Run large language models without managing servers
- **Memory-Optimized Loading**: Load models directly from S3 into memory using `s3mem-run`
- **OpenAI-Compatible API**: Interface with the model using a familiar API format
- **Streaming Responses**: Support for streaming text generation
- **Cost-Effective**: Pay only for the compute time you use

## Architecture

The application consists of these main components:

1. **Lambda Function**: Hosts the Llama.cpp server with 10GB memory allocation
2. **S3 Model Loading**: Custom `s3mem-run` utility loads models from S3 directly into memory
3. **[Lambda Web Adapter](https://github.com/awslabs/aws-lambda-web-adapter)**: Handles HTTP requests and responses
4. **Python Client**: Provides an easy way to interact with the deployed model

## How It Works: Solving Serverless LLM Challenges

This project addresses two major challenges in serverless LLM deployment through an innovative combination of `s3mem-run` and Lambda SnapStart:

### Challenge 1: Cold Start Performance

**Problem**: Lambda cold starts can cause significant delays when loading large models, resulting in poor user experience.

**Solution - Lambda SnapStart**:
- Pre-initializes the Lambda execution environment and caches it
- Restores the cached snapshot on invocation instead of cold starting
- Reduces cold start times from tens of seconds to milliseconds
- Provides consistent, predictable response times

### Challenge 2: SnapStart's Size Limitations

**Problem**: Lambda SnapStart has strict limitations that would normally prevent its use with LLMs:
- Only supports ZIP packaging (256MB max)
- Limited to 512MB `/tmp` storage
- Doesn't support 10GB `/tmp` or EFS attachments

**Solution - s3mem-run**:
- Downloads model files from S3 directly into memory during function initialization
- Uses Linux's `memfd_create` to create memory-based file descriptors
- Bypasses all filesystem limitations completely
- The entire memory state, including the loaded model, is captured in the SnapStart snapshot
- When the function is invoked, the snapshot is restored with the model already in memory
- No disk I/O or model loading happens during invocation

### The Combined Approach

The key innovation is how these technologies work together:

1. **During Deployment**:
   - The model is loaded from S3 directly into memory (not disk)
   - SnapStart captures this memory state in a snapshot

2. **During Invocation**:
   - The snapshot with the pre-loaded model is quickly restored
   - The function starts with the model already in memory
   - No model loading or disk I/O is needed

3. **Benefits**:
   - **Ultra-fast Cold Starts**: Functions start in milliseconds
   - **Large Model Support**: Run models far larger than the 256MB package or 512MB `/tmp` limits
   - **Cost Efficiency**: Reduced execution time means lower costs
   - **Better User Experience**: Consistent, fast response times

This approach makes serverless LLMs practical by solving both the cold start problem and the model size limitations simultaneously.

## Recommended Models

This project works well with a variety of GGUF models. Here are some recommended options:

### DeepSeek-R1-Distill-Qwen-1.5B

A highly efficient 1.5B parameter model that offers excellent performance in a serverless environment:
- Great balance of quality and size
- Optimized for instruction following
- Works well with the 10GB Lambda configuration
- [Model on Hugging Face](https://huggingface.co/unsloth/DeepSeek-R1-Distill-Qwen-1.5B-GGUF)

### Other Good Options

- **Qwen2.5-1.5B**: Another excellent small model with good performance
- **Phi-3-mini-4k-instruct**: Microsoft's 3.8B parameter model with strong reasoning
- **Mistral-7B-Instruct**: Larger model requiring more memory but offering higher quality

For best results with the default 10GB Lambda configuration, we recommend using models in the 1.5B-7B parameter range with Q4_K_M or Q8_0 quantization.

## Prerequisites

- [AWS SAM CLI](https://docs.aws.amazon.com/serverless-application-model/latest/developerguide/serverless-sam-cli-install.html)
- [AWS CLI](https://aws.amazon.com/cli/) configured with appropriate permissions
- [Docker](https://www.docker.com/products/docker-desktop) for local testing and building
- Python 3.8+ for the client application

## Deployment

### 1. Build the Application

```bash
sam build
```

### 2. Deploy to AWS

```bash
sam deploy --guided
```

During the guided deployment, you'll be prompted for:
- Stack name
- AWS Region
- Confirmation of IAM role creation
- S3 bucket name for model storage
- S3 key for the model file

### 3. Download and Upload Your Model to S3

#### Download the DeepSeek-R1-Distill-Qwen-1.5B Model

```bash
# Create a models directory if it doesn't exist
mkdir -p models

# Download the DeepSeek-R1-Distill-Qwen-1.5B-Q4_K_M.gguf model
wget -O models/DeepSeek-R1-Distill-Qwen-1.5B-Q4_K_M.gguf https://huggingface.co/unsloth/DeepSeek-R1-Distill-Qwen-1.5B-GGUF/resolve/main/DeepSeek-R1-Distill-Qwen-1.5B-Q4_K_M.gguf
```

#### Upload the Model to S3

```bash
aws s3 cp models/DeepSeek-R1-Distill-Qwen-1.5B-Q4_K_M.gguf s3://your-bucket-name/DeepSeek-R1-Distill-Qwen-1.5B-Q4_K_M.gguf
```

You can also use any other GGUF model of your choice:

```bash
aws s3 cp your-model.gguf s3://your-bucket-name/your-model.gguf
```

### 4. Update Environment Variables

If needed, update the Lambda function's environment variables to point to your model:

```bash
aws lambda update-function-configuration \
  --function-name your-function-name \
  --environment "Variables={S3_BUCKET=your-bucket-name,S3_KEY=your-model.gguf}"
```

## Using the Client

This project includes an interactive Python client in the `client` directory for communicating with your deployed LLM:

### Key Features

- Interactive chat interface with command history
- Multi-line input support for code snippets and longer text
- Streaming responses with interruption capability
- AWS SigV4 authentication for Lambda function URLs

### Quick Start

```bash
# Navigate to the client directory
cd client

# Install dependencies (preferably in a virtual environment)
pip install -r requirements.txt

# Run the client with your Lambda function URL
python client.py --api-base https://your-lambda-function-url
```

For detailed instructions, examples, and advanced usage, see the [client README](client/README.md).

## Customization

### Using Different Models

Update the `S3_BUCKET` and `S3_KEY` environment variables in the Lambda function to point to your model file.

### Adjusting Model Parameters

Modify the `run.sh` script to pass different parameters to the Llama.cpp server:

```bash
#!/bin/bash
exec bin/s3modelfd bin/llama-server -m {{memfd}} -c 2048 -t 8 -fa
```

For detailed documentation on all available llama-server parameters and configuration options, refer to the [official llama-server documentation](https://github.com/ggml-org/llama.cpp/blob/master/examples/server/README.md).

## Troubleshooting

### View Lambda Logs

```bash
sam logs -n LlamaServer --stack-name your-stack-name --tail
```

### Test Locally

```bash
sam local invoke LlamaServer
```

## Cleanup

To delete all resources created by this project:

```bash
sam delete --stack-name your-stack-name
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Acknowledgments

- [Llama.cpp](https://github.com/ggerganov/llama.cpp) for the efficient C++ implementation of LLM inference
- AWS SAM for the serverless application framework