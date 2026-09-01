import json
import urllib.request
from pymongo import MongoClient

def test_aggregation():
    print("1. Connecting via pymongo to FaizDB (mongodb://127.0.0.1:27017)...")
    client = MongoClient("mongodb://127.0.0.1:27017", serverSelectionTimeoutMS=2000)
    db = client["faizdb"]
    col = db["analytics_sales"]

    # Insert sample dataset
    print("2. Inserting sample transactions dataset...")
    col.insert_many([
        {"country": "Malaysia", "category": "Cloud Services", "amount": 12500, "status": "completed"},
        {"country": "Malaysia", "category": "AI Training", "amount": 27500, "status": "completed"},
        {"country": "Singapore", "category": "Cloud Services", "amount": 45000, "status": "completed"},
        {"country": "Singapore", "category": "AI Training", "amount": 55000, "status": "completed"},
        {"country": "Indonesia", "category": "Cloud Services", "amount": 8000, "status": "pending"},
    ])

    print("3. Executing MongoDB Aggregation Pipeline ($match -> $group -> $sort)...")
    pipeline = [
        {"$match": {"status": "completed"}},
        {
            "$group": {
                "_id": "$country",
                "totalRevenue": {"$sum": "$amount"},
                "avgRevenue": {"$avg": "$amount"},
                "dealCount": {"$sum": 1}
            }
        },
        {"$sort": {"totalRevenue": -1}}
    ]

    results = list(col.aggregate(pipeline))
    print(f"🔥 MongoDB Wire Protocol Aggregate Results:\n{json.dumps(results, indent=2)}")

    assert len(results) == 2
    assert results[0]["_id"] == "Singapore"
    assert results[0]["totalRevenue"] == 100000
    assert results[0]["dealCount"] == 2
    assert results[1]["_id"] == "Malaysia"
    assert results[1]["totalRevenue"] == 40000
    assert results[1]["dealCount"] == 2

    print("\n4. Testing REST API Aggregation Endpoint (POST /v1/collections/analytics_sales/aggregate)...")
    req_body = json.dumps({"pipeline": pipeline}).encode()
    req = urllib.request.Request(
        "http://127.0.0.1:27018/v1/collections/analytics_sales/aggregate",
        data=req_body,
        headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req) as resp:
        rest_data = json.loads(resp.read().decode())
        print(f"🔥 REST API Aggregate Results:\n{json.dumps(rest_data, indent=2)}")
        assert rest_data["success"] is True

    print("\n🎉 Complex Aggregation & Analytics Pipeline verification PASSED with 100% SUCCESS!")

if __name__ == "__main__":
    test_aggregation()
