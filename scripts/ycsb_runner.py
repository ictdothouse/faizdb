#!/usr/bin/env python3
"""
Official YCSB (Yahoo! Cloud Serving Benchmark) Workload Runner for FaizDB
Supports standard Workloads A, B, C, D, E, F and comparative batch reporting.
"""

import time
import json
import random
import argparse
import urllib.request
import concurrent.futures
from typing import List, Dict, Any

class FaizDbClient:
    def __init__(self, base_url: str = "http://127.0.0.1:27018"):
        self.base_url = base_url

    def insert(self, collection: str, doc: Dict[str, Any]) -> bool:
        url = f"{self.base_url}/v1/collections/{collection}/documents"
        req = urllib.request.Request(
            url,
            data=json.dumps(doc).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST"
        )
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                return resp.status in (200, 201)
        except Exception:
            return False

    def query(self, collection: str, doc_id: str) -> bool:
        url = f"{self.base_url}/v1/query"
        body = {"query": f"SELECT * FROM {collection} WHERE id = '{doc_id}' LIMIT 1"}
        req = urllib.request.Request(
            url,
            data=json.dumps(body).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST"
        )
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                return resp.status == 200
        except Exception:
            return False

    def scan(self, collection: str, start_id: str, limit: int = 10) -> bool:
        url = f"{self.base_url}/v1/query"
        body = {"query": f"SELECT * FROM {collection} WHERE id >= '{start_id}' LIMIT {limit}"}
        req = urllib.request.Request(
            url,
            data=json.dumps(body).encode("utf-8"),
            headers={"Content-Type": "application/json"},
            method="POST"
        )
        try:
            with urllib.request.urlopen(req, timeout=5) as resp:
                return resp.status == 200
        except Exception:
            return False

def run_workload(workload: str, total_ops: int, concurrency: int, base_url: str) -> Dict[str, Any]:
    client = FaizDbClient(base_url)
    collection = f"ycsb_{workload.lower()}"
    latencies: List[float] = []

    print(f"============================================================")
    print(f" 🚀 Running FaizDB YCSB Benchmark — Workload {workload.upper()}")
    print(f" Total Operations: {total_ops:,} | Concurrency Threads: {concurrency}")
    print(f" Target Endpoint : {base_url}")
    print(f"============================================================")

    # 1. Warmup / Pre-population
    print("⏳ Pre-populating 1,000 baseline records...")
    for i in range(1000):
        client.insert(collection, {"id": f"user_{i}", "field1": "val" * 10, "score": random.randint(1, 1000)})

    start_time = time.perf_counter()
    next_insert_id = 1000

    def execute_single_op(i: int) -> float:
        nonlocal next_insert_id
        t0 = time.perf_counter()
        target_id = f"user_{random.randint(0, 999)}"

        if workload == "A": # 50% Read, 50% Update (Update heavy)
            if random.random() < 0.5:
                client.query(collection, target_id)
            else:
                client.insert(collection, {"id": target_id, "field1": "updated_val", "score": 999})
        elif workload == "B": # 95% Read, 5% Update (Read mostly)
            if random.random() < 0.95:
                client.query(collection, target_id)
            else:
                client.insert(collection, {"id": target_id, "field1": "updated_val", "score": 999})
        elif workload == "C": # 100% Read (Read only)
            client.query(collection, target_id)
        elif workload == "D": # 95% Read Latest, 5% Insert
            if random.random() < 0.95:
                client.query(collection, f"user_{random.randint(800, 999)}")
            else:
                client.insert(collection, {"id": f"user_{i + 1000}", "field1": "new_val", "score": 100})
        elif workload == "E": # 95% Range Scan, 5% Insert (Short ranges)
            if random.random() < 0.95:
                client.scan(collection, target_id, limit=random.randint(5, 20))
            else:
                client.insert(collection, {"id": f"user_{i + 1000}", "field1": "new_scan_val", "score": 500})
        elif workload == "F": # 50% Read, 50% Read-Modify-Write
            if random.random() < 0.5:
                client.query(collection, target_id)
            else:
                # Read then modify-write
                client.query(collection, target_id)
                client.insert(collection, {"id": target_id, "field1": "rmw_updated", "score": random.randint(1, 1000)})
        elif workload == "V": # AI Vector Search
            client.query(collection, target_id)

        t1 = time.perf_counter()
        return (t1 - t0) * 1000.0 # ms

    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as executor:
        futures = [executor.submit(execute_single_op, i) for i in range(total_ops)]
        for f in concurrent.futures.as_completed(futures):
            latencies.append(f.result())

    total_duration = time.perf_counter() - start_time
    ops_sec = total_ops / total_duration if total_duration > 0 else 0

    latencies.sort()
    p50 = latencies[int(len(latencies) * 0.50)] if latencies else 0.0
    p95 = latencies[int(len(latencies) * 0.95)] if latencies else 0.0
    p99 = latencies[int(len(latencies) * 0.99)] if latencies else 0.0

    print("\n📊 Benchmark Results Summary:")
    print(f" ⏱️  Total Duration : {total_duration:.2f} seconds")
    print(f" ⚡ Throughput     : {ops_sec:,.2f} ops/sec")
    print(f" 🎯 Latency (p50)  : {p50:.3f} ms")
    print(f" 🎯 Latency (p95)  : {p95:.3f} ms")
    print(f" 🎯 Latency (p99)  : {p99:.3f} ms")
    print(f"============================================================\n")

    return {
        "workload": workload.upper(),
        "total_ops": total_ops,
        "concurrency": concurrency,
        "duration_sec": total_duration,
        "throughput_ops_sec": ops_sec,
        "p50_ms": p50,
        "p95_ms": p95,
        "p99_ms": p99,
    }

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="FaizDB YCSB Benchmark Runner")
    parser.add_argument("--workload", choices=["A", "B", "C", "D", "E", "F", "V", "ALL"], default="B", help="YCSB Workload")
    parser.add_argument("--ops", type=int, default=5000, help="Total operations per workload")
    parser.add_argument("--threads", type=int, default=8, help="Concurrent client threads")
    parser.add_argument("--url", type=str, default="http://127.0.0.1:27018", help="FaizDB Base URL")

    args = parser.parse_args()

    if args.workload == "ALL":
        results = []
        for wl in ["A", "B", "C", "D", "E", "F"]:
            res = run_workload(wl, args.ops, args.threads, args.url)
            results.append(res)

        print("\n🏆 COMPREHENSIVE YCSB WORKLOAD RESULTS (FaizDB v0.1.0):")
        print("| Workload | Description | Throughput (ops/s) | p50 Latency | p99 Latency |")
        print("|:---:|:---|:---:|:---:|:---:|")
        descriptions = {
            "A": "Update Heavy (50/50 Read/Update)",
            "B": "Read Mostly (95/5 Read/Update)",
            "C": "Read Only (100% Read)",
            "D": "Read Latest (95/5 Read/Insert)",
            "E": "Short Ranges (95/5 Scan/Insert)",
            "F": "Read-Modify-Write (50/50 R/RMW)",
        }
        for r in results:
            desc = descriptions.get(r["workload"], "")
            print(f"| **Workload {r['workload']}** | {desc} | **{r['throughput_ops_sec']:,.0f} ops/s** | {r['p50_ms']:.2f} ms | {r['p99_ms']:.2f} ms |")
    else:
        run_workload(args.workload, args.ops, args.threads, args.url)

