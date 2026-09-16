# 🧩 FaizDB Official Wire Protocol Compatibility Matrix

> **Last Updated:** September 2026  
> **Release Target:** FaizDB v0.1.0 (Developer Preview)  
> **Engine Architecture:** Clean-Slate Safe Rust Kernel with Native Multi-Wire Decoders  

---

## 🎯 Architectural Philosophy: Wire-Protocol Compatibility vs. Legacy Emulation

FaizDB does **not** emulate the legacy internal architecture of PostgreSQL, MongoDB, or MySQL (e.g., PostgreSQL table-level OIDs, fork-based connection models, or MySQL MyISAM table locks). Instead, FaizDB implements **Native Wire Ingress Gateways** backed by a high-throughput, unified LSM-Tree storage engine with Multi-Version Concurrency Control (MVCC Serializable Snapshot Isolation), HNSW vector indexing, and graph traversal.

The goal of FaizDB's wire gateways is to **eliminate database sprawl and client rewrite friction** — allowing existing applications, ORMs, and administrative GUIs to connect directly to FaizDB using standard client libraries without code modifications.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                             Client Ecosystem                                │
│       (psql, mongosh, mysql CLI, DBeaver, Prisma, Drizzle, SQLAlchemy)      │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ Standard Wire TCP Packets
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                    FaizDB Multi-Wire Ingress Gateways                       │
│     Port 5432 (PostgreSQL)  │  Port 27017 (MongoDB)  │  Port 3306 (MySQL)   │
├─────────────────────────────────────────────────────────────────────────────┤
│ • Handshake & Authentication (Argon2id, SASL PLAIN, Cleartext, Ed25519 JWT) │
│ • Extended Query Protocol (Parse, Bind, Describe, Execute, Sync)            │
│ • Virtual Catalog Reflection (`pg_catalog.*`, `information_schema.*`)       │
└──────────────────────────────────────┬──────────────────────────────────────┘
                                       │ AST (FaizQL)
                                       ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                Unified Core Storage & Vector Engine (Safe Rust)              │
│       MemTable SkipList + LSM-Tree SSTable + WAL + HNSW + Graph + MVCC      │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 🐘 1. PostgreSQL Wire Protocol (Port 5432 / 5433)

FaizDB provides wire-level compatibility for PostgreSQL clients communicating via the **PostgreSQL Frontend/Backend Protocol v3.0**.

### Compatibility Status:
* **Core DML / CRUD:** ✅ 95%
* **Extended Query Protocol:** ✅ 92%
* **Transaction Control:** ✅ 90% (MVCC Snapshot Isolation + SSI)
* **ORM Introspection:** ✅ 88% (Prisma, Drizzle, DBeaver, SQLAlchemy)
* **Advanced Procedural / Extensions:** 🟡 25% (Roadmap)

### Feature Breakdown:

| Feature / Capability | Status | Detailed Description |
|:---|:---:|:---|
| **Simple Query Protocol (`Q`)** | ✅ **Supported** | Direct execution of single/multi-statement SQL strings. |
| **Extended Query Protocol (`P/B/D/E/S/C`)** | ✅ **Supported** | Full Parse (`P`), Parameter Bind (`B`), Describe (`D`), Portal Execute (`E`), Sync (`S`), and Close (`C`) state machine. |
| **Binary Parameter Decoding** | ✅ **Supported** | Binary format (Code 1) parameter decoding for `int2`, `int4`, `int8`, `float4`, `float8`, `bool`, `varchar`. |
| **Transaction States (`I`, `T`, `E`)** | ✅ **Supported** | Connection-level MVCC transaction tracking. Commands rejected with `SQLSTATE 25P02` in aborted transactions until `ROLLBACK`. |
| **Lexical Expression Precedence** | ✅ **Supported** | Pratt recursive-descent parser handling nested `(A OR B) AND (C OR (D AND E))` with rigorous precedence. |
| **SQL Predicates** | ✅ **Supported** | `BETWEEN ... AND ...`, `NOT BETWEEN`, `IN (...)`, `NOT IN`, `LIKE '...'`, `NOT LIKE`, `IS NULL`, `IS NOT NULL`. |
| **Multi-Column Sorting** | ✅ **Supported** | `ORDER BY col1 ASC, col2 DESC, col3 ASC` with lexicographical tie-breaking. |
| **Pagination** | ✅ **Supported** | `LIMIT <n> OFFSET <m>` with pushdown iterators. |
| **Relational Joins** | ✅ **Supported** | Hash joins (`INNER JOIN`, `LEFT JOIN`) on indexed equality predicates. |
| **Virtual System Catalogs** | ✅ **Supported** | `pg_class`, `pg_attribute`, `pg_type`, `pg_am`, `pg_constraint`, `pg_description`, `information_schema.tables`, `information_schema.columns`. |
| **Postgres Built-in Functions** | ✅ **Supported** | `version()`, `current_schema()`, `current_database()`, `current_user`, `current_setting('server_version_num')`, `format_type()`. |
| **Window Functions (`OVER()`)** | 🟡 **Planned (v0.3)** | `ROW_NUMBER()`, `RANK()`, `PARTITION BY` currently in active design. |
| **Common Table Expressions (WITH / CTE)**| 🟡 **Planned (v0.2)** | Non-recursive CTEs planned for next minor release; recursive CTEs out of scope for v0.x. |
| **Procedural Languages (PL/pgSQL)** | ❌ **Non-Goal** | Procedural stored languages run inside FaizDB are not planned; use native microservices or Rust embedded modules. |
| **Postgres Logical Replication Stream** | 🟡 **Alternative** | FaizDB uses native WebSocket/gRPC CDC Streams instead of Postgres WAL logical decoding. |

---

## 🍃 2. MongoDB Wire Protocol (Port 27017)

FaizDB natively parses incoming BSON payloads over TCP Port 27017 using **OP_MSG (OpCode 2013)** and **OP_QUERY (OpCode 2004)**.

### Compatibility Status:
* **CRUD Operations:** ✅ 92%
* **Query Selectors:** ✅ 85%
* **Index Management:** ✅ 90%
* **Administrative Commands:** ✅ 85%
* **Aggregation Pipeline:** 🟡 45% (Core stages supported; multi-stage subqueries in roadmap)

### Feature Breakdown:

| Feature / Command | Status | Detailed Description |
|:---|:---:|:---|
| **Handshake & Hello** | ✅ **Supported** | `isMaster`, `hello`, `ping`, client driver metadata negotiation. |
| **Authentication** | ✅ **Supported** | SASL PLAIN and Argon2id challenge-response authentication. |
| **CRUD Commands** | ✅ **Supported** | `find`, `insert`, `update`, `delete`, `count`, `distinct`, `findAndModify`. |
| **Query Selectors** | ✅ **Supported** | `$eq`, `$ne`, `$gt`, `$gte`, `$lt`, `$lte`, `$in`, `$nin`, `$regex`, `$exists`, `$and`, `$or`. |
| **Update Operators** | ✅ **Supported** | `$set`, `$unset`, `$inc`, `$push`, `$pull`. |
| **Cursor Pagination** | ✅ **Supported** | Stateful server cursors with `getMore` and `killCursors`. |
| **Index Introspection** | ✅ **Supported** | `createIndexes`, `listIndexes`, `dropIndexes`. |
| **Database & Collection Admin** | ✅ **Supported** | `collStats`, `dbStats`, `serverStatus`, `renameCollection`, `connectionStatus`, `dropDatabase`, `drop`. |
| **Aggregation Pipeline** | 🟡 **Partial** | `$match`, `$project`, `$group`, `$sort`, `$limit`, `$skip`, `$count` fully functional. |
| **Complex Aggregations** | 🟡 **Planned (v0.3)** | Multi-database `$lookup`, `$facet`, `$graphLookup` in active development. |
| **Change Streams (`watch()`)** | ✅ **Alternative** | Native WebSocket real-time change stream available on Port 27018 (`/v1/collections/:name/stream`). |
| **GridFS** | ❌ **Non-Goal** | FaizDB focuses on high-speed structured/vector data; object storage should use S3/MinIO. |

---

## 🐬 3. MySQL / MariaDB Wire Protocol (Port 3306)

FaizDB provides wire compatibility for standard MySQL clients (v5.7 and v8.0 protocol).

### Compatibility Status:
* **Connection Handshake:** ✅ 95% (Protocol v10)
* **Standard DML / Queries:** ✅ 85%
* **Admin & Schema Reflection:** ✅ 80%
* **Prepared Statements Binary:** 🟡 50% (Text protocol primary; binary protocol partial)

### Feature Breakdown:

| Feature / Command | Status | Detailed Description |
|:---|:---:|:---|
| **Handshake Initialization** | ✅ **Supported** | Protocol version 10 handshake, capability flags negotiation, auth-plugin (`mysql_native_password` / cleartext). |
| **`COM_QUERY` (Text Protocol)** | ✅ **Supported** | Standard SQL execution (`SELECT`, `INSERT`, `UPDATE`, `DELETE`). |
| **`COM_PING` & `COM_QUIT`** | ✅ **Supported** | Connection keep-alive and graceful socket teardown. |
| **Metadata & Reflection** | ✅ **Supported** | `SHOW TABLES`, `SHOW DATABASES`, `SHOW COLUMNS FROM`, `SHOW CREATE TABLE`, `SHOW TABLE STATUS`, `SHOW INDEX FROM`, `SHOW VARIABLES [LIKE ...]`, `DESCRIBE / DESC`. |
| **Laravel / PHP PDO Compatibility** | ✅ **Supported** | Standard Eloquent CRUD, table listings, and basic migration assertions. |
| **MySQL Binary Prepared (`COM_STMT_*`)**| 🟡 **Partial** | Parameter binding for basic types; complete binary type coverage scheduled for v0.2. |
| **Stored Procedures & Triggers** | ❌ **Non-Goal** | MySQL triggers and stored procedures not supported; handled via external microservices. |

---

## ⚡ 4. Native High-Performance Protocols (gRPC & REST)

FaizDB's native interfaces offer the highest throughput with zero translation overhead:

| Gateway | Port | Transport | Capabilities |
|:---|:---:|:---:|:---|
| **gRPC** | `50051` | HTTP/2 + ProtoBuf | Zero-copy vector streaming, raw batch insertion, distributed cluster RPC, Ed25519 Bearer token auth. |
| **REST API** | `27018` | HTTP/1.1 + JSON | OpenAPI compliant CRUD, collection management, backup snapshot triggering, Prometheus `/metrics`. |
| **WebSocket** | `27018` | WebSocket Bus | Real-time Change Data Capture (CDC), live query mutations, server broadcast. |

---

## 🧪 5. Verified Client Ecosystem & Tested Tools

The following clients, drivers, and frameworks have been empirically tested and verified against FaizDB's multi-wire gateways:

### Command-Line & GUI Tools:
* ✅ **`psql`** (PostgreSQL CLI v14, v15, v16)
* ✅ **`mongosh`** (MongoDB Shell v2.x)
* ✅ **`mysql`** (MySQL Client CLI v8.0, MariaDB v10.x)
* ✅ **DBeaver Universal Database Tool** (Community Edition v23+)
* ✅ **TablePlus** (via PostgreSQL connection mode)
* ✅ **Postman** (REST API & WebSockets collection)

### ORMs & Application Frameworks:
* ✅ **Prisma ORM** (Introspection & CRUD via Postgres Wire)
* ✅ **Drizzle ORM** (PostgreSQL dialect)
* ✅ **SQLAlchemy** (Python, psycopg2 / psycopg3 driver)
* ✅ **PyMongo** (Python v4.x official driver)
* ✅ **Mongoose / Node.js mongodb driver** (JavaScript/TypeScript)
* ✅ **PHP PDO & Laravel Eloquent** (`DB_CONNECTION=mysql`)
* ✅ **LangGraph / LangChain** (PostgreSQL checkpointer & episodic memory)

---

## ⚠️ 6. Error Handling & Standards Compliance

FaizDB adheres strictly to industry error specifications:
* When a PostgreSQL client executes an unsupported query feature, FaizDB returns standard **`SQLSTATE 0A000` (FEATURE NOT SUPPORTED)** with an actionable error description rather than a silent failure or protocol crash.
* In MongoDB mode, unsupported aggregation pipeline stages emit MongoDB error code `115 (CommandNotSupported)` with links to the GitHub issue tracker.
* Connection failures due to authentication return **`SQLSTATE 28P01` (Invalid Password)** or Mongo code `18 (AuthFailed)`.

---

*For feature requests, edge-case compatibility reports, or driver verification questions, please open an issue on the [FaizDB Issue Tracker](https://github.com/ictdothouse/faizdb/issues).*
