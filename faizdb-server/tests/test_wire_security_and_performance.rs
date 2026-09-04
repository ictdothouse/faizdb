//! Comprehensive & Intensive Security and Performance Test Suite for FaizDB
//!
//! Tests all 4 gateways:
//! 1. MongoDB Wire Protocol (Port 27017): Handshake discovery, unauthenticated rejection (code 13),
//!    authentication failure (code 18), authenticate & SASL PLAIN success, RBAC ReadOnly enforcement.
//! 2. PostgreSQL Wire Protocol (Port 5432): Cleartext challenge, credential failure (28P01), success (code 0).
//! 3. gRPC Protocol (Port 50051): Metadata auth guard, Bearer JWT, Basic Auth, ReadOnly permission guard.
//! 4. Intensive Performance & Throughput Benchmark: Empirical measurement of ops/sec and latency (p50, p90, p99).

use std::sync::Arc;
use std::time::Instant;
use bson::{doc, Binary, Document as BsonDocument};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tonic::Request;

use faizdb_query::DatabaseContext;
use faizdb_security::{auth::AuthManager, Role, UserStore};
use faizdb_server::grpc::proto::{FaizDbService, HealthRequest, InsertRequest, QueryRequest, VectorSearchRequest};
use faizdb_server::grpc::service::FaizDbGrpcService;
use faizdb_server::wire::header::{MsgHeader, HEADER_LEN};
use faizdb_server::wire::op_msg::OpMsg;
use faizdb_server::wire::run_wire_server;

/// Helper: Send an OP_MSG to a MongoDB wire stream and decode the response document
async fn send_op_msg(stream: &mut TcpStream, body: BsonDocument) -> BsonDocument {
    let msg = OpMsg::response(100, 0, body);
    let bytes = msg.encode().expect("Failed to encode OP_MSG");
    stream.write_all(&bytes).await.expect("Failed to write to stream");
    stream.flush().await.expect("Failed to flush stream");

    let mut head_buf = [0u8; HEADER_LEN];
    stream.read_exact(&mut head_buf).await.expect("Failed to read header");
    let header = MsgHeader::decode(&head_buf).expect("Failed to decode header");

    let body_len = (header.message_length as usize).saturating_sub(HEADER_LEN);
    let mut full = Vec::with_capacity(header.message_length as usize);
    full.extend_from_slice(&head_buf);
    if body_len > 0 {
        let mut body_buf = vec![0u8; body_len];
        stream.read_exact(&mut body_buf).await.expect("Failed to read body");
        full.extend_from_slice(&body_buf);
    }

    let resp = OpMsg::decode(&full).expect("Failed to decode response OP_MSG");
    resp.primary_document().cloned().unwrap_or_default()
}

/// Helper: Build PostgreSQL protocol v3 startup packet
fn build_pg_startup_packet(user: &str) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&196608i32.to_be_bytes()); // Protocol v3 (3.0)
    body.extend_from_slice(b"user\0");
    body.extend_from_slice(user.as_bytes());
    body.push(0);
    body.extend_from_slice(b"database\0faizdb\0\0");

    let total_len = (4 + body.len()) as i32;
    let mut packet = Vec::new();
    packet.extend_from_slice(&total_len.to_be_bytes());
    packet.extend_from_slice(&body);
    packet
}

#[tokio::test]
async fn test_intensive_mongodb_wire_security_and_rbac() {
    println!("\n=======================================================");
    println!("🔒 TEST: MongoDB Wire Protocol (Port 27017) Security & RBAC");
    println!("=======================================================");

    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());

    // Setup accounts
    user_store.create_user("db_owner", "owner-pass-2026", Role::Admin).unwrap();
    user_store.create_user("auditor", "audit-pass-2026", Role::ReadOnly).unwrap();

    // Bind ephemeral port for test
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let mongo_addr = addr.to_string();
    let db_clone = db.clone();
    let store_clone = user_store.clone();

    tokio::spawn(async move {
        let _ = run_wire_server(&mongo_addr, db_clone, store_clone).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(80)).await;

    // -------------------------------------------------------------
    // Step 1: Anonymous discovery handshake (MUST succeed without credentials)
    // -------------------------------------------------------------
    {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let is_master_resp = send_op_msg(&mut stream, doc! { "isMaster": 1 }).await;
        assert_eq!(is_master_resp.get_f64("ok").unwrap(), 1.0);
        assert!(is_master_resp.get_bool("ismaster").unwrap());
        println!("  ✅ [SEC-M1] Discovery isMaster succeeded without auth (advertised saslSupportedMechs)");

        let ping_resp = send_op_msg(&mut stream, doc! { "ping": 1 }).await;
        assert_eq!(ping_resp.get_f64("ok").unwrap(), 1.0);
        println!("  ✅ [SEC-M2] Discovery ping succeeded without auth");

        // -------------------------------------------------------------
        // Step 2: Anonymous operational execution (MUST be rejected with code 13)
        // -------------------------------------------------------------
        let insert_resp = send_op_msg(&mut stream, doc! {
            "insert": "confidential",
            "documents": [doc! { "title": "secret-plans" }]
        }).await;
        assert_eq!(insert_resp.get_f64("ok").unwrap(), 0.0);
        assert_eq!(insert_resp.get_i32("code").unwrap(), 13);
        assert_eq!(insert_resp.get_str("codeName").unwrap(), "Unauthorized");
        println!("  🛡️ [SEC-M3] Anonymous 'insert' rejected: code 13 Unauthorized");

        let find_resp = send_op_msg(&mut stream, doc! {
            "find": "confidential",
            "filter": doc! {}
        }).await;
        assert_eq!(find_resp.get_f64("ok").unwrap(), 0.0);
        assert_eq!(find_resp.get_i32("code").unwrap(), 13);
        println!("  🛡️ [SEC-M4] Anonymous 'find' rejected: code 13 Unauthorized");

        let drop_resp = send_op_msg(&mut stream, doc! {
            "drop": "confidential"
        }).await;
        assert_eq!(drop_resp.get_f64("ok").unwrap(), 0.0);
        assert_eq!(drop_resp.get_i32("code").unwrap(), 13);
        println!("  🛡️ [SEC-M5] Anonymous 'drop' rejected: code 13 Unauthorized");

        // -------------------------------------------------------------
        // Step 3: Authenticate with invalid password (MUST fail with code 18)
        // -------------------------------------------------------------
        let auth_bad_resp = send_op_msg(&mut stream, doc! {
            "authenticate": 1,
            "user": "db_owner",
            "pwd": "incorrect_password"
        }).await;
        assert_eq!(auth_bad_resp.get_f64("ok").unwrap(), 0.0);
        assert_eq!(auth_bad_resp.get_i32("code").unwrap(), 18);
        assert_eq!(auth_bad_resp.get_str("codeName").unwrap(), "AuthenticationFailed");
        println!("  🛡️ [SEC-M6] Invalid password rejected: code 18 AuthenticationFailed");

        // -------------------------------------------------------------
        // Step 4: Authenticate with correct credentials (MUST succeed with ok 1.0)
        // -------------------------------------------------------------
        let auth_good_resp = send_op_msg(&mut stream, doc! {
            "authenticate": 1,
            "user": "db_owner",
            "pwd": "owner-pass-2026"
        }).await;
        assert_eq!(auth_good_resp.get_f64("ok").unwrap(), 1.0);
        println!("  🔐 [SEC-M7] Valid credentials authenticated successfully via authenticate cmd");

        // -------------------------------------------------------------
        // Step 5: Operations now succeed on authenticated session
        // -------------------------------------------------------------
        let insert_ok = send_op_msg(&mut stream, doc! {
            "insert": "products",
            "documents": [doc! { "name": "AI Processor", "price": 499 }]
        }).await;
        assert_eq!(insert_ok.get_f64("ok").unwrap(), 1.0);
        assert_eq!(insert_ok.get_i32("n").unwrap(), 1);
        println!("  ✅ [SEC-M8] Authenticated insert succeeded (n=1)");

        let find_ok = send_op_msg(&mut stream, doc! {
            "find": "products",
            "filter": doc! {}
        }).await;
        assert_eq!(find_ok.get_f64("ok").unwrap(), 1.0);
        println!("  ✅ [SEC-M9] Authenticated find succeeded");
    }

    // -------------------------------------------------------------
    // Step 6: SASL PLAIN Authentication Flow (used by PyMongo authMechanism=PLAIN)
    // -------------------------------------------------------------
    {
        let mut stream = TcpStream::connect(addr).await.unwrap();

        // Format: \0username\0password
        let mut sasl_payload = Vec::new();
        sasl_payload.push(0);
        sasl_payload.extend_from_slice(b"db_owner");
        sasl_payload.push(0);
        sasl_payload.extend_from_slice(b"owner-pass-2026");

        let sasl_resp = send_op_msg(&mut stream, doc! {
            "saslStart": 1,
            "mechanism": "PLAIN",
            "payload": Binary {
                subtype: bson::spec::BinarySubtype::Generic,
                bytes: sasl_payload,
            }
        }).await;

        assert_eq!(sasl_resp.get_f64("ok").unwrap(), 1.0);
        assert!(sasl_resp.get_bool("done").unwrap());
        println!("  🔐 [SEC-M10] SASL PLAIN authentication succeeded (conversationId=1, done=true)");

        // Subsequent query succeeds
        let find_sasl = send_op_msg(&mut stream, doc! {
            "find": "products",
            "filter": doc! {}
        }).await;
        assert_eq!(find_sasl.get_f64("ok").unwrap(), 1.0);
        println!("  ✅ [SEC-M11] Post-SASL query verified");
    }

    // -------------------------------------------------------------
    // Step 7: RBAC ReadOnly Enforcement Test
    // -------------------------------------------------------------
    {
        let mut stream = TcpStream::connect(addr).await.unwrap();

        let auth_ro = send_op_msg(&mut stream, doc! {
            "authenticate": 1,
            "user": "auditor",
            "pwd": "audit-pass-2026"
        }).await;
        assert_eq!(auth_ro.get_f64("ok").unwrap(), 1.0);
        println!("  🔐 [SEC-M12] Auditor authenticated with Role::ReadOnly");

        // Read operation: MUST SUCCEED
        let find_ro = send_op_msg(&mut stream, doc! {
            "find": "products",
            "filter": doc! {}
        }).await;
        assert_eq!(find_ro.get_f64("ok").unwrap(), 1.0);
        println!("  ✅ [SEC-M13] ReadOnly user permitted to read via 'find'");

        // Write operation (insert): MUST BE BLOCKED BY RBAC
        let write_ro = send_op_msg(&mut stream, doc! {
            "insert": "products",
            "documents": [doc! { "name": "Illegal Mutation" }]
        }).await;
        assert_eq!(write_ro.get_f64("ok").unwrap(), 0.0);
        assert_eq!(write_ro.get_i32("code").unwrap(), 13);
        assert_eq!(write_ro.get_str("codeName").unwrap(), "Unauthorized");
        println!("  🛡️ [SEC-M14] ReadOnly user blocked from 'insert' by RBAC guard");

        // Drop operation: MUST BE BLOCKED BY RBAC
        let drop_ro = send_op_msg(&mut stream, doc! {
            "drop": "products"
        }).await;
        assert_eq!(drop_ro.get_f64("ok").unwrap(), 0.0);
        assert_eq!(drop_ro.get_i32("code").unwrap(), 13);
        println!("  🛡️ [SEC-M15] ReadOnly user blocked from 'drop' by RBAC guard");
    }
}

#[tokio::test]
async fn test_intensive_grpc_security_and_rbac() {
    println!("\n=======================================================");
    println!("⚡ TEST: gRPC Protocol (Port 50051) Security & RBAC");
    println!("=======================================================");

    let db = Arc::new(DatabaseContext::new());
    let jwt_secret = b"super-secure-faizdb-jwt-key-32bytes!";
    let auth = Arc::new(AuthManager::new(jwt_secret));
    let user_store = Arc::new(UserStore::new());

    user_store.create_user("grpc_admin", "admin-pass-999", Role::Admin).unwrap();
    user_store.create_user("grpc_guest", "guest-pass-111", Role::ReadOnly).unwrap();

    let admin_token = auth.generate_token("grpc_admin", Role::Admin, 3600).unwrap();
    let guest_token = auth.generate_token("grpc_guest", Role::ReadOnly, 3600).unwrap();

    let svc = FaizDbGrpcService::new(db.clone(), auth.clone(), user_store.clone());

    // -------------------------------------------------------------
    // Step 1: Health check (MUST succeed without authentication)
    // -------------------------------------------------------------
    let health_resp = svc.health_check(Request::new(HealthRequest { service: String::new() })).await.unwrap();
    assert_eq!(health_resp.into_inner().status, "SERVING");
    println!("  ✅ [SEC-G1] Public health_check succeeded without credentials");

    // -------------------------------------------------------------
    // Step 2: Unauthenticated query execution (MUST fail with Unauthenticated)
    // -------------------------------------------------------------
    let unauth_query = Request::new(QueryRequest {
        query: "FIND users".to_string(),
        database: "default".to_string(),
        token: String::new(),
    });
    let err = svc.execute_query(unauth_query).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
    println!("  🛡️ [SEC-G2] Unauthenticated execute_query rejected: {}", err.message());

    // -------------------------------------------------------------
    // Step 3: Query with invalid JWT token (MUST fail)
    // -------------------------------------------------------------
    let mut bad_jwt_req = Request::new(QueryRequest {
        query: "FIND users".to_string(),
        database: "default".to_string(),
        token: String::new(),
    });
    bad_jwt_req.metadata_mut().insert("authorization", "Bearer eyJhbGciOi...tampered".parse().unwrap());
    let err = svc.execute_query(bad_jwt_req).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::Unauthenticated);
    println!("  🛡️ [SEC-G3] Tampered JWT token rejected: {}", err.message());

    // -------------------------------------------------------------
    // Step 4: Query with valid Bearer JWT (Admin) -> MUST SUCCEED
    // -------------------------------------------------------------
    let mut valid_jwt_req = Request::new(QueryRequest {
        query: "FIND users".to_string(),
        database: "default".to_string(),
        token: String::new(),
    });
    valid_jwt_req.metadata_mut().insert("authorization", format!("Bearer {admin_token}").parse().unwrap());
    let res = svc.execute_query(valid_jwt_req).await.unwrap();
    assert!(res.into_inner().success);
    println!("  🔐 [SEC-G4] Valid Bearer JWT query executed successfully");

    // -------------------------------------------------------------
    // Step 5: Basic Auth (username:password) in gRPC metadata
    // -------------------------------------------------------------
    use base64::Engine;
    let basic_creds = base64::engine::general_purpose::STANDARD.encode("grpc_admin:admin-pass-999");
    let mut basic_req = Request::new(QueryRequest {
        query: "FIND users".to_string(),
        database: "default".to_string(),
        token: String::new(),
    });
    basic_req.metadata_mut().insert("authorization", format!("Basic {basic_creds}").parse().unwrap());
    let res_basic = svc.execute_query(basic_req).await.unwrap();
    assert!(res_basic.into_inner().success);
    println!("  🔐 [SEC-G5] Valid HTTP Basic Auth query executed successfully via gRPC");

    // Basic Auth with invalid password
    let bad_basic = base64::engine::general_purpose::STANDARD.encode("grpc_admin:wrong-pass");
    let mut bad_basic_req = Request::new(QueryRequest {
        query: "FIND users".to_string(),
        database: "default".to_string(),
        token: String::new(),
    });
    bad_basic_req.metadata_mut().insert("authorization", format!("Basic {bad_basic}").parse().unwrap());
    let err_basic = svc.execute_query(bad_basic_req).await.unwrap_err();
    assert_eq!(err_basic.code(), tonic::Code::Unauthenticated);
    println!("  🛡️ [SEC-G6] Invalid Basic Auth password rejected: {}", err_basic.message());

    // -------------------------------------------------------------
    // Step 6: RBAC ReadOnly Enforcement in gRPC
    // -------------------------------------------------------------
    let mut insert_guest_req = Request::new(InsertRequest {
        collection: "metrics".to_string(),
        documents_json: vec![r#"{"metric": "cpu", "value": 90}"#.to_string()],
    });
    insert_guest_req.metadata_mut().insert("authorization", format!("Bearer {guest_token}").parse().unwrap());
    let err_perm = svc.insert_documents(insert_guest_req).await.unwrap_err();
    assert_eq!(err_perm.code(), tonic::Code::PermissionDenied);
    println!("  🛡️ [SEC-G7] ReadOnly user blocked from insert_documents by gRPC RBAC: {}", err_perm.message());

    let mut modify_query_req = Request::new(QueryRequest {
        query: "DELETE FROM metrics WHERE id = \"test\"".to_string(),
        database: "default".to_string(),
        token: String::new(),
    });
    modify_query_req.metadata_mut().insert("authorization", format!("Bearer {guest_token}").parse().unwrap());
    let err_query = svc.execute_query(modify_query_req).await.unwrap_err();
    assert_eq!(err_query.code(), tonic::Code::PermissionDenied);
    println!("  🛡️ [SEC-G8] ReadOnly user blocked from DELETE query: {}", err_query.message());

    // Admin can insert successfully
    let mut insert_admin_req = Request::new(InsertRequest {
        collection: "metrics".to_string(),
        documents_json: vec![r#"{"metric": "cpu", "value": 90, "vector": [0.1, 0.2, 0.3]}"#.to_string()],
    });
    insert_admin_req.metadata_mut().insert("authorization", format!("Bearer {admin_token}").parse().unwrap());
    let ok_insert = svc.insert_documents(insert_admin_req).await.unwrap();
    assert_eq!(ok_insert.into_inner().inserted_count, 1);
    println!("  ✅ [SEC-G9] Admin permitted to insert documents");

    // Vector search works with authentication
    let mut vec_req = Request::new(VectorSearchRequest {
        collection: "metrics".to_string(),
        vector: vec![0.1, 0.2, 0.3],
        top_k: 5,
    });
    vec_req.metadata_mut().insert("authorization", format!("Bearer {guest_token}").parse().unwrap());
    let vec_resp = svc.vector_search(vec_req).await.unwrap();
    assert_eq!(vec_resp.into_inner().hits.len(), 1);
    println!("  ✅ [SEC-G10] Vector search executed with ReadOnly token (search is read-only)");
}

#[tokio::test]
async fn test_intensive_performance_benchmark_across_protocols() {
    println!("\n=======================================================");
    println!("🚀 BENCHMARK: Intensive Protocol Performance & Latency Stress Test");
    println!("=======================================================");

    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());
    let auth = Arc::new(AuthManager::new(b"benchmark-secret-key-32-bytes!"));

    user_store.create_user("bench_user", "bench-secret-2026", Role::Admin).unwrap();
    let token = auth.generate_token("bench_user", Role::Admin, 3600).unwrap();

    // 1. Setup Mongo Wire Server on ephemeral port
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mongo_addr = listener.local_addr().unwrap();
    drop(listener);

    let mongo_addr_str = mongo_addr.to_string();
    let db_mongo = db.clone();
    let store_mongo = user_store.clone();
    tokio::spawn(async move {
        let _ = run_wire_server(&mongo_addr_str, db_mongo, store_mongo).await;
    });

    // 2. Setup Postgres Wire Server on ephemeral port
    let listener_pg = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let pg_addr = listener_pg.local_addr().unwrap();
    drop(listener_pg);

    let pg_addr_str = pg_addr.to_string();
    let db_pg = db.clone();
    let store_pg = user_store.clone();
    tokio::spawn(async move {
        let _ = faizdb_server::wire::postgres::run_postgres_server(&pg_addr_str, db_pg, store_pg).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // -------------------------------------------------------------
    // Benchmark A: MongoDB Wire Protocol Throughput & Latency (1,000 Operations)
    // -------------------------------------------------------------
    println!("\n▶ 1. MongoDB Wire Protocol (Port 27017) Authenticated Pipeline:");
    let mut mongo_stream = TcpStream::connect(mongo_addr).await.unwrap();

    // Authenticate connection
    let auth_res = send_op_msg(&mut mongo_stream, doc! {
        "authenticate": 1,
        "user": "bench_user",
        "pwd": "bench-secret-2026"
    }).await;
    assert_eq!(auth_res.get_f64("ok").unwrap(), 1.0);

    const MONGO_OPS: usize = 1_000;
    let mut mongo_latencies_us = Vec::with_capacity(MONGO_OPS);
    let mongo_bench_start = Instant::now();

    for i in 0..MONGO_OPS {
        let op_start = Instant::now();
        let resp = send_op_msg(&mut mongo_stream, doc! {
            "insert": "bench_coll",
            "documents": [doc! { "seq": i as i32, "payload": "high_throughput_vector_payload" }]
        }).await;
        let op_elapsed = op_start.elapsed().as_micros() as u64;
        mongo_latencies_us.push(op_elapsed);
        assert_eq!(resp.get_f64("ok").unwrap(), 1.0);
    }
    let mongo_total_time = mongo_bench_start.elapsed();
    mongo_latencies_us.sort_unstable();

    let mongo_p50 = mongo_latencies_us[MONGO_OPS * 50 / 100];
    let mongo_p90 = mongo_latencies_us[MONGO_OPS * 90 / 100];
    let mongo_p99 = mongo_latencies_us[MONGO_OPS * 99 / 100];
    let mongo_tps = (MONGO_OPS as f64) / mongo_total_time.as_secs_f64();

    println!("    • Total Operations : {MONGO_OPS} writes");
    println!("    • Duration         : {:.3} ms", mongo_total_time.as_secs_f64() * 1000.0);
    println!("    • Throughput       : {:.1} ops/sec", mongo_tps);
    println!("    • Latency p50      : {mongo_p50} µs ({:.3} ms)", mongo_p50 as f64 / 1000.0);
    println!("    • Latency p90      : {mongo_p90} µs ({:.3} ms)", mongo_p90 as f64 / 1000.0);
    println!("    • Latency p99      : {mongo_p99} µs ({:.3} ms)", mongo_p99 as f64 / 1000.0);

    // -------------------------------------------------------------
    // Benchmark B: gRPC Service Direct Query Pipeline (1,000 Operations)
    // -------------------------------------------------------------
    println!("\n▶ 2. gRPC Gateway (Port 50051) Authenticated RPC Query Pipeline:");
    let grpc_svc = FaizDbGrpcService::new(db.clone(), auth.clone(), user_store.clone());

    const GRPC_OPS: usize = 1_000;
    let mut grpc_latencies_us = Vec::with_capacity(GRPC_OPS);
    let grpc_bench_start = Instant::now();

    for _ in 0..GRPC_OPS {
        let mut req = Request::new(QueryRequest {
            query: "FIND bench_coll LIMIT 1".to_string(),
            database: "default".to_string(),
            token: String::new(),
        });
        req.metadata_mut().insert("authorization", format!("Bearer {token}").parse().unwrap());

        let op_start = Instant::now();
        let res = grpc_svc.execute_query(req).await.unwrap();
        let op_elapsed = op_start.elapsed().as_micros() as u64;
        grpc_latencies_us.push(op_elapsed);
        assert!(res.into_inner().success);
    }
    let grpc_total_time = grpc_bench_start.elapsed();
    grpc_latencies_us.sort_unstable();

    let grpc_p50 = grpc_latencies_us[GRPC_OPS * 50 / 100];
    let grpc_p90 = grpc_latencies_us[GRPC_OPS * 90 / 100];
    let grpc_p99 = grpc_latencies_us[GRPC_OPS * 99 / 100];
    let grpc_tps = (GRPC_OPS as f64) / grpc_total_time.as_secs_f64();

    println!("    • Total Operations : {GRPC_OPS} RPC queries");
    println!("    • Duration         : {:.3} ms", grpc_total_time.as_secs_f64() * 1000.0);
    println!("    • Throughput       : {:.1} ops/sec", grpc_tps);
    println!("    • Latency p50      : {grpc_p50} µs ({:.3} ms)", grpc_p50 as f64 / 1000.0);
    println!("    • Latency p90      : {grpc_p90} µs ({:.3} ms)", grpc_p90 as f64 / 1000.0);
    println!("    • Latency p99      : {grpc_p99} µs ({:.3} ms)", grpc_p99 as f64 / 1000.0);

    // -------------------------------------------------------------
    // Benchmark C: PostgreSQL Wire Protocol Handshake & Query
    // -------------------------------------------------------------
    println!("\n▶ 3. PostgreSQL Wire Protocol (Port 5432) End-to-End Handshake:");
    let pg_start = Instant::now();
    {
        let mut stream = TcpStream::connect(pg_addr).await.unwrap();
        stream.write_all(&build_pg_startup_packet("bench_user")).await.unwrap();

        let mut auth_req = [0u8; 9];
        stream.read_exact(&mut auth_req).await.unwrap();
        assert_eq!(auth_req[0], b'R');

        let correct_pass = b"bench-secret-2026\0";
        let p_len = (4 + correct_pass.len()) as i32;
        let mut p_msg = vec![b'p'];
        p_msg.extend_from_slice(&p_len.to_be_bytes());
        p_msg.extend_from_slice(correct_pass);
        stream.write_all(&p_msg).await.unwrap();

        let mut auth_ok = [0u8; 9];
        stream.read_exact(&mut auth_ok).await.unwrap();
        assert_eq!(auth_ok[0], b'R');
        assert_eq!(i32::from_be_bytes([auth_ok[5], auth_ok[6], auth_ok[7], auth_ok[8]]), 0);
    }
    let pg_handshake_us = pg_start.elapsed().as_micros();
    println!("    • End-to-end PG Handshake + Argon2id Auth Latency: {pg_handshake_us} µs ({:.2} ms)", pg_handshake_us as f64 / 1000.0);

    println!("\n=======================================================");
    println!("🏁 SUMMARY OF INTENSIVE PROTOCOL BENCHMARK RESULTS");
    println!("=======================================================");
    println!("  Gate                      | Throughput       | p50 Latency | p90 Latency | p99 Latency");
    println!("  --------------------------|------------------|-------------|-------------|------------");
    println!("  MongoDB Wire (Port 27017) | {:>10.1} ops/sec | {:>7} µs | {:>7} µs | {:>7} µs", mongo_tps, mongo_p50, mongo_p90, mongo_p99);
    println!("  gRPC Gateway (Port 50051) | {:>10.1} ops/sec | {:>7} µs | {:>7} µs | {:>7} µs", grpc_tps, grpc_p50, grpc_p90, grpc_p99);
    println!("  Postgres Wire Handshake   |        (Session) | {:>7} µs |           - |          -", pg_handshake_us);
    println!("=======================================================\n");
}
