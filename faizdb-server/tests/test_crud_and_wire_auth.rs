//! Integration tests for REST PUT/PATCH CRUD, User Management REST API,
//! and PostgreSQL Wire Protocol authentication enforcement.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tower::ServiceExt;

use faizdb_server::api::{create_router, AppState};

fn setup_test_app() -> (axum::Router, Arc<AppState>, String) {
    let db = Arc::new(faizdb_query::DatabaseContext::new());
    let auth = Arc::new(faizdb_security::auth::AuthManager::new(
        b"test-secret-key-1234567890123456",
    ));
    let user_store = Arc::new(faizdb_security::UserStore::new());
    let geo = Arc::new(faizdb_core::cluster::GeoReplicationEngine::new(
        "test-region".to_string(),
    ));

    let token = auth
        .generate_token("admin", faizdb_security::auth::Role::Admin, 3600)
        .unwrap();

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
async fn test_rest_put_document_replacement() {
    let (app, _state, token) = setup_test_app();

    // 1. Insert initial document
    let insert_req = Request::builder()
        .method("POST")
        .uri("/v1/collections/products/documents")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "document": {
                    "_id": "p100",
                    "title": "Old Laptop",
                    "price": 500,
                    "category": "electronics"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(insert_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 2. PUT full replacement
    let put_req = Request::builder()
        .method("PUT")
        .uri("/v1/collections/products/documents/p100")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "title": "Brand New Laptop 2026",
                "price": 1200
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(put_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body_json["success"], true);
    assert_eq!(body_json["data"]["_id"], "p100");
    assert_eq!(body_json["data"]["title"], "Brand New Laptop 2026");
    assert_eq!(body_json["data"]["price"], 1200);
    // Old category should be gone
    assert!(body_json["data"].get("category").is_none());
}

#[tokio::test]
async fn test_rest_patch_document_partial_and_operators() {
    let (app, _state, token) = setup_test_app();

    // 1. Insert document
    let insert_req = Request::builder()
        .method("POST")
        .uri("/v1/collections/inventory/documents")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "document": {
                    "_id": "item1",
                    "name": "Widget",
                    "stock": 10,
                    "deprecated_tag": "v1"
                }
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(insert_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 2. PATCH using MongoDB-style $set, $inc, and $unset operators
    let patch_req = Request::builder()
        .method("PATCH")
        .uri("/v1/collections/inventory/documents/item1")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "$set": { "supplier": "Global Supplies" },
                "$inc": { "stock": 5 },
                "$unset": { "deprecated_tag": "" }
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(patch_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body_json["success"], true);
    assert_eq!(body_json["data"]["name"], "Widget");
    assert_eq!(body_json["data"]["stock"], 15.0);
    assert_eq!(body_json["data"]["supplier"], "Global Supplies");
    assert!(body_json["data"].get("deprecated_tag").is_none());
}

#[tokio::test]
async fn test_user_management_api_flow() {
    let (app, state, token) = setup_test_app();

    // 1. List users (should have admin)
    let list_req = Request::builder()
        .method("GET")
        .uri("/v1/users")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(list_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body_json: Value = serde_json::from_slice(&body).unwrap();
    assert!(body_json["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|u| u["username"] == "admin"));

    // 2. Create new ReadWrite user
    let create_req = Request::builder()
        .method("POST")
        .uri("/v1/users")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": "developer",
                "password": "dev-password-2026",
                "role": "readwrite"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(create_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // Verify developer can authenticate
    assert_eq!(
        state
            .user_store
            .authenticate("developer", "dev-password-2026"),
        Some(faizdb_security::Role::ReadWrite)
    );

    // 3. Update developer password
    let update_req = Request::builder()
        .method("PUT")
        .uri("/v1/users/developer/password")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "password": "new-dev-password-999"
            })
            .to_string(),
        ))
        .unwrap();

    let resp = app.clone().oneshot(update_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert_eq!(
        state
            .user_store
            .authenticate("developer", "new-dev-password-999"),
        Some(faizdb_security::Role::ReadWrite)
    );

    // 4. Delete developer user
    let delete_req = Request::builder()
        .method("DELETE")
        .uri("/v1/users/developer")
        .header("Authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let resp = app.clone().oneshot(delete_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    assert_eq!(
        state
            .user_store
            .authenticate("developer", "new-dev-password-999"),
        None
    );
}

#[tokio::test]
async fn test_postgres_wire_authentication_success_and_failure() {
    let db = Arc::new(faizdb_query::DatabaseContext::new());
    let user_store = Arc::new(faizdb_security::UserStore::new());
    user_store
        .create_user("analyst", "mypassword123", faizdb_security::Role::ReadWrite)
        .unwrap();

    // Bind to ephemeral port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let pg_addr = addr.to_string();
    let db_clone = db.clone();
    let store_clone = user_store.clone();

    tokio::spawn(async move {
        let _ = faizdb_server::wire::postgres::run_postgres_server(&pg_addr, db_clone, store_clone)
            .await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Helper to build startup packet for a username
    fn build_startup_packet(user: &str) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&196608i32.to_be_bytes()); // Protocol v3
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

    // --- Scenario A: Failed authentication (wrong password) ---
    {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(&build_startup_packet("analyst"))
            .await
            .unwrap();

        // Server should reply with AuthenticationCleartextPassword ('R', len 8, code 3)
        let mut auth_req = [0u8; 9];
        stream.read_exact(&mut auth_req).await.unwrap();
        assert_eq!(auth_req[0], b'R');
        assert_eq!(
            i32::from_be_bytes([auth_req[5], auth_req[6], auth_req[7], auth_req[8]]),
            3
        );

        // Send incorrect password message ('p' + len + pass\0)
        let wrong_pass = b"wrong_secret\0";
        let p_len = (4 + wrong_pass.len()) as i32;
        let mut p_msg = vec![b'p'];
        p_msg.extend_from_slice(&p_len.to_be_bytes());
        p_msg.extend_from_slice(wrong_pass);
        stream.write_all(&p_msg).await.unwrap();

        // Server should return ErrorResponse ('E')
        let mut err_head = [0u8; 5];
        stream.read_exact(&mut err_head).await.unwrap();
        assert_eq!(err_head[0], b'E');
    }

    // --- Scenario B: Successful authentication (correct password) ---
    {
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(&build_startup_packet("analyst"))
            .await
            .unwrap();

        let mut auth_req = [0u8; 9];
        stream.read_exact(&mut auth_req).await.unwrap();
        assert_eq!(auth_req[0], b'R');
        assert_eq!(
            i32::from_be_bytes([auth_req[5], auth_req[6], auth_req[7], auth_req[8]]),
            3
        );

        // Send correct password
        let correct_pass = b"mypassword123\0";
        let p_len = (4 + correct_pass.len()) as i32;
        let mut p_msg = vec![b'p'];
        p_msg.extend_from_slice(&p_len.to_be_bytes());
        p_msg.extend_from_slice(correct_pass);
        stream.write_all(&p_msg).await.unwrap();

        // Server should return AuthenticationOk ('R', len 8, code 0)
        let mut auth_ok = [0u8; 9];
        stream.read_exact(&mut auth_ok).await.unwrap();
        assert_eq!(auth_ok[0], b'R');
        assert_eq!(
            i32::from_be_bytes([auth_ok[5], auth_ok[6], auth_ok[7], auth_ok[8]]),
            0
        );
    }
}
