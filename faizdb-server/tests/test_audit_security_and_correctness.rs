//! Comprehensive Regression Test Suite for Audit Findings
//!
//! Validates:
//! 1. PostgreSQL Wire Protocol table queries (e.g. `SELECT * FROM users`) are never intercepted
//!    by `current_user` scalar introspection.
//! 2. PostgreSQL Wire Protocol scalar introspections (`SELECT current_user`, `SELECT version()`, `SELECT 1`)
//!    continue to function correctly when no table is queried.
//! 3. Vector Search REST API enforces preflight dimension, empty query, and top_k checks (returns 400 without panic).
//! 4. Query Cost-Based Optimizer (CBO) handles documents with NaN floats gracefully without panicking.

use std::sync::Arc;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use faizdb_core::document::model::{Document, Value as FaizValue};
use faizdb_query::{DatabaseContext, optimizer::TableStatistics};
use faizdb_server::api::{create_router, AppState};
use faizdb_server::wire::postgres::handler::handle_postgres_query;

fn setup_test_app() -> (axum::Router, Arc<AppState>, String) {
    let db = Arc::new(DatabaseContext::new());
    let auth = Arc::new(faizdb_security::auth::AuthManager::new(b"test-secret-key-1234567890123456"));
    let user_store = Arc::new(faizdb_security::UserStore::new());
    let geo = Arc::new(faizdb_core::cluster::GeoReplicationEngine::new("test-region".to_string()));

    let token = auth.generate_token("admin", faizdb_security::auth::Role::Admin, 3600).unwrap();

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
async fn test_postgres_wire_select_from_users_table_not_intercepted() {
    let db = Arc::new(DatabaseContext::new());
    let mut in_txn = false;

    // 1. Populate the "users" collection with actual user records
    let col = db.get_or_create_collection("users");
    let mut doc1 = Document::new();
    doc1.set("name", FaizValue::String("Siti Nurhaliza".to_string()));
    doc1.set("role", FaizValue::String("artist".to_string()));
    col.insert(doc1).unwrap();

    let mut doc2 = Document::new();
    doc2.set("name", FaizValue::String("Faiz Aziz".to_string()));
    doc2.set("role", FaizValue::String("architect".to_string()));
    col.insert(doc2).unwrap();

    // 2. Querying "SELECT * FROM users" MUST NOT be intercepted as CURRENT_USER scalar!
    let resp_bytes = handle_postgres_query(&db, "SELECT * FROM users", &mut in_txn);
    let resp_str = String::from_utf8_lossy(&resp_bytes);

    // The response must contain the actual table data, NOT "current_user" or "postgres" as the sole column
    assert!(resp_str.contains("Siti Nurhaliza"), "Expected table query to return user Siti Nurhaliza");
    assert!(resp_str.contains("Faiz Aziz"), "Expected table query to return user Faiz Aziz");
    assert!(resp_str.contains("artist"), "Expected table query to return artist role");
    assert!(!resp_str.contains("current_user\0"), "Table query must not be intercepted as current_user introspection!");

    // 3. True scalar introspections WITHOUT FROM must still succeed
    let cur_user_bytes = handle_postgres_query(&db, "SELECT CURRENT_USER", &mut in_txn);
    let cur_user_str = String::from_utf8_lossy(&cur_user_bytes);
    assert!(cur_user_str.contains("current_user"));
    assert!(cur_user_str.contains("postgres"));

    let user_func_bytes = handle_postgres_query(&db, "SELECT USER()", &mut in_txn);
    let user_func_str = String::from_utf8_lossy(&user_func_bytes);
    assert!(user_func_str.contains("current_user"));
    assert!(user_func_str.contains("postgres"));

    let ver_bytes = handle_postgres_query(&db, "SELECT VERSION()", &mut in_txn);
    let ver_str = String::from_utf8_lossy(&ver_bytes);
    assert!(ver_str.contains("FaizDB"));

    let ping_bytes = handle_postgres_query(&db, "SELECT 1", &mut in_txn);
    let ping_str = String::from_utf8_lossy(&ping_bytes);
    assert!(ping_str.contains("?column?"));
}

#[tokio::test]
async fn test_vector_search_rest_preflight_validation_and_no_panic() {
    let (app, _state, token) = setup_test_app();

    // 1. Create a 4-dimensional vector index
    let create_req = Request::builder()
        .method("POST")
        .uri("/v1/vector/index")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "name": "audit_embeddings",
            "dimensions": 4,
            "metric": "cosine"
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 2. Insert valid 4D vector
    let insert_req = Request::builder()
        .method("POST")
        .uri("/v1/vector/insert")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "index_name": "audit_embeddings",
            "id": "vec_1",
            "vector": [1.0, 0.0, 0.0, 0.0]
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(insert_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 3. Search with mismatched dimension (2 dimensions instead of 4)
    // MUST return 400 Bad Request instead of panicking!
    let search_mismatch = Request::builder()
        .method("POST")
        .uri("/v1/vector/search")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "index_name": "audit_embeddings",
            "query": [1.0, 0.0],
            "top_k": 5
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(search_mismatch).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body_json["success"], false);
    assert!(body_json["error"].as_str().unwrap().contains("dimension mismatch"));

    // 4. Search with empty vector
    let search_empty = Request::builder()
        .method("POST")
        .uri("/v1/vector/search")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "index_name": "audit_embeddings",
            "query": [],
            "top_k": 5
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(search_empty).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 5. Search with top_k = 0
    let search_zero_k = Request::builder()
        .method("POST")
        .uri("/v1/vector/search")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({
            "index_name": "audit_embeddings",
            "query": [1.0, 0.0, 0.0, 0.0],
            "top_k": 0
        }).to_string()))
        .unwrap();

    let resp = app.clone().oneshot(search_zero_k).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[test]
fn test_cbo_optimizer_graceful_nan_handling() {
    let mut docs = Vec::new();

    // Document with valid floats
    let mut d1 = Document::new();
    d1.set("price", FaizValue::Float(19.99));
    docs.push(d1);

    // Document with NaN
    let mut d2 = Document::new();
    d2.set("price", FaizValue::Float(f64::NAN));
    docs.push(d2);

    // Document with another valid float
    let mut d3 = Document::new();
    d3.set("price", FaizValue::Float(49.99));
    docs.push(d3);

    // Running analyze on documents containing NaN MUST NOT panic!
    let stats = TableStatistics::analyze("products_with_nan", &docs);
    assert_eq!(stats.total_documents, 3);
    assert!(stats.column_stats.contains_key("price"));
}
