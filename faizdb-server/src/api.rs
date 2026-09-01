//! REST API handlers and Router for FaizDB Server.

use std::sync::Arc;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use faizdb_core::document::model::Document;
use faizdb_query::{parse_query, DatabaseContext};

/// Shared server state
pub struct AppState {
    pub db: DatabaseContext,
}

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Create the Axum HTTP router
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(health_check))
        .route("/v1/info", get(server_info))
        .route("/v1/query", post(execute_query))
        .route("/v1/collections/:name/insert", post(insert_document))
        .route("/v1/collections/:name/stats", get(collection_stats))
        .with_state(state)
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "engine": "FaizDB",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn server_info() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "FaizDB Server",
        "version": env!("CARGO_PKG_VERSION"),
        "creator": "Ahmad Faiz",
        "features": ["document", "vector", "graph", "acid", "faizql"]
    }))
}

async fn execute_query(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueryRequest>,
) -> impl IntoResponse {
    match parse_query(&payload.query) {
        Ok(stmt) => match state.db.execute(stmt) {
            Ok(result) => (StatusCode::OK, Json(ApiResponse::ok(result))),
            Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(e))),
        },
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(format!("Parse error: {e}")))),
    }
}

async fn insert_document(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(doc_val): Json<serde_json::Value>,
) -> impl IntoResponse {
    let doc = match Document::from_json_value(doc_val) {
        Some(d) => d,
        None => return (StatusCode::BAD_REQUEST, Json(ApiResponse::err("Expected JSON object"))),
    };

    let col = state.db.get_or_create_collection(&name);
    match col.insert(doc) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(ApiResponse::ok(serde_json::json!({ "id": id.as_str() }))),
        ),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e.to_string()))),
    }
}

async fn collection_stats(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let stats = col.stats();
    Json(ApiResponse::ok(serde_json::json!({
        "name": col.name(),
        "document_count": stats.document_count,
        "total_size": stats.total_size,
        "avg_document_size": stats.avg_document_size,
        "index_count": stats.index_count
    })))
}
