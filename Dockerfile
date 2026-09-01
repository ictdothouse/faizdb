# Multi-stage Dockerfile for FaizDB — The AI-Native NoSQL Database

# Stage 1: Build
FROM rust:1.85-slim-bookworm as builder

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

# Expose MongoDB Wire Protocol (27017) and HTTP/REST API (27018)
EXPOSE 27017 27018

# Data directory volume
VOLUME ["/data"]

# Default entrypoint starts the FaizDB dual-protocol server
ENTRYPOINT ["faizdb"]
CMD ["serve", "--wire-port", "27017", "--http-port", "27018", "--host", "0.0.0.0"]
