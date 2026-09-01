import json
import urllib.request

base_url = "http://127.0.0.1:27018"

# 1. POST insert
print("Testing POST /v1/collections/gaming_leaderboard/documents...")
req = urllib.request.Request(
    f"{base_url}/v1/collections/gaming_leaderboard/documents",
    data=json.dumps({"name": "Faiz Aziz", "score": 10000, "role": "Cyber Architect"}).encode(),
    headers={"Content-Type": "application/json"}
)
with urllib.request.urlopen(req) as resp:
    res = json.loads(resp.read().decode())
    print(f"POST Response: {res}")
    assert res["success"] is True

# 2. GET documents
print("\nTesting GET /v1/collections/gaming_leaderboard/documents...")
with urllib.request.urlopen(f"{base_url}/v1/collections/gaming_leaderboard/documents") as resp:
    res = json.loads(resp.read().decode())
    print(f"GET Response ({len(res['data'])} docs): {res}")
    assert res["success"] is True
    assert len(res["data"]) >= 1

print("\n🎉 REST Documents API PASSED 100%!")
