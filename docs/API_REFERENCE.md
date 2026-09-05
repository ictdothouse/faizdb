# 📚 FaizDB Universal API & Protocols Reference

FaizDB features a unified, multi-protocol engine running across 4 standard ports:

| Protocol | Default Port | Primary Target & Compatibility |
| :--- | :---: | :--- |
| 🐬 **MySQL / MariaDB Wire Protocol** | `3306` | Drop-in compatibility for MySQL CLI, PHP `mysqli`/PDO, Laravel, WordPress. |
| 🐘 **PostgreSQL Wire Protocol** | `5432 / 5433` | Drop-in compatibility for `psql`, DBeaver, TablePlus, Grafana, SQL ORMs. |
| 🍃 **MongoDB Wire Protocol** | `27017` | Drop-in replacement for Mongoose, PyMongo, Prisma, BSON apps. |
| ⚡ **gRPC & Protocol Buffers** | `50051` | Ultra low-latency binary microservices, AI vector search, and streaming events. |
| 🌐 **HTTP REST & WebSocket Bus**| `27018` | Browser management studio, IoT streams, and OpenAPI endpoints. |

---

## ⚡ 1. gRPC & Protocol Buffers (`Port 50051`)

Defined in standard [`proto/faizdb.proto`](../proto/faizdb.proto):

### RPC Methods:
1. **`ExecuteQuery(QueryRequest) returns (QueryResponse)`**
   * Executes SQL, Mongo JSON, or FaizQL AST queries.
2. **`VectorSearch(VectorSearchRequest) returns (VectorSearchResponse)`**
   * Sub-millisecond Cosine / Euclidean / Manhattan similarity search on high-dimensional vectors.
3. **`InsertDocuments(InsertRequest) returns (InsertResponse)`**
   * High-throughput bulk document ingestion.
4. **`SubscribeChangeStream(StreamRequest) returns (stream ChangeEventMsg)`**
   * Server-streaming reactive change feeds for real-time document mutations.
5. **`HealthCheck(HealthRequest) returns (HealthResponse)`**
   * Fast liveness and cluster readiness probe.

---

## 🌍 2. Multi-Region Geo-Replication & CRDTs (`/v1/cluster`)

FaizDB provides active-active multi-datacenter replication with mathematically guaranteed *Strong Eventual Consistency* using Conflict-Free Replicated Data Types (CRDTs):

### 1. `GET /v1/cluster/regions`
Lists local region ID and all registered peer datacenter regions.
* **Response:**
  ```json
  {
    "success": true,
    "data": {
      "local_region": "ap-southeast-1",
      "peer_count": 2,
      "regions": [
        {
          "region_id": "us-east-1",
          "endpoint": "http://us.faizdb.io:27018",
          "is_active": true,
          "last_synced_at": "2026-09-01T21:00:00Z",
          "latency_ms": 142
        }
      ]
    }
  }
  ```

### 2. `POST /v1/cluster/regions`
Registers a new peer region in the global replication mesh.
* **Request:**
  ```json
  {
    "region_id": "eu-central-1",
    "endpoint": "http://eu.faizdb.io:27018"
  }
  ```

### 3. `POST /v1/cluster/geo-sync`
Transmits and applies incoming CRDT `ReplicationDelta` batches across regions.
* **Request:**
  ```json
  {
    "deltas": [
      {
        "source_region": "us-east-1",
        "collection": "customers",
        "document_id": "cust_100",
        "field_updates": {
          "tier": ["Enterprise", 1700000000000, "us-east-1"]
        },
        "version_vector": { "versions": { "us-east-1": 1 } },
        "timestamp": 1700000000000
      }
    ]
  }
  ```

---

## 🔐 3. Authentication & RBAC

FaizDB enforces Zero-Trust Role-Based Access Control (RBAC) with **EdDSA (Ed25519)** asymmetric JWT tokens (2026 industry standard).
* Immune to timing attacks and HMAC key brute-forcing.
* Supply standard PEM keys via `FAIZDB_JWT_PRIVATE_KEY` and `FAIZDB_JWT_PUBLIC_KEY` in production, or let FaizDB auto-generate ephemeral keys during local testing.

### 1. `POST /v1/auth/login`
Authenticates a user and returns a signed Ed25519 JWT token.
* **Request:**
  ```json
  {
    "username": "admin",
    "password": "faizdb-admin-2026"
  }
  ```
* **Response:**
  ```json
  {
    "success": true,
    "data": {
      "token": "eyJhbGciOiJFZERTQSI...eyJzdWIiOiJhZG1pbiIsInJvbGUiOiJBZG1pbiJ9...",
      "username": "admin",
      "role": "Admin",
      "expires_in": 3600
    }
  }
  ```

### 2. `GET /v1/auth/whoami`
Returns claims and permissions of the currently authenticated JWT bearer token.
* **Header:** `Authorization: Bearer <TOKEN>`

### 3. `POST /v1/auth/token` (Admin only)
Generates custom service account API tokens with granular role scoping (`Admin`, `ReadWrite`, `ReadOnly`) and custom expiration windows.

---

## 👥 4. User Management API (`/v1/users`)

All user management endpoints require an authenticated `Admin` token.

### 1. `GET /v1/users`
Lists all active database user accounts and their assigned RBAC roles.
* **Response:**
  ```json
  {
    "success": true,
    "data": [
      { "username": "admin", "role": "Admin", "created_at": 1700000000 },
      { "username": "analyst", "role": "ReadOnly", "created_at": 1700001000 }
    ]
  }
  ```

### 2. `POST /v1/users`
Creates a new user account with an Argon2id-hashed password.
* **Request:**
  ```json
  {
    "username": "developer",
    "password": "SuperSecretPassword2026",
    "role": "readwrite"
  }
  ```

### 3. `PUT /v1/users/:username/password`
Updates the password for an existing user account.
* **Request:**
  ```json
  {
    "password": "NewUpdatedPassword2026"
  }
  ```

### 4. `DELETE /v1/users/:username`
Deletes a user account. Protected by a safety guard: cannot delete the last remaining administrator.

---

## 🗄️ 5. Query & Full Document CRUD Operations

FaizDB provides full REST document operations alongside multi-dialect query processing.

### 1. `POST /v1/query`
Executes multi-dialect SQL, MongoDB JSON, FaizQL, or EXPLAIN plan queries.
* **Header:** `Authorization: Bearer <TOKEN>`
* **Request:**
  ```json
  { "query": "SELECT * FROM users WHERE active = true" }
  ```
* **EXPLAIN Query Example:**
  ```json
  { "query": "EXPLAIN SELECT * FROM users WHERE email = 'faiz@ict.house'" }
  ```

### 2. `POST /v1/collections/:name/documents`
Inserts a single document or multiple documents into a collection.

### 3. `PUT /v1/collections/:name/documents/:id` (Full Replacement)
Replaces the entire document body while preserving or validating the document `_id`.
* **Request:**
  ```json
  {
    "name": "Faiz Aziz",
    "role": "Chief Architect",
    "tier": "Enterprise"
  }
  ```

### 4. `PATCH /v1/collections/:name/documents/:id` (Partial Update & Operators)
Performs partial field merges or executes atomic MongoDB operators (`$set`, `$inc`, `$unset`).
* **Operator Request Example:**
  ```json
  {
    "$set": { "verified": true, "status": "active" },
    "$inc": { "login_count": 1, "credits": 50 },
    "$unset": { "temporary_token": "" }
  }
  ```
* **Direct Field Merge Example:**
  ```json
  {
    "last_login_at": "2026-09-04T06:30:00Z"
  }
  ```

### 5. `DELETE /v1/collections/:name/documents/:id`
Deletes a document by ID and emits a `delete` ChangeStream event.

### 6. `POST /v1/collections/:name/aggregate`
Runs a multi-stage aggregation pipeline across collections with `$lookup` joins, `$match`, `$group`, `$sort`, `$project`, and `$unwind`.
* **Request:**
  ```json
  {
    "pipeline": [
      { "$match": { "active": true } },
      {
        "$lookup": {
          "from": "profiles",
          "localField": "user_id",
          "foreignField": "_id",
          "as": "user_profile"
        }
      },
      { "$limit": 20 }
    ]
  }
  ```

---

## 🧠 6. AI Vector Search & Transactional GraphRAG

FaizDB is the first engine to unify multi-hop graph traversal and vector ranking in a single query.

### 1. Vector Management Endpoints:
* **`POST /v1/vector/index`**: Creates an HNSW index with metric (`cosine`, `euclidean`, `dot`) and dimensions (e.g. 1536).
* **`POST /v1/vector/insert`**: Inserts an embedding vector (`[0.12, 0.45, ...]`) associated with a document ID.
* **`POST /v1/vector/search`**: Executes sub-millisecond Top-K ANN search with optional scalar/binary quantization.

### 2. Knowledge Graph Endpoints:
* **`POST /v1/graph/vertices`**: Adds or updates graph nodes with metadata.
* **`POST /v1/graph/edges`**: Creates directed relationships between vertices (e.g. `source`, `target`, `relationship: "cites"`).

### 3. Transactional GraphRAG Unified Query:
Execute multi-hop traversal and vector ranking in a single FaizQL statement:
```sql
FIND research_papers 
TRAVERSE FROM "paper_01" DEPTH 2 VIA "cites" 
VECTOR [0.12, 0.45, 0.88, 0.05] USING INDEX paper_embeddings 
LIMIT 5;
```

---

## 📥 7. Bulk Data Migration & Ingestion

### 1. `POST /v1/collections/:name/import`
Fast bulk import supporting either JSON documents array or CSV string with automatic type inference.
* **JSON Array Request:**
  ```json
  {
    "documents": [
      { "name": "Faiz", "role": "Architect", "active": true },
      { "name": "Elena", "role": "Engineer", "active": true }
    ]
  }
  ```
* **CSV String Request:**
  ```json
  {
    "csv": "name,role,active,price\nFaiz,Architect,true,99.50\nElena,Engineer,true,80.00"
  }
  ```
* **Response:**
  ```json
  {
    "success": true,
    "data": {
      "imported_count": 2,
      "inserted_ids": ["01a05c26-...", "01a05c26-..."],
      "failed_count": 0,
      "errors": null
    }
  }
  ```

---

## ⚡ 8. Secondary Indexes & Unique Constraints

* **`POST /v1/collections/:name/indexes`** — Creates an $O(\log N)$ B-Tree secondary index (`{ "field": "email", "unique": true }`).
* **`GET /v1/collections/:name/indexes`** — Lists all active secondary indexes.
* **`DELETE /v1/collections/:name/indexes/:field`** — Drops a secondary index.

---

## 💳 9. Multi-Document ACID Transactions

* **`POST /v1/transaction/begin`** — Starts an isolated snapshot transaction.
* **`POST /v1/transaction/commit`** — Atomically flushes staged mutations to the Write-Ahead Log (WAL).
* **`POST /v1/transaction/rollback`** — Discards staged write-buffer.

---

## 🔒 10. Disaster Recovery & Automated Backup Scheduler (SOC2)

* **`POST /v1/backup/create`** — Creates an atomic consistent point-in-time snapshot.
* **`GET /v1/backup/schedule`** — Fetches the active automated backup schedule.
* **`POST /v1/backup/schedule`** — Updates backup schedule and retention policy.
* **`POST /v1/backup/restore`** — Restores database state from a snapshot archive.

---

## 🔌 11. Native Wire Protocols (MySQL, PostgreSQL & MongoDB)

FaizDB operates 3 native wire protocol listeners alongside HTTP REST and gRPC:

### 1. 🐬 MySQL / MariaDB Wire Protocol (`Port 3306`):
* **Handshake Negotiation**: Native `HandshakeV10` protocol greeting returning server version `8.0.35-FaizDB-Universal`.
* **Authentication**: Supports `HandshakeResponse41` with `mysql_native_password` authentication and database selection.
* **Driver Support**: Direct drop-in compatibility for MySQL CLI, PHP `mysqli`, PDO, Laravel Eloquent (`DB_CONNECTION=mysql`), and WordPress (`wp-config.php`).
* **Connection String**: `mysql -h 127.0.0.1 -P 3306 -u root faizdb` or `mysql://root@127.0.0.1:3306/faizdb`

### 2. 🐘 PostgreSQL Wire Protocol (`Port 5432 / 5433`):
* **Authentication Challenge**: Employs PostgreSQL `AuthenticationCleartextPassword` ('R', code 3).
* **Verification**: Checks credentials against `UserStore` with Argon2id hashing.
* **Rejection**: Invalid credentials or missing passwords trigger PostgreSQL `FATAL 28P01` error response and connection termination.
* **Connection String**: `postgresql://admin:password@localhost:5432/faizdb`

### 3. 🍃 MongoDB Wire Protocol (`Port 27017`):
* **Driver Support**: Compatible with PyMongo, Mongoose, MongoDB Shell (`mongosh`), and Prisma.
* **OP_MSG Support**: Handles document insert, find, update, and multi-stage `aggregate` pipelines including `$lookup` cross-collection joins.
* **Connection String**: `mongodb://localhost:27017/faizdb`

---

## 🕸️ 12. openCypher Graph Syntax & AI Semantic Caching

FaizDB provides native openCypher support for knowledge graphs and hybrid GraphRAG queries:

### 1. Node Creation & Filtering
```cypher
-- Create node with properties:
CREATE (n:Person {id: 'p1', name: 'Alice', age: 30});

-- Match nodes with property filters:
MATCH (n:Person) WHERE n.age >= 18 RETURN n;
```

### 2. Relationship Creation & Multi-Hop Traversal
```cypher
-- Create directed edge with weight:
CREATE (a {id: 'p1'})-[:KNOWS {weight: 1.0}]->(b {id: 'p2'});

-- 1-hop traversal:
MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.id = 'p1' RETURN b;

-- Variable-depth traversal (up to 3 hops):
MATCH (a:Person {id: 'p1'})-[:KNOWS*1..3]->(b:Person) RETURN b;
```

### 3. Hybrid GraphRAG + Vector Search
```cypher
-- Combined Graph Traversal + HNSW Vector Ranking in ONE atomic query:
MATCH (a:docs)-[:REFERENCES]->(b:docs) WHERE a.id = 'doc1' VECTOR NEAR [0.95, 0.88, 0.12] TOP 5 RETURN b;
```

### 4. In-Memory AI Semantic Cache
Built-in `SemanticCache` performs sub-millisecond cosine similarity matching on prompt embeddings ($\ge 0.90$ threshold) with configurable TTL, preventing redundant LLM GraphRAG traversals and database scans.

---

## ⚙️ 13. LSM-Tree Anti-Stall Engine & Storage Tuning

FaizDB provides self-tuning write backpressure to prevent Level-0 SSTable file bloat and eliminate write stalls under high-throughput burst workloads:

### StorageConfig Parameters

| Parameter | Type | Default | Description |
|:---|:---:|:---:|:---|
| `l0_compaction_trigger` | `usize` | `4` | Number of Level-0 SSTables that triggers asynchronous background compaction. |
| `l0_slowdown_writes_trigger` | `usize` | `8` | Soft threshold: Injects microsecond write backpressure (`yield_now`) to throttle incoming writes and allow compaction to catch up. |
| `l0_stop_writes_trigger` | `usize` | `16` | Hard threshold: Enforces synchronous foreground compaction before accepting further writes, preventing unbounded file descriptor accumulation. |
| `memtable_size` | `usize` | `64 MB` | In-memory skiplist threshold before auto-flushing to an immutable Level-0 SSTable. |
| `sync_writes` | `bool` | `false` | When `true`, every write enforces a strict synchronous `fsync` to the WAL. |
| `enable_wal` | `bool` | `true` | Enables atomic write-ahead logging with CRC32 framing and LSN ordering. |
| `block_cache_size` | `usize` | `64 MB` | Adaptive Replacement Cache (ARC) capacity for frequent SSTable block hits. |

### Diagnostic Metrics (`StorageStats`)

```rust
let stats = engine.stats();
println!("Active SSTables: {}", stats.sstable_count);
println!("Write Stalls: {}", stats.write_stalls);
println!("Compactions Completed: {}", stats.compactions_completed);
```

---

## 🐘 14. PostgreSQL Virtual System Catalog Reflection

To provide seamless, zero-config compatibility with modern ORMs and database management tools (Prisma, Drizzle, SQLAlchemy, DBeaver, TablePlus), FaizDB's PostgreSQL wire protocol (Port 5432) synthesizes virtual catalog responses for standard metadata inspection queries:

| Catalog View / Table | Handled Columns | Supported Tools & ORMs |
|:---|:---|:---|
| `pg_catalog.pg_database` | `datname`, `encoding`, `datcollate`, `datctype` | DBeaver, TablePlus, Navicat |
| `pg_catalog.pg_namespace` | `nspname`, `nspowner`, `nspacl` | Prisma, Drizzle, pgAdmin |
| `pg_catalog.pg_type` | `typname`, `typnamespace`, `typlen`, `typtype` | SQLAlchemy, Diesel, sqlx |
| `information_schema.columns` | `table_schema`, `table_name`, `column_name`, `data_type` | ORM Schema Migrators & Introspectors |

---

## 🛡️ 15. Torn-Write Crash Recovery & WAL Integrity Assurance

FaizDB guarantees mathematical crash durability against abrupt power failures, kernel panics, and torn disk writes:

* **CRC32 Frame Validation:** Every WAL record is protected by a 32-bit CRC checksum header (`magic: 0xFDB00001`).
* **Payload Bounds Verification:** Deserialization performs explicit length validation (`pos + key_len + 4 <= payload_len`), preventing buffer overflow or slice panics on corrupted file tails.
* **Safe Tail Truncation:** If a partial or corrupted write is encountered at the end of the log during startup replay, FaizDB safely logs a diagnostic warning, truncates the torn tail at the last valid LSN boundary, and recovers 100% of committed pre-crash transactions without crashing.


