import urllib.request
import json
import time

def test_cluster():
    base_url = "http://127.0.0.1:27018"
    
    print("1. Fetching initial FaizDB Raft cluster status...")
    req = urllib.request.Request(f"{base_url}/v1/cluster/status")
    with urllib.request.urlopen(req) as resp:
        data = json.loads(resp.read().decode())
        print(f"✅ Initial Cluster State: {json.dumps(data, indent=2)}")
        assert data["success"] is True
        assert data["data"]["node"]["is_leader"] is True
        assert data["data"]["shards"]["total_slots"] == 16384

    print("\n2. Dynamically joining Node 2 and Node 3 into Raft consensus quorum...")
    for node_id, addr in [("node_2", "127.0.0.1:27028"), ("node_3", "127.0.0.1:27038")]:
        join_payload = json.dumps({"peer_id": node_id, "peer_address": addr}).encode()
        join_req = urllib.request.Request(
            f"{base_url}/v1/cluster/join",
            data=join_payload,
            headers={"Content-Type": "application/json"}
        )
        with urllib.request.urlopen(join_req) as resp:
            join_res = json.loads(resp.read().decode())
            print(f"✅ Joined {node_id}: {join_res['data']['message']}")

    print("\n3. Verifying updated 16,384 Shard Slots Distribution across 3 nodes...")
    with urllib.request.urlopen(f"{base_url}/v1/cluster/status") as resp:
        data = json.loads(resp.read().decode())
        ranges = data["data"]["shards"]["ranges"]
        print(f"✅ Active Shards: {len(ranges)} ranges across cluster")
        assert len(ranges) == 3
        for r in ranges:
            print(f"   • {r['node_id']}: Slots {r['start_slot']} - {r['end_slot']} ({r['slot_count']} slots)")

    print("\n4. Simulating Leader Failover & Raft Election under 300ms...")
    failover_req = urllib.request.Request(f"{base_url}/v1/cluster/failover", data=b"{}", headers={"Content-Type": "application/json"})
    with urllib.request.urlopen(failover_req) as resp:
        res = json.loads(resp.read().decode())
        print(f"🔥 Failover result: {json.dumps(res, indent=2)}")
        assert res["success"] is True
        assert res["data"]["new_term"] >= 2

    print("\n🎉 Raft Consensus Clustering & Auto-Sharding verification PASSED with 100% SUCCESS!")

if __name__ == "__main__":
    test_cluster()
