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
* **Status:** ✅ **200+ Tests Passed (100% Pass Rate across all Workspace Crates)**
* **Compilation Status:** **0 Errors, 0 Warnings** (Strict Clean Build under `cargo clippy -- -D warnings`)
* **Tested Monorepo Suites & Crates:**
  * `faizdb-core` (86 tests: Dynamic LSM-Tree Anti-Stall Compaction, MemTable, WAL with Torn-Write Recovery, MVCC ACID, BM25, TTL, Raft Disk Consensus, Storage Durability, Storage Fuzzing, Backup PITR AES-256-GCM)
  * `faizdb-server` (66+ tests: Jepsen Distributed Chaos Suite, Multi-Protocol Handshake, Auth Flow, Chaos CRDT Partition Healing, Document CRUD, Durability & Transaction Write Staging, Vector & Graph REST API, Vector Search, Wire Protocol Security & Performance Benchmarks, PostgreSQL Virtual Catalog & Extended Query Protocol, MongoDB Stateful Cursors & O(1) Lookup, Audit Remediation Suite, Production Hardening & Operational Standards, TLS / HTTPS Transport)
  * `faizdb-vector` (16 tests: Multi-Layer HNSW, Cosine/L2/Manhattan, Scalar & Binary 32x Quantization, GDPR Tombstone Deletion, In-Place Mutation)
  * `faizdb-query` (35 tests: openCypher Parser, Hybrid Cypher-GraphRAG + Vector Executor, AST Parser, SQL & Mongo UPDATE, Multi-Table Hash INNER/LEFT JOIN, ORDER BY ASC/DESC, Distributed Scatter-Gather Reduction, Cost-Based CBO Optimizer, $unwind Aggregation Pipeline)
  * `faizdb-security` (14 tests: AES-256-GCM AEAD, Argon2id, Ed25519 JWT RBAC, Rustls / Ring TLS Self-Signed & PEM Server Config, Central UserStore)
  * `faizdb-graph` (7 tests: Knowledge Graph, Multi-Hop BFS/DFS Traversal, Dijkstra Shortest Path, Incident Edge Pruning & Deduplication, Deterministic `extract_rag_context` Markdown Extraction, In-Memory `SemanticCache` Cosine Similarity & TTL Expiry)
  * Documentation doctests (2 tests)
* **Latest Verification Reference:** See [`LATEST_SYSTEM_VERIFICATION_AND_BENCHMARKS.md`](LATEST_SYSTEM_VERIFICATION_AND_BENCHMARKS.md) and [`PRODUCTION_STANDARDS_AND_OPERATIONAL_HARDENING.md`](PRODUCTION_STANDARDS_AND_OPERATIONAL_HARDENING.md) for full protocol throughput and enterprise architecture breakdown.

---

### B. Empirical Benchmark Classification (Durable Disk vs In-Memory):

To maintain complete scientific and engineering integrity, performance metrics are strictly categorized by execution layer:

| Benchmark Category | Workload & Hardware | Debug Build | Optimized Release (`opt-level=3` + LTO) |
|:---|:---|:---:|:---:|
| **Durable Disk Writes** | WAL + strict `fsync`, persistent disk append | **1,481 ops/sec** *(2 vCPU)* | **32,305 ops/sec** *(Verified)* |
| **In-Memory MemTable Ingestion** | Lock-Free SkipList (`crossbeam-skiplist`), standalone | **38,600 ops/sec** | **61,432 ops/sec** *(50k docs in 813ms)* |
| **In-Memory Table Scan** | Zero-Copy Memory Iterator, sequential scan | **464,465 ops/sec** | **860,001 ops/sec** *(20k docs in 23.26ms)* |
| **HNSW Vector ANN Search** | Top-5 Nearest Neighbors, 64–4096 dims, HTTP Gateway | **< 2.5 ms** | **p50 = 880 µs (0.88 ms), 1,414 QPS** |
| **Knowledge Graph Traversal** | 3-Hop Multi-Edge BFS/DFS Traversal | **< 4.0 ms** | **p50 = 916 µs (0.91 ms)** |
| **Physical Resident RAM (`VmRSS`)** | 4 Multi-Protocol Gateways active (Linux Kernel `/proc`) | **~32 MB** | **23.05 MB** *(23,608 kB idle, 69.9 MB peak)* |
| **Stripped Executable Size** | Single binary on disk (`stat -c %s`) | ~38 MB | **7.70 MB** *(8,080,104 bytes, ultra-dense)* |

```text
🏎️ FaizDB High-Throughput Benchmark (Release Binary Verified)

⚡ INSERT (In-Memory MemTable):  50,000 docs in 813.91ms ( 61,432 ops/sec )
⚡ INSERT (Durable Disk + WAL):  20,000 docs in 619.10ms ( 32,305 ops/sec )
⚡ SCAN   (Zero-Copy Iterator):  20,000 docs in  23.26ms ( 860,001 ops/sec )
⚡ FILTER (Secondary B-Tree):    25,000 docs in 111.74ms ( 223,733 ops/sec )
⚡ VECTOR (HNSW 64-dim ANN):     Top-5 nearest neighbors in 880 µs ( 1,414 QPS )
⚡ GRAPH  (GraphRAG 3-Hop):      Multi-hop traversal in 916 µs

📊 Physical Footprint Summary:
  Standalone Executable Size : 7.70 MB (8,080,104 bytes)
  Machine Code (.text segment): 7,886,000 bytes (97.6%)
  Baseline Idle Kernel RAM   : 23.05 MB VmRSS (23,608 kB)
  Peak Memory Under Load     : 69.91 MB VmRSS (71,588 kB)
```

---

## 2. Running Rust Unit & Integration Tests

Execute the following commands from the workspace root:

```bash
# Run all unit and integration tests across the workspace:
cargo test --workspace

# Run dedicated integration test suites:
cargo test -p faizdb-server --test test_auth_flow             # EdDSA JWT & Argon2id Auth Flow
cargo test -p faizdb-server --test test_document_crud          # High-Volume Document CRUD & WAL Crash Safety
cargo test -p faizdb-server --test test_vector_search          # HNSW Vector Indexing & Distance Metrics
cargo test -p faizdb-server --test test_jepsen_distributed_chaos # Jepsen Distributed Chaos, Split-Brain & Torn-Write Recovery

# Run tests for a specific module (e.g., CRDTs & Geo-Replication):
cargo test -p faizdb-core -- cluster::crdt
cargo test -p faizdb-query -- parser::tests::test_parse_cypher # openCypher Graph Syntax Tests
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

### Option C: Via Official Scientific Systems Audit Suite
```bash
# Runs full empirical verification: ELF byte analysis, live Linux kernel memory (VmRSS),
# 5,000 document ingestion, HNSW vector ANN latency, and GraphRAG traversal:
bash scripts/run_scientific_audit.sh
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

