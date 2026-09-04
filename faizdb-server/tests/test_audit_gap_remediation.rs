//! Integration tests verifying remediation of all audit gaps:
//! 1. SQL & Mongo UPDATE parsing and arithmetic execution.
//! 2. SQL ORDER BY and Mongo .sort() execution.
//! 3. REST API pagination on GET /v1/collections/{name}/documents.
//! 4. MongoDB Wire protocol: real drop, dynamic listCollections, query-filtered count, and sort.
//! 5. Persistent StorageEngine SSTable compaction.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bson::{doc, Document as BsonDocument};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tower::ServiceExt;

use faizdb_core::document::model::Document;
use faizdb_query::{parse_query, DatabaseContext, QueryResult};
use faizdb_security::{Role, UserStore};
use faizdb_server::api::{create_router, AppState};
use faizdb_server::wire::header::{MsgHeader, HEADER_LEN};
use faizdb_server::wire::op_msg::OpMsg;
use faizdb_server::wire::run_wire_server;

/// Helper: Send an OP_MSG to a MongoDB wire stream and decode the response document
async fn send_op_msg(stream: &mut TcpStream, body: BsonDocument) -> BsonDocument {
    let msg = OpMsg::response(100, 0, body);
    let bytes = msg.encode().expect("Failed to encode OP_MSG");
    stream
        .write_all(&bytes)
        .await
        .expect("Failed to write to stream");
    stream.flush().await.expect("Failed to flush stream");

    let mut head_buf = [0u8; HEADER_LEN];
    stream
        .read_exact(&mut head_buf)
        .await
        .expect("Failed to read header");
    let header = MsgHeader::decode(&head_buf).expect("Failed to decode header");

    let body_len = (header.message_length as usize).saturating_sub(HEADER_LEN);
    let mut full = Vec::with_capacity(header.message_length as usize);
    full.extend_from_slice(&head_buf);
    if body_len > 0 {
        let mut body_buf = vec![0u8; body_len];
        stream
            .read_exact(&mut body_buf)
            .await
            .expect("Failed to read body");
        full.extend_from_slice(&body_buf);
    }

    let resp = OpMsg::decode(&full).expect("Failed to decode response OP_MSG");
    resp.primary_document().cloned().unwrap_or_default()
}

fn setup_test_app() -> (axum::Router, Arc<AppState>, String) {
    let db = Arc::new(DatabaseContext::new());
    let auth = Arc::new(faizdb_security::auth::AuthManager::new(
        b"test-secret-key-remediation-1234",
    ));
    let user_store = Arc::new(UserStore::new());
    let geo = Arc::new(faizdb_core::cluster::GeoReplicationEngine::new(
        "test-region".to_string(),
    ));

    let token = auth.generate_token("admin", Role::Admin, 3600).unwrap();

    let state = Arc::new(AppState {
        db,
        auth,
        user_store,
        backup_schedule: Arc::new(parking_lot::RwLock::new(Default::default())),
        geo_replication: geo,
        metrics: Arc::new(Default::default()),
    });

    (create_router(state.clone()), state, token)
}

#[tokio::test]
async fn test_sql_and_mongo_update() {
    let db = DatabaseContext::new();
    let col = db.get_or_create_collection("leaderboards");

    let mut doc = Document::new();
    doc.set("player_id", "player_cyber_99");
    doc.set("score", 1000);
    doc.set("kills", 10);
    col.insert(doc).unwrap();

    // 1. Verify exact code example from docs/USE_CASES_AND_SOLUTIONS.md
    let sql = "UPDATE leaderboards SET score = score + 500, kills = kills + 2 WHERE player_id = 'player_cyber_99'";
    let stmt = parse_query(sql).expect("SQL UPDATE statement should parse successfully");
    let res = db.execute(stmt).expect("SQL UPDATE should execute");
    match res {
        QueryResult::Updated(count) => assert_eq!(count, 1),
        _ => panic!("Expected QueryResult::Updated"),
    }

    let updated = col
        .find_all(None)
        .into_iter()
        .find(|d| {
            d.get("player_id")
                == Some(&faizdb_core::document::model::Value::String(
                    "player_cyber_99".to_string(),
                ))
        })
        .unwrap();

    assert_eq!(
        updated.get("score"),
        Some(&faizdb_core::document::model::Value::Integer(1500))
    );
    assert_eq!(
        updated.get("kills"),
        Some(&faizdb_core::document::model::Value::Integer(12))
    );

    // 2. Verify MongoDB updateOne with $set
    let mongo_q =
        r#"db.leaderboards.updateOne({"player_id": "player_cyber_99"}, {"$set": {"kills": 20}})"#;
    let mongo_stmt = parse_query(mongo_q).expect("Mongo updateOne should parse");
    db.execute(mongo_stmt)
        .expect("Mongo updateOne should execute");

    let final_doc = col.find_all(None).into_iter().next().unwrap();
    assert_eq!(
        final_doc.get("kills"),
        Some(&faizdb_core::document::model::Value::Integer(20))
    );
}

#[tokio::test]
async fn test_sql_and_mongo_order_by() {
    let db = DatabaseContext::new();
    let col = db.get_or_create_collection("ranks");

    for (id, val) in [("a", 10), ("b", 50), ("c", 25)] {
        let mut doc = Document::new();
        doc.set("id", id);
        doc.set("rank", val);
        col.insert(doc).unwrap();
    }

    // 1. SQL ORDER BY DESC
    let stmt_desc = parse_query("SELECT * FROM ranks ORDER BY rank DESC").unwrap();
    match db.execute(stmt_desc).unwrap() {
        QueryResult::Documents(docs) => {
            let ranks: Vec<i64> = docs
                .iter()
                .filter_map(|d| d.get("rank").and_then(|v| v.as_i64()))
                .collect();
            assert_eq!(ranks, vec![50, 25, 10]);
        }
        _ => panic!("Expected Documents"),
    }

    // 2. SQL ORDER BY ASC
    let stmt_asc = parse_query("SELECT * FROM ranks ORDER BY rank ASC").unwrap();
    match db.execute(stmt_asc).unwrap() {
        QueryResult::Documents(docs) => {
            let ranks: Vec<i64> = docs
                .iter()
                .filter_map(|d| d.get("rank").and_then(|v| v.as_i64()))
                .collect();
            assert_eq!(ranks, vec![10, 25, 50]);
        }
        _ => panic!("Expected Documents"),
    }

    // 3. Mongo .sort() ASC
    let mongo_stmt = parse_query(r#"db.ranks.find().sort({"rank": 1})"#).unwrap();
    match db.execute(mongo_stmt).unwrap() {
        QueryResult::Documents(docs) => {
            let ranks: Vec<i64> = docs
                .iter()
                .filter_map(|d| d.get("rank").and_then(|v| v.as_i64()))
                .collect();
            assert_eq!(ranks, vec![10, 25, 50]);
        }
        _ => panic!("Expected Documents"),
    }
}

#[tokio::test]
async fn test_rest_pagination() {
    let (app, _state, token) = setup_test_app();

    // Insert 5 documents
    for i in 1..=5 {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/collections/items/documents")
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from(
                json!({
                    "seq": i,
                    "name": format!("Item {i}")
                })
                .to_string(),
            ))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // Query with limit=2 and offset=1
    let paginated_req = Request::builder()
        .method("GET")
        .uri("/v1/collections/items/documents?limit=2&offset=1")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.oneshot(paginated_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body["success"].as_bool().unwrap());
    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 2, "Should return exactly 2 items for limit=2");
}

#[tokio::test]
async fn test_mongo_wire_drop_list_collections_count_and_sort() {
    let db = Arc::new(DatabaseContext::new());
    let user_store = Arc::new(UserStore::new());
    user_store
        .create_user("remediation_admin", "admin-pass-2026", Role::Admin)
        .unwrap();

    // Seed collections
    let col1 = db.get_or_create_collection("users");
    let _ = db.get_or_create_collection("orders");

    for i in 1..=3 {
        let mut d = Document::new();
        d.set("name", format!("User {i}"));
        d.set("status", if i == 1 { "active" } else { "pending" });
        d.set("level", i * 10);
        col1.insert(d).unwrap();
    }

    // Ephemeral TCP wire server
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
    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Authenticate
    let auth_res = send_op_msg(
        &mut stream,
        doc! {
            "authenticate": 1,
            "user": "remediation_admin",
            "pwd": "admin-pass-2026"
        },
    )
    .await;
    assert_eq!(auth_res.get_f64("ok").unwrap(), 1.0);

    // 1. Test listCollections with integer 1 (standard Mongo driver)
    let list_res = send_op_msg(
        &mut stream,
        doc! {
            "listCollections": 1,
            "$db": "faizdb"
        },
    )
    .await;
    assert_eq!(list_res.get_f64("ok").unwrap(), 1.0);
    let cursor = list_res.get_document("cursor").unwrap();
    let first_batch = cursor.get_array("firstBatch").unwrap();
    let col_names: Vec<&str> = first_batch
        .iter()
        .filter_map(|b| b.as_document().and_then(|d| d.get_str("name").ok()))
        .collect();
    assert!(col_names.contains(&"users"));
    assert!(col_names.contains(&"orders"));

    // 2. Test count with query filter
    let count_res = send_op_msg(
        &mut stream,
        doc! {
            "count": "users",
            "query": doc! { "status": "active" },
            "$db": "faizdb"
        },
    )
    .await;
    assert_eq!(count_res.get_f64("ok").unwrap(), 1.0);
    assert_eq!(
        count_res.get_i64("n").unwrap(),
        1,
        "Only 1 document has status=active"
    );

    // 3. Test find with sort
    let find_res = send_op_msg(
        &mut stream,
        doc! {
            "find": "users",
            "filter": doc! {},
            "sort": doc! { "level": -1 },
            "$db": "faizdb"
        },
    )
    .await;
    assert_eq!(find_res.get_f64("ok").unwrap(), 1.0);
    let cursor = find_res.get_document("cursor").unwrap();
    let batch = cursor.get_array("firstBatch").unwrap();
    let levels: Vec<i64> = batch
        .iter()
        .filter_map(|b| b.as_document().and_then(|d| d.get_i64("level").ok()))
        .collect();
    assert_eq!(levels, vec![30, 20, 10]);

    // 4. Test drop collection
    let drop_res = send_op_msg(
        &mut stream,
        doc! {
            "drop": "orders",
            "$db": "faizdb"
        },
    )
    .await;
    assert_eq!(drop_res.get_f64("ok").unwrap(), 1.0);

    // Verify orders collection is removed
    let remaining = db.list_collections();
    assert!(!remaining.contains(&"orders".to_string()));
}

#[tokio::test]
async fn test_lsm_sstable_compaction() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config = faizdb_core::storage::engine::StorageConfig {
        data_dir: temp_dir.path().to_path_buf(),
        memtable_size: 128, // Flush after very small size to force multiple SSTables
        sync_writes: false,
        enable_wal: true,
        block_cache_size: 1024 * 1024,
    };

    let engine = Arc::new(faizdb_core::storage::engine::StorageEngine::open(config).unwrap());

    // Write enough distinct keys and flush repeatedly to generate multiple SSTables
    for round in 1..=5 {
        for i in 1..=20 {
            let key = format!("k_{round}_{i}").into_bytes();
            let val = format!("val_{round}_{i}").into_bytes();
            engine.put(&key, &val).unwrap();
        }
        engine.flush().unwrap();
    }

    let stats_before = engine.stats();
    assert!(
        stats_before.sstable_count >= 1,
        "Should have created SSTables"
    );

    // Trigger compaction
    let compacted_count = engine.compact().unwrap();
    println!("Compacted {compacted_count} SSTables");

    let stats_after = engine.stats();
    // After compaction, SSTables should be merged into a single consolidated SSTable
    assert_eq!(
        stats_after.sstable_count, 1,
        "All SSTables should merge into 1 compacted SSTable"
    );

    // Verify all keys remain readable with correct values
    for round in 1..=5 {
        for i in 1..=20 {
            let key = format!("k_{round}_{i}").into_bytes();
            let expected_val = format!("val_{round}_{i}").into_bytes();
            let val = engine
                .get(&key)
                .unwrap()
                .expect("Key should exist after compaction");
            assert_eq!(val, expected_val);
        }
    }
}
