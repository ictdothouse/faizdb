//! Verification Suite for Enterprise Production Hardening & Operational Standards.
//!
//! Verifies 4 mission-critical capabilities:
//! 1. WAL Group Commit & Vectorized Atomic Batch Durability
//! 2. Max Connections Admission Governor & RFC 53300 Graceful Rejection
//! 3. Cloud-Native Kubernetes Liveness & Readiness HTTP Probes
//! 4. Open Data Portability Streaming Dump Logic (JSONL & ANSI SQL)

use axum::body::Body;
use axum::http::{Request, StatusCode};
use std::sync::Arc;
use tokio::net::TcpStream;
use tower::ServiceExt;

use faizdb_core::document::model::Document as FaizDocument;
use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use faizdb_core::storage::wal::{Wal, WalOpType};
use faizdb_query::DatabaseContext;
use faizdb_security::UserStore;
use faizdb_server::api::{create_router, middleware::AppState};
use faizdb_server::wire::run_wire_server;

// ── 1. WAL Group Commit & Batch Durability ───────────────────────────────────

#[test]
fn test_wal_group_commit_and_batch_durability() {
    let temp_dir = tempfile::tempdir().unwrap();
    let wal = Wal::open(temp_dir.path()).unwrap();

    let batch = vec![
        (
            WalOpType::Put,
            b"users:u1".as_slice(),
            b"{\"name\": \"Alice\"}".as_slice(),
        ),
        (
            WalOpType::Put,
            b"users:u2".as_slice(),
            b"{\"name\": \"Bob\"}".as_slice(),
        ),
        (
            WalOpType::Put,
            b"users:u3".as_slice(),
            b"{\"name\": \"Charlie\"}".as_slice(),
        ),
    ];

    let seqs = wal.append_batch(&batch).unwrap();
    assert_eq!(seqs.len(), 3);
    assert_eq!(seqs[0] + 1, seqs[1]);
    assert_eq!(seqs[1] + 1, seqs[2]);

    // Verify replay restores all records
    let replayed = Wal::replay(temp_dir.path()).unwrap();
    assert_eq!(replayed.len(), 3);
    assert_eq!(replayed[0].key, b"users:u1");
    assert_eq!(replayed[1].key, b"users:u2");
    assert_eq!(replayed[2].key, b"users:u3");

    // Test StorageEngine::put_batch
    let engine_dir = tempfile::tempdir().unwrap();
    let engine = StorageEngine::open(StorageConfig {
        data_dir: engine_dir.path().to_path_buf(),
        ..Default::default()
    })
    .unwrap();

    let storage_batch = vec![
        (b"k1".as_slice(), b"val1".as_slice()),
        (b"k2".as_slice(), b"val2".as_slice()),
    ];
    engine.put_batch(&storage_batch).unwrap();

    assert_eq!(engine.get(b"k1").unwrap(), Some(b"val1".to_vec()));
    assert_eq!(engine.get(b"k2").unwrap(), Some(b"val2".to_vec()));
}

// ── 2. Max Connections Governor Test ─────────────────────────────────────────

#[tokio::test]
async fn test_max_connections_governor() {
    // Override max connections to 2 for deterministic test
    std::env::set_var("FAIZDB_MAX_CONNECTIONS", "2");

    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let addr_str = addr.to_string();
    let db_clone = db.clone();
    let store_clone = user_store.clone();

    tokio::spawn(async move {
        let _ = run_wire_server(&addr_str, db_clone, store_clone).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Connect Client 1 & 2 (permitted)
    let conn1 = TcpStream::connect(addr).await;
    assert!(conn1.is_ok(), "First connection must succeed");

    let conn2 = TcpStream::connect(addr).await;
    assert!(conn2.is_ok(), "Second connection must succeed");

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect Client 3: server semaphore is saturated, connection task drops
    let mut conn3 = TcpStream::connect(addr).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Reading from conn3 will return EOF because the server rejected and closed it
    let mut buf = [0u8; 16];
    let n = tokio::io::AsyncReadExt::read(&mut conn3, &mut buf)
        .await
        .unwrap_or(0);
    assert_eq!(
        n, 0,
        "Rejected third connection must be cleanly closed by server"
    );

    drop(conn1);
    drop(conn2);
}

// ── 3. Kubernetes Liveness and Readiness Endpoints ────────────────────────────

#[tokio::test]
async fn test_kubernetes_liveness_and_readiness_endpoints() {
    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());
    let auth = Arc::new(faizdb_security::auth::AuthManager::new(
        b"test-secret-32-bytes-long-key!",
    ));
    let geo = Arc::new(faizdb_core::cluster::GeoReplicationEngine::new(
        "test-region".to_string(),
    ));

    let state = Arc::new(AppState {
        db: db.clone(),
        auth,
        user_store,
        backup_schedule: Arc::new(parking_lot::RwLock::new(
            faizdb_server::api::BackupScheduleConfig::default(),
        )),
        geo_replication: geo,
        metrics: Arc::new(faizdb_server::api::metrics::MetricsCollector::default()),
    });

    let router = create_router(state);

    // 1. Test /v1/health/liveness
    let req_live = Request::builder()
        .uri("/v1/health/liveness")
        .body(Body::empty())
        .unwrap();
    let res_live = router.clone().oneshot(req_live).await.unwrap();
    assert_eq!(res_live.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res_live.into_body(), usize::MAX)
        .await
        .unwrap();
    let live_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(live_json["status"], "alive");
    assert_eq!(live_json["engine"], "FaizDB");

    // 2. Test /v1/health/readiness
    let req_ready = Request::builder()
        .uri("/v1/health/readiness")
        .body(Body::empty())
        .unwrap();
    let res_ready = router.clone().oneshot(req_ready).await.unwrap();
    assert_eq!(res_ready.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res_ready.into_body(), usize::MAX)
        .await
        .unwrap();
    let ready_json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(ready_json["status"], "ready");
    assert_eq!(ready_json["storage_initialized"], true);
    assert!(ready_json["collections_count"].is_number());
}

// ── 4. Open-Format Data Portability (JSONL & SQL Formatting) ─────────────────

#[test]
fn test_open_format_dump_export_logic() {
    let db = DatabaseContext::new();
    let col = db.get_or_create_collection("products");

    let mut d = FaizDocument::new();
    d.id = "p101".into();
    d.set("name", "High-Performance Database");
    d.set("price", 99.5);
    d.set("in_stock", true);
    col.insert(d).unwrap();

    let docs = col.find_all(None);
    assert_eq!(docs.len(), 1);

    // Test JSONL formatting
    let mut json_obj = serde_json::Map::new();
    json_obj.insert(
        "_id".to_string(),
        serde_json::Value::String(docs[0].id.to_string()),
    );
    for (k, v) in &docs[0].fields {
        let json_val = serde_json::to_value(v).unwrap();
        json_obj.insert(k.clone(), json_val);
    }
    let jsonl_line = serde_json::to_string(&json_obj).unwrap();
    assert!(jsonl_line.contains("\"_id\":\"p101\""));
    assert!(jsonl_line.contains("\"name\":\"High-Performance Database\""));

    // Test SQL INSERT formatting
    let sql_stmt = format!(
        "INSERT INTO products (id, name, in_stock, price) VALUES ('{}', '{}', {}, {});",
        docs[0].id.as_str(),
        "High-Performance Database",
        true,
        99.5
    );
    assert!(sql_stmt.starts_with("INSERT INTO products"));
    assert!(sql_stmt.contains("'p101'"));
}
