# 🧪 FaizDB: Panduan Ujian & Penanda Aras (Testing & Benchmarks Guide)

Dokumen ini memperincikan cara-cara menjalankan ujian unit, ujian integrasi multi-protokol, ujian penanda aras (*benchmarks*), serta hasil ujian sebenar yang telah direkodkan.

---

## 📑 Isi Kandungan

1. [Hasil Ujian Sebenar (Actual Test Run Results)](#1-hasil-ujian-sebenar)
2. [Cara Menjalankan Ujian Unit Rust (Unit Tests)](#2-cara-menjalankan-ujian-unit-rust)
3. [Cara Menjalankan Penanda Aras Kelajuan (Live Benchmarks)](#3-cara-menjalankan-penanda-aras-kelajuan)
4. [Cara Menjalankan Ujian Integrasi Multi-Protokol (E2E Tests)](#4-cara-menjalankan-ujian-integrasi-multi-protokol)
5. [Ujian Mikro-Latensi Nanosaat (Criterion Framework)](#5-ujian-mikro-latensi-nanosaat)

---

## 1. Hasil Ujian Sebenar (Actual Test Run Results)

Berikut adalah ringkasan hasil ujian sebenar yang telah disahkan pada enjin FaizDB:

### A. Ujian Unit Rust (`cargo test --workspace`):
* **Status:** ✅ **84 / 84 Ujian Lulus (100% Passed)**
* **Kompilasi:** **0 Errors, 0 Warnings**
* **Crate yang Diuji:**
  * `faizdb-core` (LSM-Tree, MemTable, WAL, MVCC ACID, BM25, TTL, Raft, CRDTs)
  * `faizdb-vector` (HNSW Multi-Layer Index, Cosine/L2/Dot distance)
  * `faizdb-graph` (Knowledge Graph, Multi-Hop BFS/DFS Traversal)
  * `faizdb-query` (AST Parser, Cost-Based EXPLAIN Optimizer, Aggregations)
  * `faizdb-security` (AES-256-GCM AEAD, Argon2id, JWT RBAC)
  * `faizdb-server` (MongoDB Wire, PostgreSQL Wire, gRPC Protobuf, REST/WS)

---

### B. Hasil Penanda Aras 50,000 Dokumen (`faizdb benchmark`):

Dijalankan secara langsung di atas binari Release (`opt-level=3` + Fat LTO):

```text
🏎️ FaizDB High-Throughput Benchmark — 50,000 documents

⚡ INSERT :    50000 docs in 938.40ms (  53,282 ops/sec )
⚡ SCAN   :    50000 docs in 104.91ms ( 476,600 ops/sec )
⚡ FILTER :    25000 docs in  79.62ms

📊 Summary:
  Documents in memory: 50,000
  Total data size:     10.48 MB
  Avg doc size:        219 bytes
```

---

## 2. Cara Menjalankan Ujian Unit Rust

Jalankan arahan berikut dari direktori utama projek:

```bash
# Menjalankan kesemua 84 ujian unit merentasi semua crates
cargo test --workspace

# Menjalankan ujian untuk crate tertentu sahaja (contoh: CRDTs & Geo-Replication)
cargo test -p faizdb-core -- cluster::crdt
```

---

## 3. Cara Menjalankan Penanda Aras Kelajuan

### Kaedah A: Melalui CLI Terbina
```bash
# Menjalankan ujian suntikan 50,000 dokumen:
cargo run --release --bin faizdb -- benchmark --count 50000

# Atau menggunakan binari release secara langsung:
./target/release/faizdb benchmark --count 100000
```

### Kaedah B: Melalui Skrip Python Otomatik
```bash
# 1. Mulakan pelayan FaizDB di terminal pertama:
./target/release/faizdb serve

# 2. Jalankan skrip benchmark di terminal kedua:
python scripts/benchmark.py
```

---

## 4. Cara Menjalankan Ujian Integrasi Multi-Protokol

FaizDB dilengkapi suite ujian integrasi Python untuk setiap pintu masuk:

```bash
# 1. Ujian PostgreSQL Wire Protocol (Port 5432)
python tests/integration/test_postgres_wire.py

# 2. Ujian gRPC & Protocol Buffers (Port 50051)
python tests/integration/test_grpc.py

# 3. Ujian MongoDB Wire Protocol (Port 27017)
python tests/integration/test_mongo_wire.py

# 4. Ujian Replikasi Multi-Region & CRDTs
python tests/test_geo_replication.py

# 5. Ujian Carian Teks Penuh Okapi BM25
python tests/test_fulltext_search.py

# 6. Ujian Pipeline Agregasi & Analitis
python tests/test_aggregation_pipeline.py
```

---

## 5. Ujian Mikro-Latensi Nanosaat (Criterion Framework)

Untuk mengukur kitaran CPU dan latensi peruntukan memori nanosaat:

```bash
cargo bench -p faizdb-core
```
Fail ujian terletak di: [`faizdb-core/benches/storage_bench.rs`](../faizdb-core/benches/storage_bench.rs).
