import asyncio
import json
import websockets
from pymongo import MongoClient

async def main():
    print("Connecting to FaizDB WebSocket Change Stream on ws://127.0.0.1:27018/v1/subscribe...")
    async with websockets.connect("ws://127.0.0.1:27018/v1/subscribe") as ws:
        welcome_msg = await ws.recv()
        print(f"✅ Received Welcome Frame: {welcome_msg}")

        # Now connect with official MongoDB driver and insert a document
        print("Inserting document via pymongo (mongodb://127.0.0.1:27017)...")
        client = MongoClient("mongodb://127.0.0.1:27017", serverSelectionTimeoutMS=2000)
        db = client["faizdb"]
        col = db["realtime_orders"]

        insert_res = col.insert_one({"item": "Quantum Supercomputer", "price": 99999, "currency": "MYR"})
        print(f"✅ Inserted via MongoDB Wire Protocol: {insert_res.inserted_id}")

        # Receive real-time event from WebSocket!
        event_raw = await asyncio.wait_for(ws.recv(), timeout=5.0)
        event = json.loads(event_raw)
        print(f"🔥 Real-Time Event Received via WebSocket: {json.dumps(event, indent=2)}")

        assert event["operation_type"] == "insert"
        assert event["collection"] == "realtime_orders"
        print("🎉 Real-Time Change Stream verification PASSED with 100% SUCCESS!")

if __name__ == "__main__":
    asyncio.run(main())
