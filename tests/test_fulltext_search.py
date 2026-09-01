import json
import urllib.request
from pymongo import MongoClient

def test_fulltext_search():
    base_url = "http://127.0.0.1:27018"
    print("1. Connecting via pymongo to FaizDB (mongodb://127.0.0.1:27017)...")
    client = MongoClient("mongodb://127.0.0.1:27017", serverSelectionTimeoutMS=2000)
    db = client["faizdb"]
    col = db["tech_articles"]

    print("2. Inserting rich technical articles into FaizDB...")
    articles = [
        {
            "title": "FaizDB: The High-Performance Distributed Rust Database",
            "category": "Databases",
            "content": "FaizDB is a blazing fast NoSQL engine with native HNSW vector search, Raft clustering, and BM25 full-text indexing.",
            "author": "Ahmad Faiz"
        },
        {
            "title": "Understanding Raft Distributed Consensus",
            "category": "Clustering",
            "content": "Leader election and log replication ensure fault tolerance across multiple server nodes in a cluster.",
            "author": "Systems Engineer"
        },
        {
            "title": "Rust Systems Programming Guide",
            "category": "Programming",
            "content": "Memory safety without garbage collection makes Rust ideal for building low-latency database engines.",
            "author": "Ferris"
        }
    ]
    col.insert_many(articles)

    print("\n3. Testing REST API BM25 Full-Text Search (POST /v1/collections/tech_articles/search)...")
    # Search: "database rust"
    req_body = json.dumps({"query": "database rust", "fuzzy": False, "top_k": 5}).encode()
    req = urllib.request.Request(
        f"{base_url}/v1/collections/tech_articles/search",
        data=req_body,
        headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req) as resp:
        res = json.loads(resp.read().decode())
        print(f"🔥 Exact BM25 Search Results:\n{json.dumps(res, indent=2)}")
        assert res["success"] is True
        assert len(res["data"]) >= 2
        titles = [d.get("title", "") for d in res["data"]]
        print(f"   • Matched Articles: {titles}")
        assert any("FaizDB" in t for t in titles)
        assert any("Rust" in t for t in titles)

    print("\n4. Testing Fuzzy Typo Matching ('databse' with typo)...")
    req_fuzzy = json.dumps({"query": "databse", "fuzzy": True, "top_k": 5}).encode()
    req = urllib.request.Request(
        f"{base_url}/v1/collections/tech_articles/search",
        data=req_fuzzy,
        headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req) as resp:
        res = json.loads(resp.read().decode())
        print(f"🔥 Fuzzy Typo Match Results:\n{json.dumps(res, indent=2)}")
        assert res["success"] is True
        assert len(res["data"]) > 0
        print(f"   • Matched '{res['data'][0].get('title')}' despite typo!")

    print("\n🎉 Full-Text Search Engine (Okapi BM25 + Fuzzy Match) PASSED with 100% SUCCESS!")

if __name__ == "__main__":
    test_fulltext_search()
