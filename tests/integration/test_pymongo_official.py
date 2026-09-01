"""
Test official PyMongo MongoClient connecting directly to FaizDB on mongodb://127.0.0.1:27017
"""

from pymongo import MongoClient

def main():
    print("🚀 Connecting using official PyMongo MongoClient('mongodb://127.0.0.1:27017') ...")
    client = MongoClient("mongodb://127.0.0.1:27017", serverSelectionTimeoutMS=2000, directConnection=True)

    db = client["faizdb_test"]
    users = db["users"]

    # 1. Insert documents
    print("📝 Inserting users via PyMongo...")
    res = users.insert_one({"name": "Ahmad Faiz", "role": "Innovator", "country": "Malaysia"})
    print(f"✅ Inserted with ID: {res.inserted_id}")

    # 2. Query documents
    print("🔍 Querying users via PyMongo...")
    doc = users.find_one({"country": "Malaysia"})
    print(f"✅ Found document: {doc}")
    assert doc is not None
    assert doc["name"] == "Ahmad Faiz"

    print("🎉 PyMongo official client successfully connected and queried FaizDB over wire protocol!")
    client.close()

if __name__ == "__main__":
    main()
