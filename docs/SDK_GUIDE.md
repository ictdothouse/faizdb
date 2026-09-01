# 📦 FaizDB Official Polyglot Client SDKs Guide

FaizDB provides zero-dependency, type-safe official SDKs with dual-mode support (**HTTP/REST & WebSockets** + **gRPC & Protocol Buffers**) for **Node.js/TypeScript**, **Python**, and **Go**.

---

## 🟢 1. Node.js & TypeScript SDK

### Installation
```bash
npm install ./bindings/node  # or pnpm add ./bindings/node
```

### High-Performance gRPC Client (Port 50051)
```typescript
import { FaizDbGrpcClient } from '@faizdb/client';

async function main() {
  const client = new FaizDbGrpcClient({ target: 'localhost:50051' });

  // 1. Health Check
  const health = await client.healthCheck();
  console.log('gRPC Status:', health.status);

  // 2. AI Vector Similarity Search (< 1ms)
  const matches = await client.vectorSearch('products', [0.95, 0.90, 0.10, 0.05], 5);
  console.log('Vector Matches:', matches);

  // 3. Bulk Insert
  const res = await client.insertDocuments('products', [
    { name: 'Neural Processor', vector: [0.1, 0.5, 0.9] }
  ]);
  console.log('Inserted Count:', res.insertedCount);
}

main();
```

---

## 🐍 2. Python SDK (`pip install .` / `pyproject.toml`)

### Installation (Requires Python >= 3.11)
```bash
cd bindings/python
pip install -e .
```
Supported with modern PEP 517/518 build standards (`pyproject.toml`).

### gRPC Client Usage (Port 50051)
```python
from faizdb import FaizDbGrpcClient

client = FaizDbGrpcClient(target="localhost:50051")

# 1. Health Check
health = client.health_check()
print("gRPC Status:", health["status"])

# 2. AI Vector ANN Search
hits = client.vector_search("ai_embeddings", vector=[0.95, 0.90, 0.10], top_k=5)
for h in hits:
    print(f"Match ID: {h['id']}, Score: {h['score']:.4f}")

# 3. Query Execution (SQL / Mongo AST)
res = client.execute_query("SELECT * FROM ai_embeddings WHERE score > 0.8")
print("Results:", res)
```

---

## 🔵 3. Go SDK

### Installation & Usage
```go
package main

import (
	"context"
	"fmt"
	"github.com/ictdothouse/faizdb/bindings/go"
)

func main() {
	client := faizdb.NewGrpcClient("localhost:50051")

	// 1. Health check
	health, err := client.HealthCheck(context.Background())
	if err != nil {
		panic(err)
	}
	fmt.Println("gRPC Health:", health["status"])

	// 2. AI Vector Similarity Search
	hits, err := client.VectorSearch(context.Background(), "ai_embeddings", []float32{0.95, 0.90, 0.10}, 5)
	if err != nil {
		panic(err)
	}
	fmt.Printf("Found %d vector matches\n", len(hits))
}
```
