# Serverless Llama.cpp

Run large language models on AWS Lambda using Llama.cpp and serverless architecture.

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
3. **Lambda Web Adapter**: Handles HTTP requests and responses
4. **Python Client**: Provides an easy way to interact with the deployed model

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

### 3. Upload Your Model to S3

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

The included Python client (`client.py`) provides an interactive chat interface:

```bash
# Set up a Python virtual environment
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt

# Run the client (set your Lambda function URL)
export CHAT_API_BASE=https://your-lambda-function-url
python client.py
```

You can also specify the API base URL directly:

```bash
python client.py --api-base https://your-lambda-function-url
```

### Client Commands

- `/quit` - Exit the chat
- `/new` - Start a new conversation
- Use ↑/↓ keys to navigate through history

## How It Works

### s3mem-run

The `s3mem-run` utility is a key component that:
1. Downloads model files from S3 directly into memory using Linux's `memfd_create`
2. Passes the memory file descriptor to Llama.cpp server
3. Optimizes download performance through parallel chunked downloads

This approach avoids Lambda's `/tmp` directory size limitations and improves cold start times.

### Lambda Configuration

The Lambda function is configured with:
- 10GB memory allocation
- 15-minute timeout
- SnapStart for improved cold start performance
- Lambda Web Adapter for HTTP request handling

## Customization

### Using Different Models

Update the `S3_BUCKET` and `S3_KEY` environment variables in the Lambda function to point to your model file.

### Adjusting Model Parameters

Modify the `run.sh` script to pass different parameters to the Llama.cpp server:

```bash
#!/bin/bash
exec bin/s3modelfd bin/llama-server -m {{memfd}} -c 2048 -t 8 -fa
```

## Troubleshooting

### View Lambda Logs

```bash
sam logs -n LlamaCppServer --stack-name your-stack-name --tail
```

### Test Locally

```bash
sam local invoke LlamaCppServer
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