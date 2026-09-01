# 🧪 FaizDB: Testing & Benchmarking Guide

This guide details how to run unit tests, multi-protocol integration tests, high-throughput benchmarks, and inspect real-world verification results across the FaizDB workspace.

---

## 📑 Table of Contents

1. [Actual Test Run Results](#1-actual-test-run-results)
2. [Running Rust Unit Tests (`cargo test`)](#2-running-rust-unit-tests)
3. [Running In-Memory CLI Benchmarks](#3-running-in-memory-cli-benchmarks)
4. [Running Multi-Protocol E2E Integration Tests](#4-running-multi-protocol-e2e-integration-tests)
5. [Nanosecond Micro-Benchmarking (Criterion Framework)](#5-nanosecond-micro-benchmarking)

---

## 1. Actual Test Run Results

The following metrics represent verified test runs conducted against the optimized release build of FaizDB:

### A. Workspace Unit Tests (`cargo test --workspace`):
* **Status:** ✅ **84 / 84 Tests Passed (100%)**
* **Compilation Status:** **0 Errors, 0 Warnings**
* **Tested Monorepo Crates:**
  * `faizdb-core` (LSM-Tree, MemTable, WAL, MVCC ACID, BM25, TTL, Raft, CRDTs)
  * `faizdb-vector` (HNSW Multi-Layer Index, Cosine/L2/Dot distance metrics)
  * `faizdb-graph` (Knowledge Graph, Multi-Hop BFS/DFS Traversal)
  * `faizdb-query` (AST Parser, Cost-Based EXPLAIN Optimizer, Aggregations)
  * `faizdb-security` (AES-256-GCM AEAD, Argon2id, JWT RBAC)
  * `faizdb-server` (MongoDB Wire, PostgreSQL Wire, gRPC Protobuf, REST/WebSockets)

---

### B. 50,000 Document Ingestion Benchmark (`faizdb benchmark`):

Executed directly on the Release Binary (`opt-level=3` + Fat LTO):

```text
🏎️ FaizDB High-Throughput Benchmark — 50,000 documents

⚡ INSERT :    50,000 docs in 938.40ms (  53,282 ops/sec )
⚡ SCAN   :    50,000 docs in 104.91ms ( 476,600 ops/sec )
⚡ FILTER :    25,000 docs in  79.62ms

📊 Summary:
  Documents in memory: 50,000
  Total data size:     10.48 MB
  Avg doc size:        219 bytes
```

---

## 2. Running Rust Unit Tests

Execute the following command from the workspace root:

```bash
# Run all 84 unit tests across all workspace crates:
cargo test --workspace

# Run tests for a specific crate (e.g., CRDTs & Geo-Replication):
cargo test -p faizdb-core -- cluster::crdt
```

---

## 3. Running In-Memory CLI Benchmarks

### Option A: Via Built-in CLI Subcommand
```bash
# Run a 50,000-document benchmark in release mode:
cargo run --release --bin faizdb -- benchmark --count 50000

# Or execute the compiled release binary directly:
./target/release/faizdb benchmark --count 100000
```

### Option B: Via Automated Python Benchmark Suite
```bash
# 1. Start the FaizDB Multi-Protocol Gateway daemon:
./target/release/faizdb serve

# 2. In a separate terminal, launch the benchmark runner:
python scripts/benchmark.py
```

---

## 4. Running Multi-Protocol E2E Integration Tests

FaizDB provides automated integration test suites located in `tests/`:

```bash
# 1. PostgreSQL Wire Protocol Test (Port 5432)
python tests/integration/test_postgres_wire.py

# 2. gRPC & Protocol Buffers Test (Port 50051)
python tests/integration/test_grpc.py

# 3. MongoDB Wire Protocol Test (Port 27017)
python tests/integration/test_mongo_wire.py

# 4. Multi-Region Active-Active CRDTs Test
python tests/test_geo_replication.py

# 5. Okapi BM25 Full-Text Search Test
python tests/test_fulltext_search.py

# 6. Aggregation & Analytics Pipeline Test
python tests/test_aggregation_pipeline.py
```

---

## 5. Nanosecond Micro-Benchmarking (Criterion Framework)

To measure CPU cycles, memory allocations, and nanosecond latency distributions:

```bash
cargo bench -p faizdb-core
```
Benchmark source file: [`faizdb-core/benches/storage_bench.rs`](../faizdb-core/benches/storage_bench.rs).
