# Changelog

All notable changes to **FaizDB** are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Added
- `SECURITY.md` — responsible disclosure policy
- `CONTRIBUTING.md` — full contributor guide
- `.github/workflows/ci.yml` — GitHub Actions CI (fmt, clippy -D warnings, tests, cargo-audit, MSRV)
- `faizdb-server/tests/` — Rust integration test suite (auth flow, document CRUD, vector search)
- `bindings/python/pyproject.toml` — PEP 517/518 modern Python packaging

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
