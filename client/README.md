# Serverless LLM Client

A Python client for interacting with the serverless Llama.cpp API. This client provides a command-line interface for chatting with large language models deployed on AWS Lambda.

## Features

- Interactive chat interface with command history
- Support for multi-line input with delimiters
- Streaming responses with thinking animation
- AWS SigV4 authentication for Lambda function URLs
- Environment variable configuration via .env files
- Response interruption with Ctrl+C

## Installation

1. **Set up a Python virtual environment**:

```bash
# Create and activate a virtual environment
python -m venv .venv
source .venv/bin/activate  # On Windows: .venv\Scripts\activate
```

2. **Install dependencies**:

```bash
pip install -r requirements.txt
```

3. **Configure AWS credentials**:

Ensure you have AWS credentials configured either through:
- Environment variables (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`)
- AWS credentials file (`~/.aws/credentials`)
- IAM roles (if running on AWS services)

## Usage

### Basic Usage

```bash
# Set the API base URL as an environment variable
export CHAT_API_BASE=https://your-lambda-function-url

# Run the client
python client.py
```

### Command Line Arguments

```bash
# Specify API base URL directly
python client.py --api-base https://your-lambda-function-url

# Adjust temperature (0.0-1.0)
python client.py --temperature 0.7

# Set maximum tokens for responses
python client.py --max-tokens 2048
```

### Environment Variables

You can create a `.env` file with the following variables:

```
CHAT_API_BASE=https://your-lambda-function-url
```

## Chat Commands

- `/quit` - Exit the chat
- `/new` - Start a new conversation
- Use ↑/↓ keys to navigate through history

## Multi-line Input

The client supports multi-line input with delimiters:

1. **Using EOF delimiter**:
```
➤ EOF
(Enter your multi-line text. Type 'EOF' on a new line when finished)
function calculateSum(a, b) {
  return a + b;
}

console.log(calculateSum(5, 10));
EOF
```

2. **Using triple backticks** (like in Markdown):
```
➤ ```
(Enter your multi-line text. Type '```' on a new line when finished)
def hello():
    print("Hello, world!")
    
hello()
```
```

## Response Interruption

- Press `Ctrl+C` once to interrupt the current response
- Press `Ctrl+C` twice in quick succession to exit the chat

## Windows Users

If you're using Windows, you may need to install the `pyreadline` package:

```bash
pip install pyreadline
```

## Troubleshooting

### Authentication Issues

If you encounter authentication errors, check that:
1. Your AWS credentials are correctly configured
2. The Lambda function allows access from your IAM principal
3. The region in the Lambda function URL matches your credentials

### Connection Issues

If you can't connect to the API:
1. Verify the Lambda function URL is correct
2. Check that the Lambda function is deployed and running
3. Ensure your network allows outbound HTTPS connections

## License

This project is licensed under the MIT License - see the LICENSE file for details.