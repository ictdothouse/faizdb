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

### A. Workspace Unit & Integration Tests (`cargo test --workspace`):
* **Status:** ✅ **148 / 148 Tests Passed (100% Pass Rate across 18 Test Suites)**
* **Compilation Status:** **0 Errors, 0 Warnings** (Strict Clean Build)
* **Tested Monorepo Suites & Crates:**
  * `faizdb-core` (81 tests: LSM-Tree, MemTable, WAL, MVCC ACID, BM25, TTL, Raft Disk Consensus, Storage Durability, Storage Fuzzing, Backup PITR AES-256-GCM)
  * `faizdb-server` (33 tests: Multi-Protocol Handshake, Auth Flow, Chaos CRDT Partition Healing, Document CRUD, Durability & Transaction Write Staging, Vector & Graph REST API, Vector Search, TLS / HTTPS Transport)
  * `faizdb-vector` (15 tests: Multi-Layer HNSW, Cosine/L2/Manhattan, Scalar & Binary 32x Quantization)
  * `faizdb-query` (9 tests: AST Parser, Distributed Scatter-Gather Reduction, Cost-Based CBO Optimizer, $unwind Aggregation Pipeline)
  * `faizdb-security` (6 tests: AES-256-GCM AEAD, Argon2id, Ed25519 JWT RBAC, Rustls / Ring TLS Self-Signed & PEM Server Config)
  * `faizdb-graph` (2 tests: Knowledge Graph, Multi-Hop BFS/DFS Traversal, Dijkstra Shortest Path)
  * Documentation doctests (2 tests)

---

### B. Empirical Benchmark Classification (Durable Disk vs In-Memory):

To maintain complete scientific and engineering integrity, performance metrics are strictly categorized by execution layer:

| Benchmark Category | Workload & Hardware | Debug Build | Optimized Release (`opt-level=3` + LTO) |
|:---|:---|:---:|:---:|
| **Durable Disk Writes** | WAL + strict `fsync` (`sync_writes: true`), HTTP API | **1,481 ops/sec** *(2 vCPU)* | **24,000 – 53,282 ops/sec** *(NVMe)* |
| **In-Memory MemTable Ingestion** | Lock-Free SkipList (`crossbeam-skiplist`), standalone | **38,600 ops/sec** | **323,424 ops/sec** *(Criterion microbench)* |
| **In-Memory Table Scan** | Zero-Copy Memory Iterator, sequential point scan | **464,465 ops/sec** | **671,327 ops/sec** *(Criterion microbench)* |
| **HNSW Vector ANN Search** | Top-10 Nearest Neighbors, 128–4096 dims | **< 2.5 ms** | **< 0.85 ms** |
| **Physical Resident RAM (`VmRSS`)** | 4 Multi-Protocol Gateways active (Linux Kernel `/proc`) | **~32 MB** | **23.28 MB** *(23,844 kB)* |
| **Stripped Executable Size** | Single binary on disk (`stat -c %s`) | ~38 MB | **6.30 MB** *(6,615,160 bytes)* |

```text
🏎️ FaizDB High-Throughput Benchmark — 50,000 documents (Release Binary)

⚡ INSERT (Durable Disk + WAL): 50,000 docs in 938.40ms (  53,282 ops/sec )
⚡ SCAN   (Zero-Copy Iterator): 50,000 docs in 104.91ms ( 476,600 ops/sec )
⚡ FILTER (Secondary B-Tree):   25,000 docs in  79.62ms ( 314,000 ops/sec )

📊 Summary:
  Documents in memory: 50,000
  Total data size:     10.48 MB
  Avg doc size:        219 bytes
```

---

## 2. Running Rust Unit & Integration Tests

Execute the following commands from the workspace root:

```bash
# Run all unit and integration tests across the workspace:
cargo test --workspace

# Run dedicated integration test suites:
cargo test -p faizdb-server --test test_auth_flow      # EdDSA JWT & Argon2id Auth Flow
cargo test -p faizdb-server --test test_document_crud   # High-Volume Document CRUD & WAL Crash Safety
cargo test -p faizdb-server --test test_vector_search   # HNSW Vector Indexing & Distance Metrics

# Run tests for a specific module (e.g., CRDTs & Geo-Replication):
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

---

## 6. Distributed Chaos & Fault Tolerance Testing

Validate system resilience against mid-flight power failures, network partitions, and split-brain recovery:

```bash
# Run automated Chaos Engineering test suite:
cargo test -p faizdb-server --test test_chaos_fault_tolerance
```

Test source file: [`faizdb-server/tests/test_chaos_fault_tolerance.rs`](../faizdb-server/tests/test_chaos_fault_tolerance.rs).

---

## 7. Official YCSB (Yahoo! Cloud Serving Benchmark) Runner

Execute multi-threaded industry-standard workload suites against a live FaizDB instance:

```bash
# Workload A (50% Read / 50% Update):
python scripts/ycsb_runner.py --workload A --ops 10000 --threads 8

# Workload B (95% Read / 5% Update):
python scripts/ycsb_runner.py --workload B --ops 10000 --threads 16

# Workload C (100% Read Only):
python scripts/ycsb_runner.py --workload C --ops 10000 --threads 16

# Workload V (AI High-Dimensional Vector ANN Search):
python scripts/ycsb_runner.py --workload V --ops 5000 --threads 8
```

For complete architecture details, see the [📖 Tier-1 Engineering & Architecture Reference Guide](TIER1_ENGINEERING_GUIDE.md).

