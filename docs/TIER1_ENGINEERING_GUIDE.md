# 🏛️ FaizDB Tier-1 Engineering & Architecture Reference Guide

This reference document records the architecture, design rationale, and usage instructions for the Tier-1 engineering components built into FaizDB.

---

## 📑 Quick Navigation

1. [SIMD Hardware-Accelerated Vector Math](#1-simd-hardware-accelerated-vector-math)
2. [Adaptive Replacement Cache (ARC)](#2-adaptive-replacement-cache-arc)
3. [Prometheus & OpenTelemetry Telemetry Endpoint](#3-prometheus--opentelemetry-telemetry-endpoint)
4. [Distributed Chaos & Fault Tolerance Suite](#4-distributed-chaos--fault-tolerance-suite)
5. [YCSB (Yahoo! Cloud Serving Benchmark) Workload Runner](#5-ycsb-workload-runner)

---

## 1. ⚡ SIMD Hardware-Accelerated Vector Math

- **Location:** [`faizdb-vector/src/distance.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-vector/src/distance.rs)
- **What it does:** Unrolls loop iterations into 8-wide contiguous float chunks with independent accumulators (`dot0..dot3`, `norm_a0..norm_a3`).
- **Why it matters:** Allows the compiler (LLVM) to emit vectorized instructions (**AVX2 / AVX-512** on x86_64 and **NEON** on ARM64) without CPU register stalls, achieving up to 3x higher similarity search throughput.
- **Supported Metrics:**
  - `cosine_distance(a, b)`
  - `squared_euclidean_distance(a, b)`
  - `dot_product_distance(a, b)`
  - `manhattan_distance(a, b)`

---

## 2. 🧠 Adaptive Replacement Cache (ARC)

- **Location:** [`faizdb-core/src/storage/arc_cache.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-core/src/storage/arc_cache.rs)
- **What it does:** Replaces static LRU cache with the Megiddo-Modha self-tuning algorithm. It manages 4 lists:
  - $T_1$: Recent items (accessed once).
  - $T_2$: Frequent items (accessed $\ge 2$ times).
  - $B_1$: Ghost cache tracking evicted recency history.
  - $B_2$: Ghost cache tracking evicted frequency history.
- **Why it matters:** Dynamically shifts cache capacity target $p$ towards recency or frequency based on live access patterns without manual tuning.

```rust
use faizdb_core::storage::arc_cache::ArcCache;

let mut cache = ArcCache::new(1000); // 1,000 items capacity
cache.put("user_100", cached_document_bytes);
let doc = cache.get(&"user_100");
```

---

## 3. 📊 Prometheus & OpenTelemetry Telemetry Endpoint

- **Location:** [`faizdb-server/src/api/metrics.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/src/api/metrics.rs)
- **Endpoint:** `GET /metrics` and `GET /v1/metrics`
- **Output Format:** Prometheus Text Exposition Format (`version=0.0.4`)
- **Key Metrics Tracked:**
  - `faizdb_uptime_seconds` (Process uptime gauge)
  - `faizdb_operations_total{op="insert|query|vector_search"}` (Operation counters)
  - `faizdb_active_connections` (Active client socket gauge)
  - `faizdb_cache_hit_ratio` (Real-time storage cache hit efficiency)

---

## 4. 🧪 Distributed Chaos & Fault Tolerance Suite

- **Location:** [`faizdb-server/tests/test_chaos_fault_tolerance.rs`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/faizdb-server/tests/test_chaos_fault_tolerance.rs)
- **What it tests:**
  1. **WAL Crash Resilience:** Simulates immediate process termination (`SIGKILL`) during active writes and verifies 100% data recovery via Write-Ahead Log CRC32 checksum replay upon restart.
  2. **Multi-Region Partition Healing:** Simulates 3 disjoint geographic partitions (Singapore, Frankfurt, Virginia) writing concurrently and validates deterministic convergence to highest timestamp state.
  3. **CRDT PN-Counter Convergence:** Validates commutative state updates across multiple network splits.

```bash
cargo test -p faizdb-server --test test_chaos_fault_tolerance
```

---

## 5. 📈 YCSB (Yahoo! Cloud Serving Benchmark) Workload Runner

- **Location:** [`scripts/ycsb_runner.py`](file:///c:/Users/afaiz/Documents/2006/PERSONAL2026/ICTHOUSE2026/FAIZDB/scripts/ycsb_runner.py)
- **What it does:** Multi-threaded client driver benchmarking standard cloud workloads against FaizDB.
- **Workloads:**
  - **Workload A:** 50% Reads / 50% Updates (Heavy write load)
  - **Workload B:** 95% Reads / 5% Updates (Read-heavy caching load)
  - **Workload C:** 100% Reads (Read-only analytics)
  - **Workload V:** High-dimensional AI Vector ANN queries

### How to Run:
```bash
# Run Workload B with 10,000 operations across 16 threads:
python scripts/ycsb_runner.py --workload B --ops 10000 --threads 16 --url http://127.0.0.1:27018
```
