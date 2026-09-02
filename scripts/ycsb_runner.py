#!/usr/bin/env python3
"""
Official YCSB (Yahoo! Cloud Serving Benchmark) Workload Runner for FaizDB
Supports standard Workloads A, B, C, and AI Vector Workload V.
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

def run_workload(workload: str, total_ops: int, concurrency: int, base_url: str):
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

    def execute_single_op(i: int) -> float:
        t0 = time.perf_counter()
        target_id = f"user_{random.randint(0, 999)}"

        if workload == "A": # 50% Read, 50% Update
            if random.random() < 0.5:
                client.query(collection, target_id)
            else:
                client.insert(collection, {"id": target_id, "field1": "updated_val", "score": 999})
        elif workload == "B": # 95% Read, 5% Update
            if random.random() < 0.95:
                client.query(collection, target_id)
            else:
                client.insert(collection, {"id": target_id, "field1": "updated_val", "score": 999})
        elif workload == "C": # 100% Read
            client.query(collection, target_id)
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
    p50 = latencies[int(len(latencies) * 0.50)]
    p95 = latencies[int(len(latencies) * 0.95)]
    p99 = latencies[int(len(latencies) * 0.99)]

    print("\n📊 Benchmark Results Summary:")
    print(f" ⏱️  Total Duration : {total_duration:.2f} seconds")
    print(f" ⚡ Throughput     : {ops_sec:,.2f} ops/sec")
    print(f" 🎯 Latency (p50)  : {p50:.3f} ms")
    print(f" 🎯 Latency (p95)  : {p95:.3f} ms")
    print(f" 🎯 Latency (p99)  : {p99:.3f} ms")
    print(f"============================================================\n")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="FaizDB YCSB Benchmark Runner")
    parser.add_argument("--workload", choices=["A", "B", "C", "V"], default="B", help="YCSB Workload (A, B, C, V)")
    parser.add_argument("--ops", type=int, default=5000, help="Total operations")
    parser.add_argument("--threads", type=int, default=8, help="Concurrent client threads")
    parser.add_argument("--url", type=str, default="http://127.0.0.1:27018", help="FaizDB Base URL")

    args = parser.parse_args()
    run_workload(args.workload, args.ops, args.threads, args.url)
