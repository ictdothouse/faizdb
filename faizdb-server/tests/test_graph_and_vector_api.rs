//! Integration tests for Vector search and Knowledge Graph REST endpoints.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde_json::{json, Value};
use tower::ServiceExt;

use faizdb_server::api::{create_router, AppState};

fn setup_test_app() -> (axum::Router, String) {
    let db = std::sync::Arc::new(faizdb_query::DatabaseContext::new());
    let auth = std::sync::Arc::new(faizdb_security::auth::AuthManager::new(b"test-secret-key-1234567890123456"));
    let geo = std::sync::Arc::new(faizdb_core::cluster::GeoReplicationEngine::new("test-region".to_string()));

    let token = auth.generate_token("admin_user", faizdb_security::auth::Role::Admin, 3600).unwrap();

    let state = std::sync::Arc::new(AppState {
        db,
        auth,
        backup_schedule: std::sync::Arc::new(parking_lot::RwLock::new(Default::default())),
        geo_replication: geo,
        metrics: std::sync::Arc::new(Default::default()),
    });

    (create_router(state), token)
}

#[tokio::test]
async fn test_vector_rest_api_lifecycle() {
    let (app, token) = setup_test_app();

    // 1. Create vector index
    let req = Request::builder()
        .method("POST")
        .uri("/v1/vector/index")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "name": "embeddings",
            "dimensions": 4,
            "metric": "cosine"
        }).to_string()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // 2. Insert vectors
    for (id, vec) in [
        ("doc_1", vec![1.0, 0.0, 0.0, 0.0]),
        ("doc_2", vec![0.0, 1.0, 0.0, 0.0]),
        ("doc_3", vec![0.9, 0.1, 0.0, 0.0]),
    ] {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/vector/insert")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({
                "index_name": "embeddings",
                "id": id,
                "vector": vec
            }).to_string()))
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    // 3. Search vector
    let req = Request::builder()
        .method("POST")
        .uri("/v1/vector/search")
        .header("authorization", format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({
            "index_name": "embeddings",
            "query": [1.0, 0.0, 0.0, 0.0],
            "top_k": 2
        }).to_string()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body_json["success"].as_bool().unwrap());
    let results = body_json["data"]["results"].as_array().unwrap();
    assert_eq!(results.len(), 2);
    // Closest match must be doc_1 (exact match, distance ~ 0)
    assert_eq!(results[0]["id"].as_str().unwrap(), "doc_1");
}

#[tokio::test]
async fn test_graph_rest_api_lifecycle() {
    let (app, token) = setup_test_app();

    // 1. Create vertices
    for (id, label) in [("alice", "Person"), ("bob", "Person"), ("charlie", "Person")] {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/graph/vertices")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({
                "id": id,
                "label": label
            }).to_string()))
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    // 2. Create edges
    for (from, to, rel) in [("alice", "bob", "KNOWS"), ("bob", "charlie", "MANAGES")] {
        let req = Request::builder()
            .method("POST")
            .uri("/v1/graph/edges")
            .header("authorization", format!("Bearer {token}"))
            .header("content-type", "application/json")
            .body(Body::from(json!({
                "from": from,
                "to": to,
                "relation": rel
            }).to_string()))
            .unwrap();

        let res = app.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
    }

    // 3. Shortest path alice -> charlie
    let req = Request::builder()
        .method("GET")
        .uri("/v1/graph/shortest_path?start=alice&target=charlie")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body_json["success"].as_bool().unwrap());
    assert_eq!(body_json["data"]["hop_count"].as_u64().unwrap(), 2);
    let path = body_json["data"]["path"].as_array().unwrap();
    let expected = json!(["alice", "bob", "charlie"]);
    assert_eq!(path, expected.as_array().unwrap());

    // 4. Graph stats
    let req = Request::builder()
        .method("GET")
        .uri("/v1/graph/stats")
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body_bytes = axum::body::to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let body_json: Value = serde_json::from_slice(&body_bytes).unwrap();
    assert_eq!(body_json["data"]["vertices"].as_u64().unwrap(), 3);
    assert_eq!(body_json["data"]["edges"].as_u64().unwrap(), 2);
}
