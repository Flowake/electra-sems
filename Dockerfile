# Multi-stage build for minimal production image
FROM rust:1.90 as builder

# Set working directory
WORKDIR /app

# Copy manifest files
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/

# Build the application in release mode
RUN cargo build --release

# Runtime stage with minimal base image
FROM debian:bookworm-slim

# Install required runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user for security
RUN useradd -r -s /bin/false electra

# Set working directory
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/sems_api /usr/local/bin/sems_api

# Copy example configuration
COPY examples/ ./examples/

# Change ownership to non-root user
RUN chown -R electra:electra /app

# Switch to non-root user
USER electra

# Expose the default port
EXPOSE 3000

# Set default command
CMD ["sems_api", "--config", "examples/station_config.json", "--port", "3000"]
