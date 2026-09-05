//! Integration and Unit Tests for MySQL / MariaDB Wire Protocol (Port 3306)

use bytes::BytesMut;
use faizdb_query::DatabaseContext;
use faizdb_security::UserStore;
use faizdb_server::wire::mysql::codec::{
    build_handshake_v10, encode_packet, parse_handshake_response,
    CLIENT_CONNECT_WITH_DB, CLIENT_PLUGIN_AUTH, CLIENT_PROTOCOL_41,
};
use faizdb_server::wire::mysql::handler::handle_mysql_query;
use faizdb_server::wire::mysql::run_mysql_server_with_shutdown;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[test]
fn test_mysql_codec_handshake_roundtrip() {
    let salt = [42u8; 20];
    let packet = build_handshake_v10(1, &salt);

    assert!(packet.len() >= 36);
    assert_eq!(packet[3], 0); // Sequence ID 0
    assert_eq!(packet[4], 10); // Protocol version 10

    // Construct synthetic client HandshakeResponse41 packet
    let mut payload = BytesMut::with_capacity(128);
    let caps = CLIENT_PROTOCOL_41 | CLIENT_CONNECT_WITH_DB | CLIENT_PLUGIN_AUTH;
    payload.extend_from_slice(&caps.to_le_bytes()); // capabilities
    payload.extend_from_slice(&16777216u32.to_le_bytes()); // max packet size (16MB)
    payload.extend_from_slice(&[45u8]); // utf8mb4 charset
    payload.extend_from_slice(&[0u8; 23]); // 23 reserved bytes
    payload.extend_from_slice(b"root\0"); // username
    payload.extend_from_slice(&[0u8]); // auth response len = 0
    payload.extend_from_slice(b"faizdb\0"); // database
    payload.extend_from_slice(b"mysql_native_password\0"); // plugin name

    let resp = parse_handshake_response(payload.freeze()).expect("Valid client response");
    assert_eq!(resp.username, "root");
    assert_eq!(resp.database.as_deref(), Some("faizdb"));
    assert_eq!(resp.auth_plugin_name.as_deref(), Some("mysql_native_password"));
}

#[test]
fn test_mysql_system_variable_queries() {
    let db = Arc::new(DatabaseContext::new());
    let res = handle_mysql_query(&db, "faizdb", "SELECT @@version", 1);
    // Expect: [column_count, column_def, eof, row, eof]
    assert_eq!(res.len(), 5);

    // Verify row contains 8.0.35
    let row_pkt = &res[3];
    let row_str = String::from_utf8_lossy(row_pkt);
    assert!(row_str.contains("8.0.35"));
}

#[test]
fn test_mysql_query_dml_and_select() {
    let db = Arc::new(DatabaseContext::new());
    db.get_or_create_collection("products");

    // Insert document using SQL
    let insert_res = handle_mysql_query(
        &db,
        "faizdb",
        "INSERT INTO products (id, name, price) VALUES ('p1', 'Laptop', 1500)",
        1,
    );
    assert_eq!(insert_res.len(), 1);
    assert_eq!(insert_res[0][4], 0x00); // OK Header

    // Select query
    let select_res = handle_mysql_query(&db, "faizdb", "SELECT * FROM products", 1);
    // 1 count + 4 col defs + 1 eof + 1 row + 1 eof = 8 packets
    assert_eq!(select_res.len(), 8);
    let row_pkt = &select_res[6];
    let select_str = String::from_utf8_lossy(row_pkt);
    assert!(select_str.contains("Laptop"));
}

#[tokio::test]
async fn test_mysql_wire_full_tcp_handshake_and_ping() {
    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());

    // Bind to random available local port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = listener.local_addr().unwrap();
    drop(listener);

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server_addr = local_addr.to_string();

    let srv_db = db.clone();
    let srv_users = user_store.clone();
    let srv_addr = server_addr.clone();

    let server_task = tokio::spawn(async move {
        let shutdown_fut = async move {
            let _ = shutdown_rx.await;
        };
        run_mysql_server_with_shutdown(&srv_addr, srv_db, srv_users, shutdown_fut)
            .await
            .unwrap();
    });

    // Give server a moment to bind
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Connect via raw TCP as a MySQL client
    let mut client = TcpStream::connect(&server_addr).await.expect("TCP connect");

    // 1. Read Server Initial HandshakeV10 packet
    let mut header = [0u8; 4];
    client.read_exact(&mut header).await.unwrap();
    let payload_len = (header[0] as usize) | ((header[1] as usize) << 8) | ((header[2] as usize) << 16);
    let seq_id = header[3];
    assert_eq!(seq_id, 0);

    let mut greeting_payload = vec![0u8; payload_len];
    client.read_exact(&mut greeting_payload).await.unwrap();
    assert_eq!(greeting_payload[0], 10); // HandshakeV10 protocol version

    // 2. Send HandshakeResponse41
    let mut resp_payload = BytesMut::new();
    let caps = CLIENT_PROTOCOL_41 | CLIENT_CONNECT_WITH_DB;
    resp_payload.extend_from_slice(&caps.to_le_bytes());
    resp_payload.extend_from_slice(&16777216u32.to_le_bytes());
    resp_payload.extend_from_slice(&[45u8]); // utf8mb4
    resp_payload.extend_from_slice(&[0u8; 23]); // 23 reserved
    resp_payload.extend_from_slice(b"root\0"); // user
    resp_payload.extend_from_slice(&[0u8]); // auth len 0
    resp_payload.extend_from_slice(b"faizdb\0"); // db

    let client_resp_pkt = encode_packet(1, &resp_payload);
    client.write_all(&client_resp_pkt).await.unwrap();
    client.flush().await.unwrap();

    // 3. Read OK_Packet
    let mut ok_header = [0u8; 4];
    client.read_exact(&mut ok_header).await.unwrap();
    let ok_len = (ok_header[0] as usize) | ((ok_header[1] as usize) << 8) | ((ok_header[2] as usize) << 16);
    let mut ok_payload = vec![0u8; ok_len];
    client.read_exact(&mut ok_payload).await.unwrap();
    assert_eq!(ok_payload[0], 0x00); // OK Header

    // 4. Send COM_PING (0x0E)
    let ping_pkt = encode_packet(0, &[0x0E]);
    client.write_all(&ping_pkt).await.unwrap();
    client.flush().await.unwrap();

    // 5. Read OK response from COM_PING
    let mut ping_resp_header = [0u8; 4];
    client.read_exact(&mut ping_resp_header).await.unwrap();
    let resp_len = (ping_resp_header[0] as usize)
        | ((ping_resp_header[1] as usize) << 8)
        | ((ping_resp_header[2] as usize) << 16);
    let mut ping_resp_payload = vec![0u8; resp_len];
    client.read_exact(&mut ping_resp_payload).await.unwrap();
    assert_eq!(ping_resp_payload[0], 0x00); // OK Header

    // 6. Send COM_QUIT (0x01)
    let quit_pkt = encode_packet(0, &[0x01]);
    client.write_all(&quit_pkt).await.unwrap();
    client.flush().await.unwrap();

    // Shutdown server
    let _ = shutdown_tx.send(());
    let _ = server_task.await;
}
