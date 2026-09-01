# 📦 FaizDB Official Client SDKs Guide

FaizDB provides zero-dependency, type-safe official SDKs for **Node.js/TypeScript**, **Python**, and **Go**.

---

## 🟢 1. Node.js & TypeScript SDK

### Installation
```bash
# In your project root
npm install ./bindings/node  # or pnpm add ./bindings/node
```

### Usage
```typescript
import { FaizDB } from 'faizdb';

async function main() {
  const db = new FaizDB('http://localhost:27018');

  // Authenticate
  await db.login('admin', 'faizdb-admin-2026');

  // 1. Collections & Document CRUD
  const users = db.collection('users');
  const userId = await users.insert({ name: 'Ahmad Faiz', role: 'Architect', email: 'faiz@ict.house' });
  console.log('Inserted Document ID:', userId);

  // 2. Secondary Index & Constraints
  await users.createIndex('email', { unique: true });

  // 3. AI Vector Similarity Search (< 1ms)
  const matches = await users.vectorSearch([0.95, 0.90, 0.10, 0.05], 5);

  // 4. Okapi BM25 Fuzzy Text Search
  const results = await users.search('Faiz', { fuzzy: true, topK: 10 });

  // 5. Cost-Based EXPLAIN Query Plan
  const plan = await db.explain('SELECT * FROM users WHERE email = "faiz@ict.house"');
  console.log('Plan:', plan.plan_type, 'Latency:', plan.execution_time_us, 'µs');
}

main();
```

---

## 🐍 2. Python SDK (`pip install .`)

### Installation
```bash
cd bindings/python
pip install -e .
```

### Usage
```python
from faizdb import FaizDB

db = FaizDB("http://localhost:27018")
db.login("admin", "faizdb-admin-2026")

users = db.collection("users")

# Document Insert
user_id = users.insert({"name": "Ahmad Faiz", "role": "Architect", "email": "faiz@ict.house"})

# AI Vector Search
matches = users.vector_search([0.95, 0.90, 0.10, 0.05], top_k=5)

# Okapi BM25 Fuzzy Text Search
results = users.search("Faiz", fuzzy=True, top_k=10)

# EXPLAIN Query Planner
plan = db.explain("SELECT * FROM users WHERE email = 'faiz@ict.house'")
print(f"Plan: {plan['plan_type']} in {plan['execution_time_us']} µs")
```

---

## 🔵 3. Go SDK

### Usage
```go
package main

import (
	"fmt"
	"github.com/ictdothouse/faizdb/bindings/go"
)

func main() {
	client := faizdb.NewClient("http://localhost:27018")
	token, err := client.Login("admin", "faizdb-admin-2026")
	if err != nil {
		panic(err)
	}
	fmt.Println("Authenticated with Token:", token)

	users := client.Collection("users")
	docId, err := users.Insert(map[string]interface{}{
		"name":  "Ahmad Faiz",
		"role":  "Architect",
		"email": "faiz@ict.house",
	})
	if err != nil {
		panic(err)
	}
	fmt.Println("Inserted document:", docId)
}
```
