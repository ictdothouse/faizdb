import json
import time
import urllib.request
from pymongo import MongoClient

def test_ttl_cache():
    base_url = "http://127.0.0.1:27018"
    print("1. Connecting via pymongo to FaizDB (mongodb://127.0.0.1:27017)...")
    client = MongoClient("mongodb://127.0.0.1:27017", serverSelectionTimeoutMS=2000)
    db = client["faizdb"]
    col = db["session_tokens"]

    print("2. Inserting 2-second expiring OTP token (Redis-like TTL)...")
    doc = {
        "_id": "otp_test_8831",
        "user": "faiz",
        "code": "749201",
        "_ttl": 2  # 2 seconds lifetime
    }
    col.insert_one(doc)

    print("3. Checking document immediately at t = 0.5s...")
    time.sleep(0.5)
    found = col.find_one({"_id": "otp_test_8831"})
    print(f"✅ Immediate Read Result: {found}")
    assert found is not None
    assert found["code"] == "749201"

    print("4. Waiting for 2.2 seconds for TTL expiration (t = 2.7s)...")
    time.sleep(2.2)

    print("5. Querying document after TTL expired...")
    expired_doc = col.find_one({"_id": "otp_test_8831"})
    print(f"🔥 Post-Expiry Read Result: {expired_doc}")
    assert expired_doc is None

    print("\n6. Checking TTL Cache Stats API (GET /v1/collections/session_tokens/ttl/stats)...")
    req = urllib.request.Request(f"{base_url}/v1/collections/session_tokens/ttl/stats")
    with urllib.request.urlopen(req) as resp:
        stats = json.loads(resp.read().decode())
        print(f"🔥 TTL Stats: {json.dumps(stats, indent=2)}")
        assert stats["success"] is True

    print("\n🎉 Time-To-Live (TTL) Auto-Expiry Cache Engine PASSED with 100% SUCCESS!")

if __name__ == "__main__":
    test_ttl_cache()
