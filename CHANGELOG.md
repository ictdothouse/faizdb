# Changelog

All notable changes to **FaizDB** are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

- **Phase 5: In-Browser WebAssembly (WASM) Headless Engine (`bindings/wasm`)**: Native browser and Node.js WebAssembly package built with `wasm-bindgen`, exposing pure in-memory zero-server FaizDB execution, collections CRUD, JSON document serialization, and client-side HNSW vector similarity search with Cosine, Euclidean, and Dot Product metrics directly inside browser web applications.
- **Phase 4: Distributed Scatter-Gather Query Coordinator (`faizdb-core/src/cluster/scatter_gather.rs`)**: 16,384 virtual hash slot routing engine with `{hash_tag}` extraction for colocation, parallel partition query scattering, multi-field sort-merge (`ORDER BY ... ASC/DESC`) with streaming `LIMIT`/`OFFSET`, and distributed columnar analytical aggregations (`COUNT`, `SUM`, `MIN`, `MAX`, `AVG`) with pushdown to remote shards.
- **Phase 4: Active Outbound CDC Stream Dispatcher (`faizdb-server/src/stream/cdc_dispatcher.rs`)**: Real-time event streaming pipeline pushing database mutations (`insert`, `update`, `delete`) to Kafka REST proxies, Debezium gateways, and HTTP webhooks with bounded buffer queues, exponential retry with jitter, batching (`batch_size`, `flush_interval_ms`), and live Prometheus/atomic telemetry.
- **Phase 3: Automated Tiered Storage Architecture (`StorageEngine` + `TieredStorageManager`)**: Transparent point lookups (`get`) and prefix scans (`prefix_scan`) querying Hot NVMe and Cold HDD/Blob tiers seamlessly; dual-tier SSTable reader management (`cold_sstables: RwLock<Vec<SSTableReader>>`) with ARC block caching; autonomous background tier migration triggered on MemTable flushes; cold SSTables persistence and recovery across restarts; real-time telemetry tracking hot vs. cold bytes and tables.
- **Phase 3: Hardware SIMD Vector Acceleration (`faizdb-vector`)**: 8-lane SIMD-unrolled vector math loops for Cosine Distance, Squared Euclidean Distance, and Dot Product distance metrics with trailing remainder handlers, utilizing 256-bit AVX2 / ARM NEON vectorization for high-dimensional embeddings (1536-dim OpenAI, 4096-dim Llama).
- **Phase 3: Columnar Analytical Aggregations (`ColumnarBatch`)**: In-memory analytical aggregation operators (`avg_f64`, `min_f64`, `max_f64`, `count`) operating directly on columnar vectors without full document deserialization overhead.
- **5-Way Universal Protocol Gateway & Native MySQL / MariaDB Wire Ingress (Port 3306)**: Complete async MySQL HandshakeV10 protocol engine (`faizdb-server/src/wire/mysql/`), supporting MySQL CLI, Laravel Eloquent (`DB_CONNECTION=mysql`), PHP PDO/mysqli, ColumnDef41 packet encoding, EOF/OK packets, and automated test suite (`tests/test_mysql_wire_protocol.rs`).
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
- **Bounded Working-Set Disk Fallback & Data Integrity Hardening**: Closed data-loss edge cases when `max_memory_documents` is enabled by integrating transparent disk fallback across `find_all`, `delete_by_id`, `update_by_id`, `update_many`, and duplicate key validation against disk storage, while strictly enforcing memory caps on cache misses.
- **Collection Lifecycle & Persistent Storage Purging**: Eliminated zombie records where `DROP TABLE`, `DROP COLLECTION`, or `db.collection.drop()` only evicted RAM structures; now atomically scans and purges all underlying LSM-Tree SSTable/WAL keys matching `doc:{name}:`.
- **REST Collection Dropping Route**: Introduced `DELETE /v1/collections/{name}` REST endpoint with RBAC write authentication for complete collection lifecycle management.
- **Memory Cap Enforcement on Recovery**: Hardened `Collection::load_document` with memory cap enforcement preventing out-of-memory blowouts during crash recovery or snapshot restores.
- **Defensive SSTable & WAL Zero-Copy Bounds**: Hardened SSTable `read_entry_ref` and WAL `from_reader` against integer overflow and unbounded memory allocations via checked arithmetic and `MAX_WAL_SIZE` caps.
- **Cross-Tier LSM Tombstone Retention (Zombie Resurrection Prevention)**: Preserved tombstones during hot-tier compaction whenever cold SSTables exist (`!cold_sstables.is_empty()`), eliminating zombie record resurrection across hybrid storage tiers.
- **Deterministic Compaction Path Precedence**: Enforced ascending age order (`.iter().rev()`) when merging SSTables, ensuring newest updates deterministically supersede stale records in `merge_sstables`.
- **Terminal Tier Cold Compaction**: Added `compact_cold()` to reclaim space and permanently purge tombstones from cold storage without application downtime.
- **Defensive Vector Slice Clamping**: Guarded SIMD vector distance calculations against out-of-bounds slice indexing via `a.len().min(b.len())`.
- **Zero-Allocation Columnar Aggregation**: Optimized `avg_f64` to accumulate sums in a single pass without allocating temporary vectors.
- **SQL DDL Quoting & `IF [NOT] EXISTS` Parsing**: Hardened DDL and DML statements to strip backticks (`` ` ``) and double quotes (`"`), correctly parsing `CREATE TABLE IF NOT EXISTS` and `DROP TABLE IF EXISTS` without corrupting table names.
- **Graph Edge Query Durability**: Extended FaizQL query execution to write graph edge creations and deletions directly to durable LSM storage (`graph:e:`), guaranteeing edge persistence across reboots. Added native support for `CREATE EDGE [FROM] ... TO ... VIA ... [WEIGHT ...]` and `DELETE EDGE [FROM] ... TO ... [VIA ...]`.
- **Offline TTL Expiration / Zombie Purge**: Fixed document recovery on restart to compute absolute TTL expiration (`created_at + ttl_secs`); expired documents are purged immediately from storage upon reboot instead of having their TTL reset.
- **MySQL Wire Protocol Column Typing**: Correctly mapped string/UUID IDs to `MYSQL_TYPE_VAR_STRING (0xFD)` instead of `MYSQL_TYPE_LONGLONG (0x08)`, preventing client drivers (e.g. PHP PDO, Laravel, Go, Python) from failing on UUID v7 strings.
- **SQL Comment Stripping & Tautology `1 = 1` Predicates**: Implemented robust SQL comment stripping (`--`, `#`, `/* ... */`), case-insensitive ` AND ` compound filters, and instant resolution of boolean/numeric tautologies (`WHERE 1=1`) commonly emitted by ORMs (Prisma, Drizzle, Hibernate).
- **SQL Keyword Boundary & Substring Collision Isolation**: Implemented `find_keyword_top_level` with word boundaries (`is_ascii_whitespace`, `;`, `(`, `)`, `,`) while preserving quoted literals, preventing substring collisions on table names like `settings`, `assets`, or `warehouse`, and index names like `idx_location`.
- **SQL Range (`BETWEEN` / `NOT BETWEEN`) & Set Membership (`IN` / `NOT IN`)**: Resolved parser dropping of `BETWEEN ... AND ...` ternary predicates, and added complete support for `NOT BETWEEN`, `IN (...)`, `NOT IN (...)`, and `OR` with parenthesized precedence in `parse_sql_where`.
- **PostgreSQL Wire Parse Message Negative Parameter Count Underflow Guard**: Enforced `raw_num_params > 0` validation and buffer bounds in `faizdb-server/src/wire/postgres/listener.rs`, eliminating a 100% CPU lock loop when receiving `-1` parameter count codes.
- **HNSW Vector Tombstone Query Consistency**: Updated `HnswIndex::try_search` to verify `self.is_empty()`, matching `search()` behavior on indexes where all nodes have been tombstone-deleted.
- **MVCC Write History Autonomous Memory Pruning**: Integrated auto-clearing and `gc()` trigger in `TransactionManager::commit` when transactions finish, preventing unbounded growth of `committed_writes` over millions of transactions.
- **REST Vector Deletion & Axum 0.7 Routing**: Added `DELETE /v1/vector/{index_name}/{id}` and `DELETE /v1/vector/index/{name}` endpoints with correct Axum 0.7 `{param}` URL syntax.
- **Histogram NaN Ingestion Guard**: Filtered non-finite floats in cost-based query optimizer histograms to prevent `NaN` step intervals.
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
- **MVCC Atomic Conflict Validation (TOCTOU Race Prevention)**: Fixed race condition in `TransactionManager::commit` where conflict validation and write insertion were split across separate read and write locks, allowing concurrent transactions with overlapping write sets to commit simultaneously without detecting conflicts. Validation and insertion are now executed atomically under a single write lock.
- **WAL Sequence Continuity on Empty Rotated Segments**: Fixed sequence counter reset bug in `find_or_create_wal_file` where a freshly rotated segment with 0 records reset the sequence number counter to 0 upon reboot, colliding with earlier WAL files. The engine now scans existing segments in reverse to maintain sequence number monotonicity.
- **PostgreSQL Wire Extended Query Parameter Substitution & Bounds Protection**: Fixed query mangling where `$1` corrupted `$10`, `$11`, etc., and string literals containing `'$1'`. Implemented tokenizer-based substitution (`substitute_postgres_params`) matching whole `$N` tokens outside quoted string literals. Added bounds validation on `num_formats` and `num_params` preventing integer underflow panics.
- **MongoDB Wire Cursor TTL Eviction**: Fixed unbounded memory growth in `CURSOR_CACHE` where abandoned queries left paginated cursors in RAM indefinitely. Added automated reaper evicting cursors older than 10 minutes (600s).
- **Document Primary Identifier (`_id` / `id`) Query & Sort Resolution**: Fixed filter evaluation and query sorting where referencing `_id` or `id` returned `None` because fields only checked nested document attributes. `Collection::find` and SQL/MongoDB sorting now resolve `doc.id` natively.
- **Relational Hash Join NULL Key Isolation & Canonical Stringification**: Fixed bug where missing foreign keys (`None`) defaulted to empty string (`""`), causing unrelated records without keys to join. INNER JOIN now strictly rejects NULL/missing keys according to ANSI SQL standards, and formats numeric, boolean, UUID, and datetime keys canonically.
- **ANSI SQL WHERE Operators (`<>`, `IS NULL`, `IS NOT NULL`, `LIKE`)**: Added support for standard ANSI SQL inequality (`<>`), nullity checks (`IS NULL`, `IS NOT NULL`), and wildcard patterns (`LIKE '%pattern%'`, `'prefix%'`, `'%suffix'`).
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
