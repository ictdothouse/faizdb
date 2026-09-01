//! REST API handlers and Router for FaizDB Server.

use std::sync::Arc;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use faizdb_core::document::model::Document;
use faizdb_core::stream::ChangeEvent;
use faizdb_query::{parse_query, DatabaseContext};

/// Shared server state
pub struct AppState {
    pub db: Arc<DatabaseContext>,
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

/// Create the Axum HTTP router with REST and WebSocket Change Streams
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(health_check))
        .route("/v1/info", get(server_info))
        .route("/v1/query", post(execute_query))
        .route("/v1/collections/{name}/insert", post(insert_document))
        .route("/v1/collections/{name}/stats", get(collection_stats))
        .route("/v1/subscribe", get(ws_global_subscribe))
        .route("/v1/collections/{name}/watch", get(ws_collection_watch))
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
        "features": ["document", "vector", "graph", "acid", "faizql", "change_streams", "websockets"]
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
    let doc_clone = doc.clone();
    match col.insert(doc) {
        Ok(id) => {
            state.db.change_stream_bus().publish(ChangeEvent::insert(&name, doc_clone));
            (
                StatusCode::CREATED,
                Json(ApiResponse::ok(serde_json::json!({ "id": id.as_str() }))),
            )
        }
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

/// WebSocket Change Stream: `/v1/subscribe`
async fn ws_global_subscribe(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = state.db.clone();
    ws.on_upgrade(move |socket| handle_change_stream_socket(socket, db, None))
}

/// WebSocket Collection Watch: `/v1/collections/{name}/watch`
async fn ws_collection_watch(
    ws: WebSocketUpgrade,
    Path(collection_name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = state.db.clone();
    ws.on_upgrade(move |socket| handle_change_stream_socket(socket, db, Some(collection_name)))
}

async fn handle_change_stream_socket(
    socket: WebSocket,
    db: Arc<DatabaseContext>,
    target_collection: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = db.change_stream_bus().subscribe();
    let col_filter = target_collection.unwrap_or_else(|| "*".to_string());

    info!(
        "WebSocket client connected to Change Stream for '{}' (Total subscribers: {})",
        col_filter,
        db.change_stream_bus().subscriber_count()
    );

    // Initial greeting
    let welcome = serde_json::json!({
        "status": "connected",
        "stream": "faizdb-change-streams-v1",
        "collection": col_filter,
        "active_subscribers": db.change_stream_bus().subscriber_count(),
        "timestamp": chrono::Utc::now()
    });
    if let Ok(msg_str) = serde_json::to_string(&welcome) {
        let _ = sender.send(Message::Text(msg_str.into())).await;
    }

    let filter_for_task = col_filter.clone();
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if filter_for_task == "*" || filter_for_task == event.collection {
                if let Ok(json) = serde_json::to_string(&event) {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Close(_) = msg {
                break;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };

    debug!("WebSocket client disconnected from Change Stream");
}
