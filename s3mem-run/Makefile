# AWS Lambda Layer build target using Docker with musl-gcc
build-S3MemRunLayer:
	@echo "Building s3mem-run for AWS Lambda Layer using Docker..."
	docker build -t s3mem-run-builder .
	@echo "Extracting binary from Docker image..."
	mkdir -p $(ARTIFACTS_DIR)/bin
	docker create --name temp-container s3mem-run-builder
	docker cp temp-container:/s3mem-run $(ARTIFACTS_DIR)/bin/
	docker rm temp-container
	@echo "Binary available at $(ARTIFACTS_DIR)/bin/s3mem-run"
	chmod +x $(ARTIFACTS_DIR)/bin/s3mem-run
