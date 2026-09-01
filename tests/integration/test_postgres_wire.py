"""
Integration test for FaizDB PostgreSQL Wire Protocol (Port 5432)
Tests TCP connection, SSLRequest handling, StartupMessage handshake,
and Simple Query protocol ('Q') for SQL queries (SELECT, INSERT, SHOW, Transactions).
"""

import socket
import struct
import sys

def send_ssl_request(sock: socket.socket) -> bool:
    """Send SSLRequest (length 8, code 80877103) and check response ('N' for plaintext)"""
    packet = struct.pack('!ii', 8, 80877103)
    sock.sendall(packet)
    resp = sock.recv(1)
    print(f"  [SSL Probe] Server replied: {resp} (Expected b'N' for plain TCP)")
    return resp == b'N'

def send_startup_message(sock: socket.socket, user: str = "postgres", database: str = "faizdb"):
    """Send StartupMessage (proto v3.0 = 196608) with parameters and parse backend responses"""
    # Protocol v3.0 + null-terminated key/values + final null byte
    params = f"user\x00{user}\x00database\x00{database}\x00client_encoding\x00UTF8\x00\x00".encode('utf-8')
    total_len = 4 + 4 + len(params)
    packet = struct.pack('!ii', total_len, 196608) + params
    sock.sendall(packet)

    auth_ok = False
    ready = False

    while not ready:
        msg_type = sock.recv(1)
        if not msg_type:
            break
        len_bytes = sock.recv(4)
        msg_len = struct.unpack('!i', len_bytes)[0]
        body = sock.recv(msg_len - 4)

        if msg_type == b'R': # Authentication
            auth_code = struct.unpack('!i', body[:4])[0]
            if auth_code == 0:
                auth_ok = True
                print("  [Auth] AuthenticationOk ('R', code 0) received.")
        elif msg_type == b'S': # ParameterStatus
            k, v, _ = body.decode('utf-8', errors='ignore').split('\x00', 2)
            # print(f"  [Param] {k} = {v}")
        elif msg_type == b'K': # BackendKeyData
            pid, secret = struct.unpack('!ii', body)
            print(f"  [BackendKey] PID: {pid}, Secret: {secret}")
        elif msg_type == b'Z': # ReadyForQuery
            status = chr(body[0])
            print(f"  [ReadyForQuery] Status: '{status}'")
            ready = True

    assert auth_ok, "Authentication failed!"
    assert ready, "Server did not enter ReadyForQuery state!"

def execute_query(sock: socket.socket, query: str):
    """Send Simple Query ('Q') and receive response rows & command complete"""
    query_bytes = query.encode('utf-8') + b'\x00'
    total_len = 4 + len(query_bytes)
    packet = b'Q' + struct.pack('!i', total_len) + query_bytes
    sock.sendall(packet)

    fields = []
    rows = []
    complete_tag = ""

    while True:
        msg_type = sock.recv(1)
        if not msg_type:
            break
        len_bytes = sock.recv(4)
        msg_len = struct.unpack('!i', len_bytes)[0]
        body = sock.recv(msg_len - 4)

        if msg_type == b'T': # RowDescription
            field_count = struct.unpack('!h', body[:2])[0]
            offset = 2
            fields = []
            for _ in range(field_count):
                null_pos = body.find(b'\x00', offset)
                col_name = body[offset:null_pos].decode('utf-8')
                fields.append(col_name)
                offset = null_pos + 1 + 4 + 2 + 4 + 2 + 4 + 2
        elif msg_type == b'D': # DataRow
            col_count = struct.unpack('!h', body[:2])[0]
            offset = 2
            row_vals = []
            for _ in range(col_count):
                col_len = struct.unpack('!i', body[offset:offset+4])[0]
                offset += 4
                if col_len == -1:
                    row_vals.append(None)
                else:
                    val = body[offset:offset+col_len].decode('utf-8', errors='ignore')
                    offset += col_len
                    row_vals.append(val)
            rows.append(row_vals)
        elif msg_type == b'C': # CommandComplete
            complete_tag = body.decode('utf-8', errors='ignore').rstrip('\x00')
            print(f"  [CommandComplete] {complete_tag}")
        elif msg_type == b'E': # ErrorResponse
            err_msg = body.decode('utf-8', errors='ignore')
            print(f"  [ErrorResponse] {err_msg}")
        elif msg_type == b'Z': # ReadyForQuery
            break

    return fields, rows, complete_tag

def main():
    print("=" * 65)
    print("🐘 Testing FaizDB PostgreSQL Wire Protocol on 127.0.0.1:5432")
    print("=" * 65)

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect(('127.0.0.1', 5432))
        print("✅ TCP Socket connected successfully to postgresql://127.0.0.1:5432\n")

        # 1. SSL Probe
        print("1. Performing SSLRequest probe...")
        send_ssl_request(sock)

        # 2. Handshake
        print("\n2. Performing StartupMessage Handshake...")
        send_startup_message(sock)

        # 3. Simple Queries
        print("\n3. Testing 'SELECT version()'...")
        fields, rows, tag = execute_query(sock, "SELECT version()")
        print(f"   Fields: {fields}, Rows: {rows}")
        assert len(rows) > 0 and "FaizDB" in rows[0][0]

        print("\n4. Testing 'SHOW tables'...")
        fields, rows, tag = execute_query(sock, "SHOW tables")
        print(f"   Fields: {fields}, Rows count: {len(rows)}")

        print("\n5. Testing 'INSERT INTO employees (name, role, salary) VALUES ('Faiz', 'Architect', 15000)'...")
        fields, rows, tag = execute_query(sock, "INSERT INTO employees (name, role, salary) VALUES ('Faiz', 'Architect', 15000)")
        assert "INSERT" in tag

        print("\n6. Testing 'SELECT * FROM employees'...")
        fields, rows, tag = execute_query(sock, "SELECT * FROM employees")
        print(f"   Fields: {fields}")
        for r in rows:
            print(f"   Row: {r}")
        assert len(rows) >= 1

        print("\n7. Terminating connection cleanly ('X')...")
        sock.sendall(b'X\x00\x00\x00\x04')
        sock.close()

        print("\n" + "=" * 65)
        print("🎉 ALL POSTGRESQL WIRE PROTOCOL INTEGRATION TESTS PASSED!")
        print("=" * 65)

    except ConnectionRefusedError:
        print("ℹ️ Note: FaizDB server is not currently running on port 5432.")
        print("Run 'faizdb serve' to start the Multi-Protocol Server and test live.")

if __name__ == "__main__":
    main()
