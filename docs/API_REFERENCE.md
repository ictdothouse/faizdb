# 📚 FaizDB Universal API & Protocols Reference

FaizDB features a unified, multi-protocol engine running across 4 standard ports:

| Protocol | Default Port | Primary Target & Compatibility |
| :--- | :---: | :--- |
| 🍃 **MongoDB Wire Protocol** | `27017` | Drop-in replacement for Mongoose, PyMongo, Prisma, BSON apps. |
| 🐘 **PostgreSQL Wire Protocol** | `5432` | Drop-in compatibility for `psql`, DBeaver, TablePlus, Grafana, SQL ORMs. |
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

FaizDB enforces Zero-Trust Role-Based Access Control (RBAC) with HMAC-SHA256 JWT tokens.

### 1. `POST /v1/auth/login`
Authenticates a user and returns a signed JWT token.
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
      "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
      "username": "admin",
      "role": "Admin",
      "expires_in": 2592000
    }
  }
  ```

### 2. `POST /v1/auth/token` (Admin only)
Generates custom API service tokens with granular role scoping (`Admin`, `ReadWrite`, `ReadOnly`).

---

## 🗄️ 4. Query & Document Operations

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
* **EXPLAIN Response:**
  ```json
  {
    "success": true,
    "data": {
      "Explain": {
        "plan_type": "IndexScan(idx_email)",
        "collection": "users",
        "index_used": "idx_email",
        "execution_time_us": 74,
        "documents_examined": 1,
        "documents_returned": 1,
        "is_unique": true,
        "estimated_cost_score": 1.05
      }
    }
  }
  ```

---

## 📥 5. Bulk Data Migration & Ingestion

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

## ⚡ 6. Secondary Indexes & Unique Constraints

* **`POST /v1/collections/:name/indexes`** — Creates an $O(\log N)$ B-Tree secondary index (`{ "field": "email", "unique": true }`).
* **`GET /v1/collections/:name/indexes`** — Lists all active secondary indexes.
* **`DELETE /v1/collections/:name/indexes/:field`** — Drops a secondary index.

---

## 💳 7. Multi-Document ACID Transactions

* **`POST /v1/transaction/begin`** — Starts an isolated snapshot transaction.
* **`POST /v1/transaction/commit`** — Atomically flushes staged mutations to the Write-Ahead Log (WAL).
* **`POST /v1/transaction/rollback`** — Discards staged write-buffer.

---

## 🔒 8. Disaster Recovery & Automated Backup Scheduler (SOC2)

* **`POST /v1/backup/create`** — Creates an atomic consistent point-in-time snapshot.
* **`GET /v1/backup/schedule`** — Fetches the active automated backup schedule.
* **`POST /v1/backup/schedule`** — Updates backup schedule and retention policy.
* **`POST /v1/backup/restore`** — Restores database state from a snapshot archive.
