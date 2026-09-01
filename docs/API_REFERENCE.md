# 📚 FaizDB Enterprise REST API Reference

FaizDB provides a unified high-performance HTTP/REST and WebSocket API on port `27018`.

---

## 🔐 Authentication & RBAC

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

## 🗄️ Query & Document Operations

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

## 📥 Bulk Data Migration & Ingestion

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

## ⚡ Secondary Indexes & Unique Constraints

### 1. `POST /v1/collections/:name/indexes`
Creates a high-speed $O(\log N)$ B-Tree secondary index with optional uniqueness enforcement.
* **Request:**
  ```json
  {
    "field": "email",
    "unique": true
  }
  ```

### 2. `GET /v1/collections/:name/indexes`
Lists all active secondary indexes on the collection.

### 3. `DELETE /v1/collections/:name/indexes/:field`
Drops a secondary index.

---

## 💳 Multi-Document ACID Transactions

* **`POST /v1/transaction/begin`** — Starts an isolated snapshot transaction.
* **`POST /v1/transaction/commit`** — Atomically flushes staged mutations to the Write-Ahead Log (WAL).
* **`POST /v1/transaction/rollback`** — Discards staged write-buffer.

---

## 🔒 Disaster Recovery & Automated Backup Scheduler (SOC2)

### 1. `POST /v1/backup/create`
Creates an atomic consistent point-in-time snapshot with optional AES-256-GCM encryption.
* **Request:**
  ```json
  { "passphrase": "vault-master-key-2026" }
  ```

### 2. `GET /v1/backup/schedule`
Fetches the active automated backup schedule and retention policy.

### 3. `POST /v1/backup/schedule` (Admin only)
Updates the automated backup schedule and retention policy.
* **Request:**
  ```json
  {
    "enabled": true,
    "frequency_minutes": 1440,
    "retention_days": 14,
    "passphrase": "vault-master-key-2026"
  }
  ```

### 4. `POST /v1/backup/restore`
Restores database state from an existing snapshot archive.
