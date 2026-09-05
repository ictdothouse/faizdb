# Multi-stage Dockerfile for FaizDB — The AI-Native NoSQL Database

# Stage 1: Build
FROM rust:1.88-slim-bookworm AS builder

WORKDIR /usr/src/faizdb

# Install build dependencies
RUN apt-get update && apt-get install -y build-essential pkg-config libssl-dev clang && rm -rf /var/lib/apt/lists/*

# Copy sources
COPY . .

# Build release binary
RUN cargo build --release --bin faizdb

# Stage 2: Runtime image (Ultra lightweight, < 50MB)
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy compiled binary from builder
COPY --from=builder /usr/src/faizdb/target/release/faizdb /usr/local/bin/faizdb

# Expose MySQL (3306), MongoDB (27017), PostgreSQL (5432), gRPC (50051), and HTTP/REST (27018)
EXPOSE 3306 27017 5432 50051 27018

# Data directory volume
VOLUME ["/data"]

# Default entrypoint starts the FaizDB 5-way multi-protocol server
ENTRYPOINT ["faizdb"]
CMD ["serve", "--mysql-port", "3306", "--wire-port", "27017", "--pg-port", "5432", "--grpc-port", "50051", "--http-port", "27018", "--host", "0.0.0.0"]
