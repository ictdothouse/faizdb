import json
import urllib.request
from pymongo import MongoClient

def test_backup_and_disaster_recovery():
    base_url = "http://127.0.0.1:27018"
    print("1. Connecting via pymongo to FaizDB (mongodb://127.0.0.1:27017)...")
    client = MongoClient("mongodb://127.0.0.1:27017", serverSelectionTimeoutMS=2000)
    db = client["faizdb"]
    col = db["mission_critical_assets"]
    col.delete_many({})

    print("2. Inserting mission-critical dataset...")
    col.insert_many([
        {"asset_id": "AST-901", "name": "Quantum Compute Cluster", "valuation": 5000000},
        {"asset_id": "AST-902", "name": "High-Yield AI Patent Portfolio", "valuation": 25000000},
        {"asset_id": "AST-903", "name": "Distributed Mesh Network Infrastructure", "valuation": 12000000},
    ])

    initial_count = col.count_documents({})
    print(f"✅ Initial Assets Count: {initial_count}")
    assert initial_count == 3

    print("\n3. Triggering Online Consistent Snapshot Creation (POST /v1/backup/create)...")
    req = urllib.request.Request(f"{base_url}/v1/backup/create", data=b"{}", headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(req) as resp:
        res = json.loads(resp.read().decode())
        print(f"🔥 Snapshot Manifest Created:\n{json.dumps(res, indent=2)}")
        assert res["success"] is True
        manifest = res["data"]
        assert manifest["total_documents"] >= 3
        assert len(manifest["checksum"]) >= 8

    print("\n4. Verifying Backups Listing API (GET /v1/backup/list)...")
    with urllib.request.urlopen(f"{base_url}/v1/backup/list") as resp:
        res = json.loads(resp.read().decode())
        print(f"🔥 Available Snapshots: {len(res['data'])} archives")
        assert res["success"] is True
        assert len(res["data"]) > 0

    print("\n5. Simulating Total Data Deletion / Catastrophic Disaster...")
    col.delete_many({})
    post_disaster_count = col.count_documents({})
    print(f"⚠️ Post-Disaster Assets Count: {post_disaster_count} (Data wiped!)")
    assert post_disaster_count == 0

    print("\n6. Triggering Disaster Recovery & Snapshot Restore (POST /v1/backup/restore)...")
    restore_req = urllib.request.Request(f"{base_url}/v1/backup/restore", data=b"{}", headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(restore_req) as resp:
        res = json.loads(resp.read().decode())
        print(f"🔥 Restore Result:\n{json.dumps(res, indent=2)}")
        assert res["success"] is True

    print("\n7. Validating Restored Assets in Database...")
    restored_docs = list(col.find({}))
    print(f"✅ Restored Assets in DB ({len(restored_docs)} items):\n{json.dumps(restored_docs, default=str, indent=2)}")
    assert len(restored_docs) == 3
    asset_names = [d["name"] for d in restored_docs]
    assert "Quantum Compute Cluster" in asset_names
    assert "High-Yield AI Patent Portfolio" in asset_names

    print("\n🎉 Automated Backup, Snapshot & Disaster Recovery PASSED with 100% SUCCESS!")

if __name__ == "__main__":
    test_backup_and_disaster_recovery()
