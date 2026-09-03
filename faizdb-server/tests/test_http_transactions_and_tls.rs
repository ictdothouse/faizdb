//! End-to-end integration tests for HTTP Transaction lifecycle, TLS serving, and $lookup aggregation.

use std::sync::Arc;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use faizdb_server::api::{create_router, AppState};

fn setup_test_app() -> (axum::Router, Arc<AppState>, String) {
    let db = Arc::new(faizdb_query::DatabaseContext::new());
    let auth = Arc::new(faizdb_security::auth::AuthManager::new(b"test-secret-key-1234567890123456"));
    let geo = Arc::new(faizdb_core::cluster::GeoReplicationEngine::new("test-region".to_string()));

    let token = auth.generate_token("admin_user", faizdb_security::auth::Role::Admin, 3600).unwrap();

    let state = Arc::new(AppState {
        db,
        auth,
        backup_schedule: Arc::new(parking_lot::RwLock::new(Default::default())),
        geo_replication: geo,
        metrics: Arc::new(Default::default()),
    });

    (create_router(state.clone()), state, token)
}

#[tokio::test]
async fn test_http_transaction_commit_no_deadlock() {
    let (app, state, token) = setup_test_app();

    // 1. Check health
    let req = Request::builder()
        .uri("/v1/health")
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 2. Begin transaction
    let req = Request::builder()
        .method("POST")
        .uri("/v1/transaction/begin")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let txn_id = body_json["data"]["txn_id"].as_str().unwrap().to_string();
    assert!(!txn_id.is_empty());

    // 3. Stage document write inside transaction
    let req = Request::builder()
        .method("POST")
        .uri("/v1/collections/orders/insert")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "document": {
                "_id": "order_101",
                "item": "CyberTruck Model",
                "amount": 42000.0
            },
            "txn_id": txn_id
        }).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 4. Commit transaction (Previously deadlocked permanently here!)
    let req = Request::builder()
        .method("POST")
        .uri("/v1/transaction/commit")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "txn_id": txn_id
        }).to_string()))
        .unwrap();
    let res = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        app.clone().oneshot(req)
    ).await.expect("Transaction commit timed out! Deadlock detected").unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["data"]["status"], "Committed");

    // 5. Verify server is still healthy and responsive immediately after commit
    let req = Request::builder()
        .uri("/v1/health")
        .body(Body::empty())
        .unwrap();
    let res = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        app.clone().oneshot(req)
    ).await.expect("Health check timed out after commit! Server deadlocked").unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 6. Verify committed document is visible in collection
    let col = state.db.get_or_create_collection("orders");
    let doc = col.find_by_id("order_101").expect("Committed document not found in collection!");
    assert_eq!(doc.get("item").unwrap().as_str().unwrap(), "CyberTruck Model");
}

#[tokio::test]
async fn test_http_transaction_rollback_no_deadlock() {
    let (app, state, token) = setup_test_app();

    // 1. Begin transaction
    let req = Request::builder()
        .method("POST")
        .uri("/v1/transaction/begin")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let txn_id = body_json["data"]["txn_id"].as_str().unwrap().to_string();

    // 2. Stage write
    let req = Request::builder()
        .method("POST")
        .uri("/v1/collections/orders/insert")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "document": {
                "_id": "order_cancelled",
                "item": "Cancelled Item"
            },
            "txn_id": txn_id
        }).to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    // 3. Rollback transaction
    let req = Request::builder()
        .method("POST")
        .uri("/v1/transaction/rollback")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "txn_id": txn_id
        }).to_string()))
        .unwrap();
    let res = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        app.clone().oneshot(req)
    ).await.expect("Transaction rollback timed out! Deadlock detected").unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["data"]["status"], "Aborted");

    // 4. Verify uncommitted document is NOT present in collection
    let col = state.db.get_or_create_collection("orders");
    assert!(col.find_by_id("order_cancelled").is_err());
}

#[tokio::test]
async fn test_vector_insert_into_missing_index_returns_404() {
    let (app, _state, token) = setup_test_app();

    let req = Request::builder()
        .method("POST")
        .uri("/v1/vector/insert")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "index_name": "phantom_index_does_not_exist",
            "id": "v1",
            "vector": [0.1, 0.2, 0.3, 0.4]
        }).to_string()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body_json["error"].as_str().unwrap().contains("does not exist"));
}

#[tokio::test]
async fn test_rest_aggregation_lookup_pipeline() {
    let (app, state, token) = setup_test_app();

    // 1. Populate customers collection
    let cust_col = state.db.get_or_create_collection("customers");
    let mut c1 = faizdb_core::Document::new();
    c1.set("_id", "cust_1");
    c1.set("name", "Faiz");
    let _ = cust_col.insert(c1);

    // 2. Populate orders collection
    let order_col = state.db.get_or_create_collection("orders");
    let mut o1 = faizdb_core::Document::new();
    o1.set("_id", "ord_101");
    o1.set("customer_id", "cust_1");
    o1.set("item", "Neuralink DevKit");
    let _ = order_col.insert(o1);

    // 3. Execute $lookup aggregation on customers
    let req = Request::builder()
        .method("POST")
        .uri("/v1/collections/customers/aggregate")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "pipeline": [
                {
                    "$lookup": {
                        "from": "orders",
                        "localField": "_id",
                        "foreignField": "customer_id",
                        "as": "customer_orders"
                    }
                }
            ]
        }).to_string()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    let results = body_json["data"].as_array().unwrap();
    assert_eq!(results.len(), 1);
    let orders = results[0]["customer_orders"].as_array().unwrap();
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0]["item"], "Neuralink DevKit");
}

#[tokio::test]
async fn test_tls_rustls_config_and_server_binding() {
    let _ = rustls::crypto::ring::default_provider().install_default();

    let (certs, key) = faizdb_security::generate_self_signed_cert(&["localhost".into(), "127.0.0.1".into()]).unwrap();
    let certs_der: Vec<Vec<u8>> = certs.into_iter().map(|c| c.to_vec()).collect();
    let key_der = match key {
        rustls_pki_types::PrivateKeyDer::Pkcs8(p) => p.secret_pkcs8_der().to_vec(),
        rustls_pki_types::PrivateKeyDer::Pkcs1(p) => p.secret_pkcs1_der().to_vec(),
        rustls_pki_types::PrivateKeyDer::Sec1(p) => p.secret_sec1_der().to_vec(),
        _ => Vec::new(),
    };

    let rustls_config = axum_server::tls_rustls::RustlsConfig::from_der(certs_der, key_der)
        .await
        .expect("Failed to build RustlsConfig from DER");

    // Bind to loopback ephemeral port 127.0.0.1:0
    let socket_addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
    let handle = axum_server::Handle::new();
    let (app, _state, _token) = setup_test_app();

    let server_handle = tokio::spawn(async move {
        axum_server::bind_rustls(socket_addr, rustls_config)
            .handle(handle)
            .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await
    });

    // Let the server spin up for a moment
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(!server_handle.is_finished());

    // Abort server task cleanly
    server_handle.abort();
}
