FROM public.ecr.aws/docker/library/rust:slim-bookworm as builder

# Install dependencies including musl-tools
RUN apt-get update && apt-get install -y \
    musl-tools \
    build-essential \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Set up the working directory
WORKDIR /app

# Copy the source code
COPY . .

# Add musl target
RUN rustup target add x86_64-unknown-linux-musl

# Build the application using musl-gcc
RUN CC=musl-gcc \
    cargo build --release --target x86_64-unknown-linux-musl

# Strip the binary to reduce size
RUN strip /app/target/x86_64-unknown-linux-musl/release/s3mem-run

# Create a minimal runtime image
FROM scratch

# Copy the binary from the builder stage
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/s3mem-run /s3mem-run

# Set the entrypoint
ENTRYPOINT ["/s3mem-run"]