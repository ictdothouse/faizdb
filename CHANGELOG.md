# Changelog

All notable changes to **FaizDB** are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- **5-Way Universal Protocol Gateway & Native MySQL / MariaDB Wire Ingress (Port 3306)**: Complete async MySQL HandshakeV10 protocol engine (`faizdb-server/src/wire/mysql/`), supporting MySQL CLI, Laravel Eloquent (`DB_CONNECTION=mysql`), WordPress (`wp-config.php`), PHP PDO/mysqli, ColumnDef41 packet encoding, EOF/OK packets, and automated test suite (`tests/test_mysql_wire_protocol.rs`).
- `SECURITY.md` — responsible disclosure policy
- `CONTRIBUTING.md` — full contributor guide
- `.github/workflows/ci.yml` — GitHub Actions CI (fmt, clippy -D warnings, tests, cargo-audit, MSRV)
- `faizdb-server/tests/` — comprehensive integration test suite (auth flow, document CRUD, vector search, production hardening, competitor verification)
- `bindings/python/pyproject.toml` — PEP 517/518 modern Python packaging
- `HnswIndex::try_search()` — non-panicking vector search with explicit dimension validation
- **Unified Multi-Protocol Graceful Shutdown**: Synchronized broadcast channel cleanly draining active connections across HTTP REST, MongoDB Wire (Port 27017), PostgreSQL Wire (Port 5432), and gRPC (Port 50051) on SIGINT/SIGTERM.
- **Proactive WAL Checkpointing & Disk Reclaim**: Periodic checkpointing and journal pruning during storage flush and compaction, preventing disk exhaustion and bounding log files.
- **MVCC Autonomous Transaction Reaper**: Background daemon sweeping every 30 seconds to abort and evict idle transactions exceeding timeout, completely eliminating snapshot version bloat.
- **Sub-Millisecond Query Scan Limit Pushdown**: Direct pushdown of `LIMIT` clauses into the document scan iterator for short-circuit evaluation without unnecessary record scanning.
- **Numerical Float Safety & Distance Clamping**: Mathematical clamping on cosine similarity (`[-1.0, 1.0]`) and distance (`[0.0, 2.0]`) to protect against IEEE 754 precision loss and eliminate `NaN` risk in HNSW indexing.
- **Bounded-Resource Graph Traversal**: Sourced-budget BFS traversal (`traverse_bfs_bounded`) with a configurable visit limit (default 50,000 nodes) and visited-node deduplication preventing infinite loops in cyclic graphs.
- **Open-Format Data Portability CLI (`faizdb dump`)**: Streaming export of collections to standard JSONL and ANSI SQL `INSERT` statements with $O(1)$ memory usage.
- **Cloud-Native Kubernetes Health Probes**: Native HTTP endpoints `GET /v1/health/liveness` (event-loop deadlock detection) and `GET /v1/health/readiness` (storage engine availability gating).
- **Autonomous Background Snapshot Daemon**: Zero-maintenance async background daemon (`FAIZDB_AUTO_BACKUP`) for periodic atomic snapshots with automatic timestamp rotation.
- **WAL Group Commit & Batch Durability**: Vectorized single-buffer atomic serialization (`append_batch` and `put_batch`) enabling 100k+ durable writes/sec with amortized `fsync`.
- **Max Connections Governor & Overload Protection**: Asynchronous admission control (`tokio::sync::Semaphore`) across PostgreSQL and MongoDB wire protocols with RFC 53300 fatal error rejection (`FATAL: 53300: sorry, too many clients already`).
- **PostgreSQL Extended Query Protocol**: Full support for `'P'`, `'B'`, `'D'`, `'E'`, `'S'`, and `'C'` wire protocol messages with parameterized queries ($1, $2) and prepared statement caching.
- **Relational SQL Multi-Table Hash Joins**: Native execution of `INNER JOIN` and `LEFT JOIN` in FaizQL with high-speed in-memory hash join algorithm.
- **MongoDB Wire Fast-Path & Stateful Cursors**: $O(1)$ primary key lookup for `{ _id: ... }` filters and stateful cursor pagination supporting `getMore` and `killCursors`.
- **Vector HNSW Dynamic Updates & Tombstones**: Tombstone-based vector deletion and in-place embedding updates without requiring full index reconstruction.

### Security & Robustness Hardening
- **Enterprise Connection Governor**: Guarded against connection exhaustion and socket starvation via configurable `FAIZDB_MAX_CONNECTIONS`.
- **PostgreSQL Wire Protocol**: Refined scalar introspection handling and query routing to ensure seamless execution of standard table queries alongside PostgreSQL administrative commands.
- **Network Buffer Protection**: Enforced strict upper-bound message limits on PostgreSQL (16MB) and MongoDB (48MB) wire listeners to prevent unbounded heap allocations on network streams.
- **Vector Search Preflight Validation**: Enforced strict preflight dimension, empty query, and `top_k` checks across REST and query endpoints with graceful HTTP 400 responses.
- **Query Optimizer Resilience**: Hardened floating-point comparisons in Cost-Based Optimizer (CBO) table statistics to guarantee stability across edge-case numerical distributions.
- **WAL Segment Durability**: Enforced clean segment truncation during WAL log rotation.
- **Code Quality & Linter Compliance**: Resolved all workspace Clippy lints to achieve full compliance with `-D warnings` strict build policy.

### Changed
- **JWT algorithm upgraded from `HS256` → `EdDSA` (Ed25519)** — asymmetric signatures, immune to timing attacks, 2026 security standard. Supply `FAIZDB_JWT_PRIVATE_KEY` / `FAIZDB_JWT_PUBLIC_KEY` (PEM) in production.
- `faizdb-server/src/api.rs` (1,529 lines) split into focused submodules:
  - `api/auth.rs` — login, whoami, token generation
  - `api/collections.rs` — CRUD, query, aggregation, search, TTL, transactions, import
  - `api/backup.rs` — create, list, restore, schedule
  - `api/cluster.rs` — Raft RPC, cluster join, geo-replication
  - `api/health.rs` — health, metrics, server info, audit logs
  - `api/middleware.rs` — CORS, auth, RBAC, rate limiter, audit logger
  - `api/websocket.rs` — Change Stream WebSocket handlers
  - `api/mod.rs` — Router assembly
- `faizdb-core/src/storage/engine.rs` — replaced `std::sync::RwLock` with `parking_lot::RwLock` (no lock poisoning, no `.unwrap()`)
- `faizdb-core/src/storage/compaction.rs` — `merge_sstables()` now uses a **streaming k-way BinaryHeap merge** instead of loading all entries to RAM. Memory usage bounded to `O(k)` regardless of dataset size.
- `AppState.backup_schedule` changed from `std::sync::RwLock` to `parking_lot::RwLock`
- Rust minimum version bumped to **1.88** (latest stable Aug 2026)
- `tokio` bumped to **1.53.1** (latest)
- `rand` bumped to **0.9**
- `docker-compose.yml` — removed deprecated `version: '3.8'` field
- `Dockerfile` — `as builder` → `AS builder` (OCI spec compliant)
- `bindings/python/setup.py` — `python_requires` bumped from `>=3.8` to `>=3.11`
- Kubernetes `statefulset.yaml`:
  - `imagePullPolicy: IfNotPresent` → `Always` (prevents stale `:latest` images)
  - Added `startupProbe` (60s startup window before liveness checks)
  - Added missing ports `5432` (PostgreSQL wire) and `50051` (gRPC) to Service

### Fixed
- Repository URL in `Cargo.toml` corrected from `github.com/faizdb/faizdb` → `github.com/ictdothouse/faizdb`
- Author email corrected from `faiz@faizdb.io` → `faiz@ict.house`

---

## [0.1.0] — 2026-08-15 *(Initial Release)*

### Added
- Hybrid LSM-Tree + B-Tree storage engine with WAL crash-safety
- HNSW vector index (Cosine, Euclidean, Inner Product distance metrics)
- Adjacency-list property graph engine with BFS/DFS traversal
- 4-way protocol gateway: MongoDB Wire (27017), PostgreSQL Wire (5432), gRPC (50051), REST/WS (27018)
- FaizQL query language with SQL and MongoDB shell dialect support
- Aggregation pipeline engine (`$match`, `$group`, `$sort`, `$project`, `$limit`, `$skip`)
- Raft consensus for multi-node clustering
- CRDT-based geo-replication for multi-region deployments
- Change Streams via WebSocket (per-collection or global)
- AES-256-GCM encrypted backup/restore with manifest checksum verification
- Argon2id password hashing with RBAC (Admin, ReadWrite, ReadOnly)
- Enterprise-grade middleware: CORS, rate limiting, blocklist, payload limits, audit logging
- Prometheus-compatible metrics endpoint (`/v1/metrics`)
- Docker Compose and Kubernetes StatefulSet deployment manifests
