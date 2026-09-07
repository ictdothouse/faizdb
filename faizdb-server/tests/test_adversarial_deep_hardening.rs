//! Comprehensive Integration Tests for Deep Adversarial Hardening
//!
//! Verifies:
//! 1. SQL DDL & DML resilience with `IF NOT EXISTS`, `IF EXISTS`, and quoted identifiers (`"table"`, `` `table` ``).
//! 2. Graph edge persistence & tombstone deletion across query execution.
//! 3. Offline TTL expiration: expired documents are never revived upon restart.
//! 4. MySQL wire protocol string/UUID column type mapping.
//! 5. REST vector deletion (`DELETE /v1/vector/{index}/{id}`) and index dropping (`DELETE /v1/vector/index/{name}`).

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use faizdb_core::document::model::Document;
use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use faizdb_query::{parse_query, DatabaseContext, QueryResult};
use faizdb_server::api::{create_router, AppState};
use faizdb_server::wire::mysql::codec::MYSQL_TYPE_VAR_STRING;
use faizdb_server::wire::mysql::handler::handle_mysql_query;
use serde_json::json;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

#[test]
fn test_sql_ddl_if_not_exists_and_identifier_quotes() {
    let dir = tempdir().unwrap();
    let ctx = DatabaseContext::with_storage_dir(dir.path()).unwrap();

    // 1. CREATE TABLE IF NOT EXISTS with backticks
    let stmt = parse_query("CREATE TABLE IF NOT EXISTS `users` (id text, name text);").unwrap();
    let create_res = ctx.execute(stmt).unwrap();
    match create_res {
        QueryResult::Success(msg) => assert!(msg.contains("users")),
        _ => panic!("Expected success for CREATE TABLE"),
    }

    // 2. INSERT INTO with double quotes
    let insert_stmt = parse_query("INSERT INTO \"users\" (id, name) VALUES ('u1', 'Alice');").unwrap();
    ctx.execute(insert_stmt).unwrap();

    // 3. SELECT FROM with backticks
    let select_stmt = parse_query("SELECT * FROM `users` WHERE id = 'u1';").unwrap();
    let select_res = ctx.execute(select_stmt).unwrap();
    match select_res {
        QueryResult::Documents(docs) => {
            assert_eq!(docs.len(), 1);
            assert_eq!(docs[0].get("name").unwrap().as_str().unwrap(), "Alice");
        }
        _ => panic!("Expected documents"),
    }

    // 4. DROP TABLE IF EXISTS with backticks
    let drop_stmt = parse_query("DROP TABLE IF EXISTS `users`;").unwrap();
    let drop_res = ctx.execute(drop_stmt).unwrap();
    match drop_res {
        QueryResult::Success(msg) => assert!(msg.contains("users")),
        _ => panic!("Expected success for DROP TABLE"),
    }
}

#[test]
fn test_graph_edge_query_durability() {
    let dir = tempdir().unwrap();
    let ctx = DatabaseContext::with_storage_dir(dir.path()).unwrap();

    // Create edge via FaizQL query
    let create_edge_stmt = parse_query(
        "CREATE EDGE from 'alice' to 'bob' VIA 'COLLABORATES' WEIGHT 3.0;",
    )
    .unwrap();
    ctx.execute(create_edge_stmt).unwrap();

    // Verify edge is stored in storage engine under graph:e:
    let storage = ctx.storage().unwrap();
    let edge_key = b"graph:e:alice:bob:COLLABORATES";
    let stored_bytes = storage.get(edge_key).unwrap();
    assert!(stored_bytes.is_some(), "Graph edge must be persisted to disk storage");

    // Delete edge via FaizQL query
    let del_edge_stmt = parse_query(
        "DELETE EDGE from 'alice' to 'bob' VIA 'COLLABORATES';",
    )
    .unwrap();
    ctx.execute(del_edge_stmt).unwrap();

    // Verify edge tombstone / deletion in storage engine
    let deleted_bytes = storage.get(edge_key).unwrap();
    assert!(deleted_bytes.is_none(), "Graph edge must be deleted from disk storage");
}

#[test]
fn test_offline_expired_ttl_purged_on_reboot() {
    let dir = tempdir().unwrap();
    let storage_cfg = StorageConfig {
        data_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(StorageEngine::open(storage_cfg).unwrap());

    let col_cfg = faizdb_core::document::collection::CollectionConfig {
        name: "sessions".into(),
        ..Default::default()
    };
    let col = faizdb_core::document::collection::Collection::with_config_and_storage(
        col_cfg,
        storage.clone(),
    );

    // Create a document with 10-second TTL that was created 2 hours ago
    let mut doc = Document::new().field("user", "bob").field("_ttl", 10);
    if let Some(ref mut meta) = doc.metadata {
        meta.created_at = Utc::now() - Duration::hours(2);
    }
    let doc_id = doc.id.as_str().to_string();
    let doc_key = format!("doc:sessions:{}", doc_id).into_bytes();

    // Manually persist to storage as if it was written 2 hours ago
    storage
        .put(&doc_key, &serde_json::to_vec(&doc).unwrap())
        .unwrap();

    // Load document during recovery
    col.load_document(doc);

    // Verify that the document was purged because its TTL expired while offline
    assert!(
        col.find_by_id(&doc_id).is_err(),
        "Offline-expired document must not be active"
    );
    assert!(
        storage.get(&doc_key).unwrap().is_none(),
        "Offline-expired document must be purged from disk storage"
    );
}

#[test]
fn test_mysql_string_and_uuid_id_column_type() {
    let db = Arc::new(DatabaseContext::new());
    db.get_or_create_collection("articles");
    let insert_stmt = parse_query("INSERT INTO articles (id, title) VALUES ('art_001', 'Rust Guide');").unwrap();
    let _ = db.execute(insert_stmt);

    let res = handle_mysql_query(&db, "faizdb", "SELECT id, title FROM articles", 1);
    // MySQL Resultset: count packet, 2 column def packets, eof, row, eof
    assert!(res.len() >= 5);

    // Column def for 'id' is index 1
    let id_col_pkt = &res[1];
    // ColumnDefinition41: skip 4-byte packet header, then skip 6 lenenc strings
    let mut payload = &id_col_pkt[4..];
    for _ in 0..6 {
        let len = payload[0] as usize;
        payload = &payload[1 + len..];
    }
    assert_eq!(payload[0], 0x0C, "Fixed fields length byte must be 0x0C");
    // Offset 1 (length byte) + 2 (charset) + 4 (column length) = 7
    let col_type = payload[7];
    assert_eq!(
        col_type, MYSQL_TYPE_VAR_STRING,
        "String/UUID ID must be typed as VAR_STRING (0xFD) so MySQL drivers parse it without error"
    );
}

#[test]
fn test_sql_comments_and_tautology_filters_and_set() {
    let dir = tempdir().unwrap();
    let ctx = DatabaseContext::with_storage_dir(dir.path()).unwrap();

    // 1. SET session variable
    let set_stmt = parse_query("SET NAMES 'utf8mb4';").unwrap();
    let set_res = ctx.execute(set_stmt).unwrap();
    match set_res {
        QueryResult::Success(msg) => assert!(msg.contains("NAMES")),
        _ => panic!("Expected success for SET"),
    }

    // 2. CREATE TABLE with SQL line comments
    let create_sql = "-- Initialize users table\nCREATE TABLE `members` (id text, status text);";
    let stmt = parse_query(create_sql).unwrap();
    ctx.execute(stmt).unwrap();

    // 3. INSERT with backticks around column names and block comment
    let insert_sql = "/* bulk insert */ INSERT INTO `members` (`id`, `status`) VALUES ('m1', 'active');";
    let insert_stmt = parse_query(insert_sql).unwrap();
    ctx.execute(insert_stmt).unwrap();

    // 4. SELECT with WHERE 1 = 1 AND status = 'active'
    let select_sql = "SELECT * FROM `members` WHERE 1 = 1 AND `status` = 'active';";
    let select_stmt = parse_query(select_sql).unwrap();
    let res = ctx.execute(select_stmt).unwrap();
    match res {
        QueryResult::Documents(docs) => {
            assert_eq!(docs.len(), 1);
            assert_eq!(docs[0].get("status").unwrap().as_str().unwrap(), "active");
        }
        _ => panic!("Expected document matching WHERE 1 = 1"),
    }

    // 5. Unconditional DELETE FROM members; (without WHERE)
    let del_sql = "/* clean table */ DELETE FROM `members`;";
    let del_stmt = parse_query(del_sql).unwrap();
    let del_res = ctx.execute(del_stmt).unwrap();
    match del_res {
        QueryResult::Deleted(n) => assert_eq!(n, 1),
        _ => panic!("Expected 1 deleted document"),
    }

    // 6. Verify table is empty
    let count_stmt = parse_query("COUNT FROM `members`;").unwrap();
    match ctx.execute(count_stmt).unwrap() {
        QueryResult::Count(n) => assert_eq!(n, 0),
        _ => panic!("Expected count 0"),
    }
}

#[tokio::test]
async fn test_rest_vector_delete_and_drop_index() {
    let db = Arc::new(DatabaseContext::new());
    let auth = Arc::new(faizdb_security::auth::AuthManager::new(
        b"test-secret-key-1234567890123456",
    ));
    let geo = Arc::new(faizdb_core::cluster::GeoReplicationEngine::new(
        "test-region".to_string(),
    ));

    let token = auth
        .generate_token("admin_user", faizdb_security::auth::Role::Admin, 3600)
        .unwrap();

    let state = Arc::new(AppState {
        db,
        auth,
        user_store: Arc::new(faizdb_security::UserStore::new()),
        backup_schedule: Arc::new(parking_lot::RwLock::new(Default::default())),
        geo_replication: geo,
        metrics: Arc::new(Default::default()),
    });

    let app = create_router(state);

    // 1. Create vector index
    let create_req = Request::builder()
        .method("POST")
        .uri("/v1/vector/index")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "name": "embeddings",
            "dimensions": 4,
            "metric": "Cosine"
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 2. Insert a vector
    let insert_req = Request::builder()
        .method("POST")
        .uri("/v1/vector/insert")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "index_name": "embeddings",
            "id": "doc_1",
            "vector": [0.1, 0.2, 0.3, 0.4]
        }).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(insert_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. Delete the vector via DELETE /v1/vector/{index_name}/{id}
    let del_req = Request::builder()
        .method("DELETE")
        .uri("/v1/vector/embeddings/doc_1")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(del_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 4. Drop the vector index via DELETE /v1/vector/index/{name}
    let drop_req = Request::builder()
        .method("DELETE")
        .uri("/v1/vector/index/embeddings")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(drop_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
