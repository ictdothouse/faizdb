# 📖 FaizDB Universal Commands & Syntax Reference Manual

This is the definitive, exhaustive command and syntax manual for **FaizDB**. It covers all **5 query dialects**, the **CLI toolchain**, the **HTTP REST endpoints**, the **gRPC Protocol Buffers RPCs**, and the **Embedded In-Process Engine**.

---

## 📑 Table of Contents

1. [CLI Toolchain Commands (`faizdb`)](#1-cli-toolchain-commands)
2. [Relational SQL Syntax (Port 5432 / 3306 / REST / Shell)](#2-relational-sql-syntax)
3. [openCypher Knowledge Graph Syntax (Port 5432 / 3306 / REST)](#3-opencypher-knowledge-graph-syntax)
4. [FaizQL Native Multi-Model Syntax (REST / gRPC / Shell)](#4-faizql-native-multi-model-syntax)
5. [MongoDB Wire & Shell Syntax (Port 27017 / REST)](#5-mongodb-wire--shell-syntax)
6. [HTTP REST API Endpoints Reference (Port 27018)](#6-http-rest-api-endpoints-reference)
7. [gRPC Protocol Buffers RPC Reference (Port 50051)](#7-grpc-protocol-buffers-rpc-reference)
8. [Python SDK & LangGraph Native Saver](#8-python-sdk--langgraph-native-saver)
9. [Embedded In-Process Rust Engine (`faizdb-core`)](#9-embedded-in-process-rust-engine)

---

## 1. CLI Toolchain Commands

All commands can be run via the compiled binary `faizdb` (or `./target/release/faizdb`):

```bash
faizdb [SUBCOMMAND] [OPTIONS]
```

### Core Subcommands

| Command | Description | Key Options / Flags |
| :--- | :--- | :--- |
| `faizdb serve` | Starts the 5-way universal database daemon | `--host 0.0.0.0`, `--http-port 27018`, `--grpc-port 50051`, `--mongo-port 27017`, `--pg-port 5432`, `--mysql-port 3306`, `--data-dir ./faizdb_data` |
| `faizdb shell` | Starts the interactive multi-dialect REPL | `--endpoint http://localhost:27018`, `--token <jwt>`, `--username <user>`, `--password <pass>` |
| `faizdb query "<query>"` | Runs a one-shot query from terminal | `--endpoint http://localhost:27018`, `--format json\|table\|csv` |
| `faizdb backup` | Creates a non-blocking snapshot archive | `--output ./backups/snap.enc.json`, `--encrypt`, `--key <aes_key>` |
| `faizdb restore` | Restores snapshot & replays WAL logs | `--input ./backups/snap.enc.json`, `--replay-wal`, `--target-lsn <lsn>` |
| `faizdb benchmark` | Executes high-concurrency throughput stress test | `--threads 16`, `--operations 100000`, `--workload read_heavy\|write_heavy\|mixed` |
| `faizdb ycsb` | Runs official standard YCSB benchmarks | `--workload A\|B\|C\|D\|E\|F`, `--recordcount 100000`, `--operationcount 500000` |
| `faizdb cluster join` | Joins node to a distributed Raft cluster | `--peer 10.0.0.1:27018`, `--node-id node_02`, `--raft-port 7000` |
| `faizdb cluster status` | Displays cluster topology and shard heatmaps | `--endpoint http://localhost:27018` |
| `faizdb version` | Prints binary version, git commit, and build info | None |
| `faizdb help` | Prints general or subcommand help screen | `faizdb help [subcommand]` |

### Practical CLI Examples

```bash
# Start full multi-gateway daemon in production mode
faizdb serve --pg-port 5432 --mysql-port 3306 --mongo-port 27017 --http-port 27018 --grpc-port 50051

# Execute a one-shot SQL query and output as formatted table
faizdb query "SELECT * FROM users WHERE score > 9000 LIMIT 5;" --format table

# Generate an encrypted disaster recovery backup
faizdb backup --output /var/backups/faizdb_20260906.enc.json --encrypt

# Point-In-Time Disaster Recovery to exact microsecond LSN
faizdb restore --input /var/backups/faizdb_20260906.enc.json --replay-wal --target-lsn 849204820
```

---

## 2. Relational SQL Syntax

FaizDB parses standard ANSI SQL with Cost-Based Optimization (`faizdb-query`), available over **Port 5432 (Postgres)**, **Port 3306 (MySQL)**, **Port 27018 (`/v1/query`)**, and **CLI shell**.

### A. Data Definition Language (DDL)

```sql
-- Create a new table / collection
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    role TEXT,
    score INT,
    created_at TIMESTAMP
);

-- Drop a table
DROP TABLE IF EXISTS users;

-- Create Secondary B-Tree Index for O(log N) lookup
CREATE INDEX idx_users_score ON users (score);

-- Create Unique Constraint Index
CREATE UNIQUE INDEX idx_users_email ON users (email);

-- Drop Secondary Index
DROP INDEX idx_users_score ON users;

-- Collect distribution statistics for the Cost-Based Optimizer
ANALYZE users;
```

### B. Data Manipulation Language (DML)

```sql
-- Insert Row using standard SQL syntax
INSERT INTO users (name, email, role, score) 
VALUES ('Ahmad Faiz', 'faiz@ict.house', 'Architect', 9900);

-- Insert Row using native JSON document syntax
INSERT INTO users {"name": "Alice Vance", "role": "Engineer", "score": 9200};

-- Select all columns with conditional filtering
SELECT * FROM users 
WHERE score >= 9000 AND role = 'Architect';

-- Select with sorting and pagination
SELECT name, role, score FROM users 
WHERE role != 'Suspended'
ORDER BY score DESC 
LIMIT 20 OFFSET 0;

-- String Matching Operators
SELECT * FROM users WHERE name LIKE '%Faiz%';
SELECT * FROM users WHERE email ENDS_WITH '@ict.house';
SELECT * FROM users WHERE role STARTS_WITH 'Lead';
SELECT * FROM users WHERE role IN ('Architect', 'Principal', 'Fellow');

-- Update Records
UPDATE users 
SET role = 'Distinguished Architect', score = score + 500 
WHERE email = 'faiz@ict.house';

-- Delete Records
DELETE FROM users 
WHERE score < 5000 AND role = 'Trial';

-- Record Counting
SELECT COUNT(*) FROM users WHERE score >= 8000;
```

### C. Multi-Table Relational Hash Joins

```sql
-- Relational INNER JOIN (In-Memory Vectorized Hash Join)
SELECT orders.id, customers.name, orders.amount 
FROM orders 
INNER JOIN customers ON orders.customer_id = customers.id
WHERE orders.amount > 500.00;

-- Relational LEFT OUTER JOIN
SELECT customers.name, orders.id, orders.status 
FROM customers 
LEFT JOIN orders ON customers.id = orders.customer_id;
```

### D. ACID Transactions & Snapshot Isolation

```sql
-- Begin multi-statement transaction
BEGIN TRANSACTION;

-- Atomic stock decrement with check constraint
UPDATE inventory 
SET qty = qty - 1 
WHERE item_id = 'A14' AND qty > 0;

-- Record customer purchase
INSERT INTO orders (item_id, customer_id, status) 
VALUES ('A14', 'cust_882', 'CONFIRMED');

-- Commit mutations atomically
COMMIT;

-- Or abort on failure:
-- ROLLBACK;
```

### E. Cost-Based Query Optimizer (`EXPLAIN`)

```sql
-- Generate hierarchical execution plan
EXPLAIN SELECT * FROM users WHERE score > 9000;

-- Execute query and measure actual shard execution timings
EXPLAIN ANALYZE SELECT orders.id, customers.name 
FROM orders 
INNER JOIN customers ON orders.customer_id = customers.id;

-- Output verbose cost scoring, selectivity percentages, and index rationales
EXPLAIN VERBOSE SELECT * FROM users WHERE email = 'faiz@ict.house';
```

---

## 3. openCypher Knowledge Graph Syntax

FaizDB provides a high-performance in-memory and disk-backed **Directional Graph Engine** with native openCypher support.

### A. Graph Edge Creation

```cypher
-- Create nodes and directional relationships with edge weights and attributes
CREATE (a:Person {id: 'p1', name: 'Alice'})-[:KNOWS {weight: 1.0}]->(b:Person {id: 'p2', name: 'Bob'});
CREATE (b:Person {id: 'p2'})-[:WORKS_AT {since: 2024}]->(c:Company {id: 'c1', name: 'ICT House'});
```

### B. Graph Pattern Matching & Multi-Hop Traversal

```cypher
-- 1-Hop Neighbor Match
MATCH (a:Person)-[:KNOWS]->(b:Person) 
WHERE a.id = 'p1' 
RETURN b;

-- Variable-Length Path Traversal (1 to 3 hops)
MATCH (a:Person)-[:KNOWS*1..3]->(b:Person) 
WHERE a.id = 'p1' 
RETURN b;

-- Filter by target node properties
MATCH (a:Person)-[:WORKS_AT]->(c:Company) 
WHERE c.name = 'ICT House' 
RETURN a;
```

### C. Tri-Hybrid GraphRAG (Graph Traversal + HNSW Vector Similarity)

```cypher
-- Match 2 hops out, then rank destination nodes by HNSW vector cosine similarity
MATCH (a:Entity)-[:relates_to*1..2]->(b:Entity) 
WHERE a.id = 'doc_root' 
VECTOR NEAR [0.12, 0.45, 0.88, 0.05] TOP 5 
RETURN b;
```

### D. Graph Edge Deletion

```cypher
-- Delete specific directional relationship
DELETE EDGE FROM 'p1' TO 'p2' VIA 'KNOWS';
```

---

## 4. FaizQL Native Multi-Model Syntax

FaizQL is FaizDB's proprietary unified query syntax that allows executing relational queries, HNSW vector search, and graph traversal within a single statement.

### A. Search & Traversal Clauses

```sql
-- 1. Standard Document Finding
FIND users 
WHERE score > 8000 
SORT BY score DESC 
LIMIT 10 SKIP 0;

-- 2. Dense Vector ANN Similarity Search (< 1ms)
FIND articles 
VECTOR NEAR [0.95, 0.88, 0.12, 0.04] TOP 5;

-- 3. Vector Search using explicit index target
FIND embeddings 
VECTOR NEAR [0.11, 0.42, 0.77, 0.99] TOP 10 
USING INDEX idx_article_embeddings;

-- 4. Bounded Breadth-First Graph Traversal
FIND accounts 
TRAVERSE FROM "acc_suspicious_9" DEPTH 4 VIA "transferred_to";

-- 5. Hybrid Graph Traversal + Dense Vector Ranking (GraphRAG)
FIND agent_memories 
TRAVERSE FROM "agent_alpha" DEPTH 2 VIA "interacted_with" 
VECTOR [0.045, 0.812, 0.334] TOP 5;

-- 6. Combine WHERE Filter with Graph Traversal
FIND documents 
WHERE verified = true 
TRAVERSE FROM "root_doc" DEPTH 3 VIA "references";
```

---

## 5. MongoDB Wire & Shell Syntax

FaizDB speaks native MongoDB wire protocol on **Port 27017**. All standard MongoDB commands, Compass queries, PyMongo, and Mongoose operations work out of the box.

### A. Inserting Documents

```javascript
// Insert single document with automatic 3600-second TTL expiration
db.users.insertOne({
    name: "Cyber Specialist",
    role: "Security",
    score: 9200,
    _ttl: 3600
});

// Bulk insert multiple documents
db.users.insertMany([
    { name: "Alice", role: "DevOps", active: true },
    { name: "Bob", role: "Frontend", active: false }
]);
```

### B. Querying Documents

```javascript
// Basic find with equality filter
db.users.find({ role: "Security" });

// Advanced comparison operators ($gt, $gte, $lt, $lte, $ne, $in)
db.users.find({ score: { $gte: 9000, $lt: 10000 } });

// Projection, Sorting, and Pagination
db.users.find({ role: "Security" }, { name: 1, email: 1, _id: 0 })
    .sort({ score: -1 })
    .limit(10)
    .skip(0);

// Find single document
db.users.findOne({ name: "Cyber Specialist" });

// Document count
db.users.count({ role: "Security" });
```

### C. Updating Documents

```javascript
// Atomic update with $set and $inc
db.users.updateOne(
    { name: "Cyber Specialist" },
    { $set: { status: "Active" }, $inc: { score: 100 } }
);

// Bulk update multiple matching documents
db.users.updateMany(
    { status: "Pending" },
    { $set: { status: "Verified" } }
);
```

### D. Deleting Documents

```javascript
// Delete single matching document
db.users.deleteOne({ name: "Cyber Specialist" });

// Delete multiple matching documents
db.users.deleteMany({ status: "Inactive" });
```

### E. Index & Collection Management

```javascript
// Create secondary index
db.users.createIndex({ score: 1 }, { unique: false });

// Create unique constraint index
db.users.createIndex({ email: 1 }, { unique: true });

// Drop index
db.users.dropIndex("score_1");

// Fetch collection telemetry statistics
db.users.stats();

// Drop entire collection
db.users.drop();
```

### F. MongoDB Graph Traversal ($traverse)

```javascript
// Execute graph traversal via MongoDB dialect
db.users.find({
    $traverse: {
        from: "usr_10",
        depth: 2,
        via: "friend_of"
    }
});
```

---

## 6. HTTP REST API Endpoints Reference

Base URL: `http://localhost:27018`

### A. Universal Query Execution

```http
POST /v1/query
Content-Type: application/json
Authorization: Bearer <JWT_TOKEN>

{
  "query": "SELECT * FROM users WHERE score > 8000 LIMIT 5;"
}
```

*Response:*
```json
{
  "success": true,
  "data": [
    { "id": "u1", "name": "Ahmad Faiz", "role": "Architect", "score": 9900 }
  ],
  "execution_time_us": 412
}
```

### B. Authentication & RBAC

```http
POST /v1/auth/login
Content-Type: application/json

{
  "username": "admin",
  "password": "faizdb-admin-2026"
}
```

```http
GET /v1/auth/whoami
Authorization: Bearer <JWT_TOKEN>
```

### C. Health & Metrics Probes

| Route | Method | Description |
| :--- | :---: | :--- |
| `/v1/health` | `GET` | Comprehensive system health and subsystem status |
| `/v1/health/liveness` | `GET` | Kubernetes liveness probe (returns 200 OK) |
| `/v1/health/readiness` | `GET` | Kubernetes readiness probe (verifies WAL & cluster quorum) |
| `/v1/metrics` | `GET` | Prometheus-compatible telemetry metrics |

### D. Collections & Document Endpoints

```http
# List all collections
GET /v1/collections

# Create collection
POST /v1/collections
{ "name": "customers" }

# Drop collection
DELETE /v1/collections/customers

# Insert Document
POST /v1/collections/customers/insert
{ "name": "Global Logistics Corp", "tier": "Enterprise" }

# Bulk Document Ingestion (High-Throughput)
POST /v1/collections/customers/import
{
  "documents": [
    { "name": "Client A", "tier": "Pro" },
    { "name": "Client B", "tier": "Enterprise" }
  ]
}

# Fetch Document by ID
GET /v1/collections/customers/documents/cust_101

# Delete Document by ID
DELETE /v1/collections/customers/documents/cust_101

# Okapi BM25 Full-Text Search with Fuzzy Typo-Tolerance
POST /v1/collections/customers/search
{
  "query": "logistics",
  "fuzzy": true,
  "top_k": 10
}

# Create Secondary B-Tree Index
POST /v1/collections/customers/indexes
{
  "field": "tier",
  "unique": false
}

# Collection Statistics
GET /v1/collections/customers/stats
```

### E. Change Data Capture (WebSocket Streaming)

```http
WS /v1/stream
```

Connect a standard WebSocket client to receive sub-millisecond mutation events:
```json
{
  "operation": "INSERT",
  "collection": "orders",
  "document_id": "ord_99",
  "timestamp": 1788339200000,
  "data": { "amount": 450.0, "status": "CONFIRMED" }
}
```

### F. Backup & Disaster Recovery

```http
# Create non-blocking snapshot
POST /v1/backup
{
  "output_path": "./backups/faizdb_snapshot.json",
  "encrypt": false
}

# Restore from snapshot
POST /v1/restore
{
  "input_path": "./backups/faizdb_snapshot.json"
}
```

### G. Multi-Region Geo-Replication Mesh

```http
GET /v1/cluster/regions
POST /v1/cluster/regions
{
  "region_id": "eu-central-1",
  "endpoint": "http://eu.faizdb.io:27018"
}

POST /v1/cluster/geo-sync
{
  "deltas": [ ... ]
}
```

---

## 7. gRPC Protocol Buffers RPC Reference

Port: `50051`. Protobuf definition: `proto/faizdb.proto`.

```protobuf
service FaizDbService {
  // 1. Execute SQL, MongoDB JSON, or FaizQL query
  rpc ExecuteQuery (QueryRequest) returns (QueryResponse);

  // 2. High-speed HNSW dense vector ANN search (< 1ms)
  rpc VectorSearch (VectorSearchRequest) returns (VectorSearchResponse);

  // 3. High-throughput bulk document ingestion
  rpc InsertDocuments (InsertRequest) returns (InsertResponse);

  // 4. Server-streaming Change Data Capture (CDC) feed
  rpc SubscribeChangeStream (StreamRequest) returns (stream ChangeEventMsg);

  // 5. Cluster health and readiness probe
  rpc HealthCheck (HealthRequest) returns (HealthResponse);
}
```

---

## 8. Python SDK & LangGraph Native Saver

Install or use local Python SDK from `bindings/python`:

```python
from faizdb import FaizDB
from faizdb.langgraph import FaizDbSaver
from langgraph.graph import StateGraph, END
from typing import TypedDict

# 1. Initialize Native FaizDB Client
db = FaizDB("http://localhost:27018")

# 2. Document CRUD
users = db.collection("users")
doc_id = users.insert({"name": "Ahmad Faiz", "role": "Architect"})
records = users.find({"role": "Architect"})

# 3. HNSW Vector Similarity Search
vectors = users.vector_search([0.95, 0.88, 0.12, 0.04], top_k=5)

# 4. Okapi BM25 Full-Text Search
matches = users.search("Faiz", fuzzy=True, top_k=10)

# 5. Native LangGraph Checkpointer (Zero PostgreSQL Dependency)
checkpointer = FaizDbSaver(db, collection_prefix="production_agents")

class AgentState(TypedDict):
    query: str
    response: str

def agent_step(state: AgentState):
    facts = db.query("MATCH (a:Agent)-[:knows]->(b) RETURN b LIMIT 5")
    return {"response": f"Resolved: {facts}"}

workflow = StateGraph(AgentState)
workflow.add_node("agent", agent_step)
workflow.set_entry_point("agent")
workflow.add_edge("agent", END)

# Compile LangGraph with native FaizDbSaver
app = workflow.compile(checkpointer=checkpointer)

# Sub-millisecond snapshot persistence & time-travel enabled
config = {"configurable": {"thread_id": "session_001"}}
result = app.invoke({"query": "Analyze system load"}, config)
print(result)
```

---

## 9. Embedded In-Process Rust Engine (`faizdb-core`)

Use FaizDB directly inside your Rust binaries without running any server process.

```rust
use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use faizdb_vector::{HnswConfig, HnswIndex, DistanceMetric, QuantizationType};
use faizdb_core::storage::columnar::ColumnarBatch;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Embedded Storage Engine (LSM-Tree + WAL)
    let config = StorageConfig {
        data_dir: PathBuf::from("./embedded_db_data"),
        memtable_size: 4 * 1024 * 1024, // 4MB RAM footprint
        sync_writes: false,
        enable_wal: true,
    };
    let db = StorageEngine::open(config)?;
    db.put(b"device:sensor_01", b"{\"temp\": 24.5}")?;
    let val = db.get(b"device:sensor_01")?;

    // 2. Embedded HNSW Vector Index with Scalar8 Quantization
    let v_config = HnswConfig::new(1536, DistanceMetric::Cosine)
        .with_quantization(QuantizationType::Scalar8);
    let mut index = HnswIndex::new(v_config);
    index.insert("article_01", embedding_vec)?;
    let nearest = index.search(&query_vec, 5);

    // 3. Embedded SIMD Columnar Analytics
    let batch = ColumnarBatch::from_json_documents(&documents)?;
    let total_volume = batch.sum_f64("trade_volume").unwrap();

    Ok(())
}
```

---

*© 2026 FaizDB Project · Official Engineering Specification · All Rights Reserved.*
