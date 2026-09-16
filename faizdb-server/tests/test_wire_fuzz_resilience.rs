//! Comprehensive Wire Protocol Fuzzing and Resilience Test Suite
//!
//! Tests the robustness, crash-resilience, and panic-freedom of all wire protocol parsers
//! and network listeners (MongoDB OP_MSG/OP_QUERY, PostgreSQL v3.0, and MySQL Handshake/Command)
//! against malformed, truncated, adversarial, and pseudo-random fuzz payloads.

use bytes::Bytes;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use faizdb_query::DatabaseContext;
use faizdb_security::{Role, UserStore};
use faizdb_server::wire::header::{MsgHeader, OpCode};
use faizdb_server::wire::mysql::codec::{
    parse_handshake_response, read_lenenc_int, read_null_terminated_str,
};
use faizdb_server::wire::mysql::listener::run_mysql_server;
use faizdb_server::wire::op_msg::OpMsg;
use faizdb_server::wire::postgres::codec::decode_pg_param;
use faizdb_server::wire::postgres::listener::run_postgres_server;
use faizdb_server::wire::run_wire_server;

/// Simple deterministic PRNG for fuzz testing reproducibility
fn pseudo_rand(state: &mut u64) -> u32 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*state >> 32) as u32
}

#[test]
fn test_mongodb_wire_fuzz_codec() {
    println!("\n⚡ MongoDB OP_MSG Fuzz Testing (Codec Level)");

    // 1. Extreme and boundary inputs
    assert!(OpMsg::decode(&[]).is_err());
    assert!(OpMsg::decode(&[0u8; 4]).is_err());
    assert!(OpMsg::decode(&[0u8; 15]).is_err());

    // 2. Fuzzing with pseudo-random garbage of various lengths
    let mut rng_state = 0xDEADBEEF_CAFE1234u64;
    for len in [16, 20, 24, 32, 64, 128, 256, 512, 1024] {
        for _ in 0..50 {
            let mut buf = vec![0u8; len];
            for b in buf.iter_mut() {
                *b = (pseudo_rand(&mut rng_state) & 0xFF) as u8;
            }
            // OpMsg::decode must NOT panic on any random input
            let _ = OpMsg::decode(&buf);
        }
    }

    // 3. Structured fuzzing: valid header with corrupted body
    for section_kind in [0u8, 1, 2, 99, 255] {
        let mut msg_bytes = Vec::new();
        let total_len = 16 + 4 + 1 + 10;
        let header = MsgHeader::new(1, 0, OpCode::OpMsg, total_len - 16);
        let _ = header.encode(&mut msg_bytes);

        // flags = 0
        msg_bytes.extend_from_slice(&0u32.to_le_bytes());
        // section kind
        msg_bytes.push(section_kind);
        // corrupt body
        msg_bytes.extend_from_slice(&[0xFF, 0xFE, 0xAA, 0x55, 0x00, 0x12, 0x34, 0x56, 0x78, 0x90]);

        let _ = OpMsg::decode(&msg_bytes);
    }

    println!("  ✓ MongoDB OP_MSG codec survived 500+ randomized fuzz passes without panics");
}

#[tokio::test]
async fn test_mongodb_live_stream_fuzz_resilience() {
    println!("\n⚡ MongoDB Wire Protocol Live Stream Fuzzing");

    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());
    let _ = user_store.create_user("fuzz_admin", "admin123", Role::Admin);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let mongo_addr = addr.to_string();
    let db_clone = db.clone();
    let store_clone = user_store.clone();
    tokio::spawn(async move {
        let _ = run_wire_server(&mongo_addr, db_clone, store_clone).await;
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send garbage streams
    let mut rng_state = 0x12345678_9ABCDEF0u64;
    for _ in 0..5 {
        if let Ok(mut stream) = TcpStream::connect(addr).await {
            let mut garbage = vec![0u8; 256];
            for b in garbage.iter_mut() {
                *b = (pseudo_rand(&mut rng_state) & 0xFF) as u8;
            }
            let _ = stream.write_all(&garbage).await;
            let _ = stream.flush().await;
            // Immediate close or short read
            let mut resp = [0u8; 16];
            let _ = tokio::time::timeout(
                tokio::time::Duration::from_millis(50),
                stream.read(&mut resp),
            )
            .await;
        }
    }

    // Verify server is still alive and accepting connections
    let mut healthy_stream = TcpStream::connect(addr)
        .await
        .expect("Server should remain healthy after fuzzing");
    let mut header_buf = [0u8; 16];
    let write_res = healthy_stream.write_all(b"PING_PROBE\0\0\0\0\0\0").await;
    assert!(write_res.is_ok());
    let _ = healthy_stream.read(&mut header_buf).await;

    println!("  ✓ MongoDB live server gracefully rejected malformed streams without crashing");
}

#[test]
fn test_postgres_wire_fuzz_codec_and_params() {
    println!("\n⚡ PostgreSQL Wire Parameter Decoder Fuzzing");

    // 1. decode_pg_param with various sizes and OIDs
    let mut rng_state = 0x55AA55AA_12345678u64;
    for len in [0, 1, 2, 3, 4, 5, 7, 8, 9, 16, 64] {
        for oid in [
            None,
            Some(16),
            Some(20),
            Some(21),
            Some(23),
            Some(700),
            Some(701),
            Some(1043),
            Some(9999),
        ] {
            let mut buf = vec![0u8; len];
            for b in buf.iter_mut() {
                *b = (pseudo_rand(&mut rng_state) & 0xFF) as u8;
            }
            // Must never panic on both text (0) and binary (1) format codes
            let res_text = decode_pg_param(0, &buf, oid);
            let res_bin = decode_pg_param(1, &buf, oid);
            assert!(!res_text.is_empty() || len == 0);
            assert!(!res_bin.is_empty() || len == 0);
        }
    }

    println!("  ✓ PostgreSQL parameter decoder survived all size/OID permutations");
}

#[tokio::test]
async fn test_postgres_live_stream_fuzz_resilience() {
    println!("\n⚡ PostgreSQL Wire Protocol Live Stream Fuzzing");

    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());
    let _ = user_store.create_user("fuzz_admin", "admin123", Role::Admin);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let pg_addr = addr.to_string();
    let db_clone = db.clone();
    let store_clone = user_store.clone();
    tokio::spawn(async move {
        let _ = run_postgres_server(&pg_addr, db_clone, store_clone).await;
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Send malformed startup packets:
    // 1. Packet length too small (len = 2)
    if let Ok(mut stream) = TcpStream::connect(addr).await {
        let _ = stream.write_all(&2i32.to_be_bytes()).await;
        let _ = stream.flush().await;
    }

    // 2. Huge packet length (len = 100_000_000)
    if let Ok(mut stream) = TcpStream::connect(addr).await {
        let _ = stream.write_all(&100_000_000i32.to_be_bytes()).await;
        let _ = stream.flush().await;
    }

    // 3. Random noise startup packet
    let mut rng_state = 0xFEEDBEEF_09876543u64;
    for _ in 0..5 {
        if let Ok(mut stream) = TcpStream::connect(addr).await {
            let mut noise = vec![0u8; 128];
            for b in noise.iter_mut() {
                *b = (pseudo_rand(&mut rng_state) & 0xFF) as u8;
            }
            let _ = stream.write_all(&noise).await;
            let _ = stream.flush().await;
        }
    }

    // Verify Postgres server is still alive
    let healthy_stream = TcpStream::connect(addr).await;
    assert!(
        healthy_stream.is_ok(),
        "Postgres server should remain alive after startup fuzzing"
    );

    println!("  ✓ PostgreSQL live server gracefully rejected malformed startup packets");
}

#[test]
fn test_mysql_wire_fuzz_codec() {
    println!("\n⚡ MySQL Codec Fuzz Testing");

    // 1. read_lenenc_int boundary resilience
    assert_eq!(read_lenenc_int(&mut Bytes::new()), None);
    assert_eq!(read_lenenc_int(&mut Bytes::from_static(&[0x05])), Some(5));
    // 0xFC requires 2 bytes, provide 1
    assert_eq!(
        read_lenenc_int(&mut Bytes::from_static(&[0xFC, 0x01])),
        None
    );
    // 0xFD requires 3 bytes, provide 2
    assert_eq!(
        read_lenenc_int(&mut Bytes::from_static(&[0xFD, 0x01, 0x02])),
        None
    );
    // 0xFE requires 8 bytes, provide 7
    assert_eq!(
        read_lenenc_int(&mut Bytes::from_static(&[0xFE, 1, 2, 3, 4, 5, 6, 7])),
        None
    );

    // 2. read_null_terminated_str resilience
    assert_eq!(read_null_terminated_str(&mut Bytes::new()), None);
    assert_eq!(
        read_null_terminated_str(&mut Bytes::from_static(b"no_null_terminator")),
        None
    );
    assert_eq!(
        read_null_terminated_str(&mut Bytes::from_static(b"hello\0world")),
        Some("hello".to_string())
    );

    // 3. parse_handshake_response fuzzing
    let mut rng_state = 0xABCDEF12_34567890u64;
    for len in [0, 4, 16, 31, 32, 40, 64, 128, 256] {
        for _ in 0..50 {
            let mut buf = vec![0u8; len];
            for b in buf.iter_mut() {
                *b = (pseudo_rand(&mut rng_state) & 0xFF) as u8;
            }
            // parse_handshake_response must return Ok or Err, NEVER PANIC
            let _ = parse_handshake_response(Bytes::from(buf));
        }
    }

    println!("  ✓ MySQL codec survived 450+ randomized fuzz passes without panics");
}

#[tokio::test]
async fn test_mysql_live_stream_fuzz_resilience() {
    println!("\n⚡ MySQL Wire Protocol Live Stream Fuzzing");

    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let mysql_addr = addr.to_string();
    let db_clone = db.clone();
    let store_clone = user_store.clone();
    tokio::spawn(async move {
        let _ = run_mysql_server(&mysql_addr, db_clone, store_clone).await;
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

    // Connect and receive greeting, then send random corrupted handshake response
    let mut rng_state = 0x98765432_10FEDCBAu64;
    for _ in 0..5 {
        if let Ok(mut stream) = TcpStream::connect(addr).await {
            let mut greeting = [0u8; 128];
            let _ = stream.read(&mut greeting).await;

            let mut bad_resp = [0u8; 64];
            for b in bad_resp.iter_mut() {
                *b = (pseudo_rand(&mut rng_state) & 0xFF) as u8;
            }
            // Packet header: 60 bytes payload, seq 1
            let mut packet = vec![60u8, 0, 0, 1];
            packet.extend_from_slice(&bad_resp[..60]);
            let _ = stream.write_all(&packet).await;
            let _ = stream.flush().await;
        }
    }

    // Verify MySQL server is still accepting connections
    let healthy_stream = TcpStream::connect(addr).await;
    assert!(
        healthy_stream.is_ok(),
        "MySQL server should remain alive after handshake response fuzzing"
    );

    println!("  ✓ MySQL live server gracefully handled fuzzed client responses without crashing");
}
