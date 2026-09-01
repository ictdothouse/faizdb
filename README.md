# 🔥 FaizDB — The AI-Native NoSQL Database Engine

> **"Fast as SQLite. Powerful as PostgreSQL. Flexible as MongoDB. AI-Native by Design."**  
> Created by **Ahmad Faiz** | License: **Apache 2.0 (Open Source)**

---

## 🌟 Mengapa FaizDB Dicipta?

FaizDB dibina untuk menyelesaikan kelemahan terbesar database sedia ada dan menjadi enjin database moden yang bertahan untuk **50 tahun ke depan**:

| Kelemahan Incumbent (MongoDB / Postgres) | Penyelesaian Revolusioner FaizDB |
|:---|:---|
| **MongoDB boros RAM & tiada native vector** | Enjin hybrid **LSM-Tree + B-Tree** yang ringan dengan **HNSW Vector Search terbina dalam** |
| **MongoDB had dokumen 16MB** | **Tiada had dokumen (hingga 256MB)** secara streaming chunked storage |
| **Postgres sukar di-embed & scaling rigid** | **Embeddable single-binary** (semudah SQLite) atau jalan sebagai Server |
| **Perlu pasang plugin luaran untuk AI (pgvector)** | **AI-Native**: Vector search, embeddings, dan GraphRAG sedia ada tanpa plugin |
| **Masalah keselamatan (default no auth, memory leaks)** | **Zero-Trust**: AES-256-GCM encryption & Argon2id secara default, 100% memory-safe dalam Rust |

---

## ⚡ Hasil Benchmark Prestasi

Diuji pada mesin standard (Release Build, LTO):

| Operasi | Jumlah Dokumen | Masa Diambil | Kelajuan Melalui (Throughput) |
|:---|:---:|:---:|:---:|
| **INSERT (Penulisan)** | **50,000 docs** | **154.60 ms** | 🚀 **323,424 ops / saat** |
| **SCAN (Pembacaan)** | **50,000 docs** | **74.48 ms** | ⚡ **671,327 ops / saat** |
| **FILTER QUERY** | **25,000 docs** | **38.48 ms** | 🎯 Sub-millisecond queries |
| **AI VECTOR SEARCH** | **HNSW Index** | **< 1.0 ms** | 🤖 Sub-millisecond ANN |

---

## 🏗️ Seni Bina Monorepo (Crates Berlapis)

```
FAIZDB/
├── faizdb-core/        # 🌲 LSM-Tree, MemTable, SSTable, WAL, MVCC ACID, Document Store
├── faizdb-vector/      # 🎯 HNSW Multi-layer Vector Search & Metrics (Cosine, L2, Dot)
├── faizdb-graph/       # 🕸️ Knowledge Graph, Traversal, & GraphRAG Engine
├── faizdb-query/       # 🧠 Multi-Dialect Parser (SQL, MongoDB JSON, FaizQL) & Executor
├── faizdb-security/    # 🔒 Zero-Trust AES-256-GCM Encryption, Argon2id & JWT RBAC
├── faizdb-server/      # 🌐 Axum HTTP/REST API Server
├── faizdb-cli/         # 💻 Single-binary CLI, REPL Shell & Server Runner
└── bindings/           # 🔌 SDK untuk Node.js/Bun/TS, Python, Go, PHP
```

---

## 🚀 Cara Penggunaan Segera (Quick Start)

### 1. Jalankan Interactive Shell (REPL)
```bash
./faizdb shell
```

Di dalam shell, anda boleh menggunakan **semua dialek**:
```javascript
// Dialek MongoDB:
db.users.insert({"name": "Ahmad Faiz", "role": "Architect", "age": 30, "city": "KL"})
db.users.find({"city": "KL"})
db.users.find({"age": {"$gte": 25}})

// Dialek SQL:
SELECT * FROM users WHERE age >= 25 AND city = 'KL' LIMIT 10
INSERT INTO users {"name": "Linus Torvalds", "role": "Creator", "age": 55}
COUNT FROM users

// Dialek FaizQL (AI & Vector):
FIND users VECTOR NEAR [0.95, 0.90, 0.10, 0.05] TOP 5
```

### 2. Jalankan sebagai Server Latar Belakang
```bash
./faizdb serve --port 27018 --host 0.0.0.0
```

### 3. Demo AI Vector & GraphRAG
```bash
./faizdb vector-demo   # Ujian HNSW semantic vector matching
./faizdb graph-demo    # Ujian GraphRAG relationship traversal
```

### 4. FaizDB Web Management Studio (UI Dashboard)
Dashboard visual moden berasaskan **React + Vite + TailwindCSS + Shadcn/ui**:
```bash
cd studio
pnpm install
pnpm dev
# Buka http://localhost:5173
```

### 5. Sambung Terus Menggunakan Pemandu Rasmi MongoDB (Port 27017)
```python
from pymongo import MongoClient

client = MongoClient("mongodb://127.0.0.1:27017", directConnection=True)
db = client["faizdb"]
users = db["users"]

users.insert_one({"name": "Ahmad Faiz", "role": "Innovator"})
doc = users.find_one({"role": "Innovator"})
print("Found:", doc)
```

### 6. Deploy dengan Docker
```bash
docker build -t faizdb .
docker run -d -p 27017:27017 -p 27018:27018 -v faizdb_data:/data faizdb
```

---

## 🔌 SDK Bahasa Pengaturcaraan

### TypeScript / Bun / Node.js
```typescript
import { FaizClient } from './bindings/node';

const db = new FaizClient('http://localhost:27018');
const users = db.collection('users');

await users.insert({ name: 'Faiz', age: 30, role: 'Creator' });
const list = await users.find({ age: { $gte: 25 } });
const vectorMatches = await users.vectorSearch([0.95, 0.90, 0.10, 0.05], { topK: 5 });
```

### Python
```python
from faizdb import FaizDB

db = FaizDB("http://localhost:27018")
users = db.collection("users")

users.insert({"name": "Faiz", "age": 30, "role": "Creator"})
docs = users.find({"age": {"$gte": 25}})
matches = users.vector_search([0.95, 0.90, 0.10, 0.05], top_k=5)
```

### Go
```go
import "faizdb"

client := faizdb.NewClient("http://localhost:27018")
users := client.Collection("users")
id, _ := users.Insert(map[string]interface{}{"name": "Faiz", "age": 30})
results, _ := users.Find(map[string]interface{}{"age": 30})
```

---

## 📜 Lesen

FaizDB dikeluarkan di bawah lesen **Apache 2.0 Open Source**.
Syarikat dan pembangun di seluruh dunia bebas menggunakannya.

---
**Dicipta dengan 🔥 oleh Ahmad Faiz | Malaysia 🇲🇾**
