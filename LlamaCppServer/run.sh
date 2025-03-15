#!/bin/bash
exec /opt/bin/s3mem-run --bucket $S3_BUCKET --key $S3_KEY bin/llama-server -m {{memfd}} -fa -c 8192