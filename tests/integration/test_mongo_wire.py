"""
Integration test for FaizDB MongoDB Wire Protocol (Port 27017)
Tests TCP connection, 16-byte header framing, OP_MSG handshake, Insert, and Find queries.
"""

import socket
import struct
import json

def encode_cstring(s: str) -> bytes:
    return s.encode('utf-8') + b'\x00'

def make_op_msg(request_id: int, body_dict: dict) -> bytes:
    import bson
    body_bson = bson.encode(body_dict)
    
    # OP_MSG payload: flags (4 bytes) + section 0 (1 byte kind 0 + body_bson)
    payload = struct.pack('<I', 0) + b'\x00' + body_bson
    
    # 16-byte header: length (i32), request_id (i32), response_to (i32), opcode (2013)
    total_len = 16 + len(payload)
    header = struct.pack('<iiii', total_len, request_id, 0, 2013)
    
    return header + payload

def parse_op_msg(data: bytes) -> dict:
    import bson
    total_len, req_id, resp_to, opcode = struct.unpack('<iiii', data[:16])
    flags = struct.unpack('<I', data[16:20])[0]
    kind = data[20]
    body_bson = data[21:total_len]
    return bson.decode(body_bson)

def main():
    print("🍃 Connecting to FaizDB MongoDB Wire Protocol on 127.0.0.1:27017 ...")
    
    # Check if pymongo or bson is installed, if not we test with raw TCP handshake
    try:
        import bson
    except ImportError:
        print("Note: 'bson' package not installed in test runner, testing raw socket handshake...")
        s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        s.connect(('127.0.0.1', 27017))
        print("✅ TCP Socket connected successfully to mongodb://127.0.0.1:27017!")
        s.close()
        return

    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.connect(('127.0.0.1', 27017))
    print("✅ Connected to TCP port 27017")

    # 1. Handshake `isMaster`
    handshake_msg = make_op_msg(1, {"isMaster": 1, "client": {"driver": {"name": "FaizDB-Test"}}})
    s.sendall(handshake_msg)
    resp_header = s.recv(16)
    resp_len = struct.unpack('<i', resp_header[:4])[0]
    resp_body = s.recv(resp_len - 16)
    resp_doc = parse_op_msg(resp_header + resp_body)
    print("✅ Handshake isMaster response:", resp_doc)
    assert resp_doc.get("ismaster") == True or resp_doc.get("ok") == 1.0

    # 2. Insert via Wire Protocol
    insert_msg = make_op_msg(2, {
        "insert": "products",
        "documents": [
            {"title": "FaizDB Enterprise", "price": 999, "category": "database"},
            {"title": "FaizDB Cloud", "price": 29, "category": "cloud"}
        ]
    })
    s.sendall(insert_msg)
    resp_header = s.recv(16)
    resp_len = struct.unpack('<i', resp_header[:4])[0]
    resp_body = s.recv(resp_len - 16)
    resp_doc = parse_op_msg(resp_header + resp_body)
    print("✅ Insert response via Wire Protocol:", resp_doc)
    assert resp_doc.get("n") == 2

    # 3. Find via Wire Protocol
    find_msg = make_op_msg(3, {
        "find": "products",
        "filter": {"category": "database"}
    })
    s.sendall(find_msg)
    resp_header = s.recv(16)
    resp_len = struct.unpack('<i', resp_header[:4])[0]
    resp_body = s.recv(resp_len - 16)
    resp_doc = parse_op_msg(resp_header + resp_body)
    print("✅ Find query response via Wire Protocol:", resp_doc)
    
    first_batch = resp_doc.get("cursor", {}).get("firstBatch", [])
    assert len(first_batch) == 1
    assert first_batch[0]["title"] == "FaizDB Enterprise"
    print("🎉 ALL MONGODB WIRE PROTOCOL INTEGRATION TESTS PASSED!")

    s.close()

if __name__ == "__main__":
    main()
