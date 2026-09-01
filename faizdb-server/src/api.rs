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

use faizdb_core::cluster::{AppendEntriesArgs, RequestVoteArgs};
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

#[derive(Debug, Deserialize)]
pub struct JoinClusterRequest {
    pub peer_id: String,
    pub peer_address: String,
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

/// Create the Axum HTTP router with REST, WebSocket Change Streams & Cluster RPC
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/v1/health", get(health_check))
        .route("/v1/info", get(server_info))
        .route("/v1/query", post(execute_query))
        .route("/v1/collections/{name}/insert", post(insert_document))
        .route("/v1/collections/{name}/stats", get(collection_stats))
        .route("/v1/collections/{name}/aggregate", post(aggregate_collection))
        .route("/v1/collections/{name}/search", post(search_collection))
        .route("/v1/collections/{name}/ttl/stats", get(collection_ttl_stats))
        .route("/v1/collections/{name}/ttl/purge", post(collection_ttl_purge))
        .route("/v1/subscribe", get(ws_global_subscribe))
        .route("/v1/collections/{name}/watch", get(ws_collection_watch))
        // Cluster & Raft Endpoints
        .route("/v1/cluster/status", get(cluster_status))
        .route("/v1/cluster/join", post(cluster_join))
        .route("/v1/cluster/failover", post(cluster_trigger_failover))
        .route("/v1/cluster/raft/vote", post(raft_request_vote))
        .route("/v1/cluster/raft/append", post(raft_append_entries))
        // Backup & Disaster Recovery Endpoints
        .route("/v1/backup/create", post(backup_create))
        .route("/v1/backup/list", get(backup_list))
        .route("/v1/backup/restore", post(backup_restore))
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
        "features": ["document", "vector", "graph", "acid", "faizql", "change_streams", "websockets", "raft_clustering", "auto_sharding"]
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

#[derive(Debug, Deserialize)]
pub struct AggregateRequest {
    pub pipeline: serde_json::Value,
}

async fn aggregate_collection(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<AggregateRequest>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let all_docs = col.find_all(None);

    match faizdb_query::parse_pipeline(&payload.pipeline) {
        Ok(stages) => {
            let results = faizdb_query::execute_pipeline(all_docs, &stages);
            (StatusCode::OK, Json(ApiResponse::ok(results)))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(format!("Aggregation error: {e}")))),
    }
}

#[derive(Debug, Deserialize)]
pub struct FullTextSearchRequest {
    pub query: String,
    #[serde(default)]
    pub fuzzy: bool,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    10
}

async fn search_collection(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<FullTextSearchRequest>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let results = col.search_text(&payload.query, payload.fuzzy, payload.top_k);

    let output: Vec<serde_json::Value> = results
        .into_iter()
        .map(|(doc, score, matched_terms)| {
            let mut val = serde_json::to_value(&doc.fields).unwrap_or(serde_json::Value::Null);
            if let Some(obj) = val.as_object_mut() {
                obj.insert("_id".to_string(), serde_json::Value::String(doc.id.as_str().to_string()));
                obj.insert("_score".to_string(), serde_json::json!(score));
                obj.insert("_matched_terms".to_string(), serde_json::json!(matched_terms));
            }
            val
        })
        .collect();

    Json(ApiResponse::ok(output))
}

async fn collection_ttl_stats(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let stats = col.ttl_stats();
    Json(ApiResponse::ok(stats))
}

async fn collection_ttl_purge(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let purged_ids = col.purge_expired();
    Json(ApiResponse::ok(serde_json::json!({
        "purged_count": purged_ids.len(),
        "purged_ids": purged_ids,
    })))
}

#[derive(Debug, Deserialize)]
pub struct RestoreBackupRequest {
    pub filename: Option<String>,
}

/// Create a new atomic consistent snapshot
async fn backup_create(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let collections = state.db.all_collections();
    let mut data = Vec::new();
    for (name, col) in collections {
        let docs = col.find_all(None);
        data.push((name, docs));
    }

    let archive = faizdb_core::backup::build_snapshot(&data);
    let filename = format!("faizdb_snapshot_{}.json", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let path = std::path::PathBuf::from("./backups").join(&filename);

    match faizdb_core::backup::save_snapshot_file(&archive, &path) {
        Ok(_) => (StatusCode::CREATED, Json(ApiResponse::ok(archive.manifest))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e))),
    }
}

/// List all available snapshot files
async fn backup_list() -> impl IntoResponse {
    let backup_dir = std::path::Path::new("./backups");
    if !backup_dir.exists() {
        return Json(ApiResponse::ok(Vec::<faizdb_core::backup::SnapshotManifest>::new()));
    }

    let mut manifests = Vec::new();
    if let Ok(entries) = std::fs::read_dir(backup_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(archive) = faizdb_core::backup::load_and_verify_snapshot(&path) {
                    manifests.push(archive.manifest);
                }
            }
        }
    }

    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(ApiResponse::ok(manifests))
}

/// Restore database from snapshot
async fn backup_restore(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RestoreBackupRequest>,
) -> impl IntoResponse {
    let backup_dir = std::path::Path::new("./backups");
    let target_file = match payload.filename {
        Some(name) => backup_dir.join(name),
        None => {
            // Find latest
            let mut latest: Option<(std::path::PathBuf, String)> = None;
            if let Ok(entries) = std::fs::read_dir(backup_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        let name = path.to_string_lossy().to_string();
                        if latest.as_ref().map_or(true, |l| name > l.1) {
                            latest = Some((path, name));
                        }
                    }
                }
            }
            match latest {
                Some((p, _)) => p,
                None => return (StatusCode::NOT_FOUND, Json(ApiResponse::err("No backup snapshots found to restore"))),
            }
        }
    };

    match faizdb_core::backup::load_and_verify_snapshot(&target_file) {
        Ok(archive) => {
            let mut restored_count = 0;
            for (col_name, doc_vals) in archive.collections_data {
                let col = state.db.get_or_create_collection(&col_name);
                for val in doc_vals {
                    if let Some(doc) = faizdb_core::document::model::Document::from_json_value(val) {
                        if col.insert(doc).is_ok() {
                            restored_count += 1;
                        }
                    }
                }
            }
            (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                "message": "Database snapshot successfully verified and restored",
                "checksum": archive.manifest.checksum,
                "restored_documents": restored_count,
            }))))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(format!("Restore verification failed: {e}")))),
    }
}

/// Cluster Status Handler: `/v1/cluster/status`
async fn cluster_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let node_info = state.db.raft().get_info();
    let shard_dist = state.db.shards().get_distribution();
    Json(ApiResponse::ok(serde_json::json!({
        "node": node_info,
        "shards": shard_dist,
        "consensus": "Raft v1.0",
        "virtual_slots": 16384,
    })))
}

/// Dynamic Cluster Join Handler: `/v1/cluster/join`
async fn cluster_join(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<JoinClusterRequest>,
) -> impl IntoResponse {
    state.db.raft().add_peer(payload.peer_id.clone(), payload.peer_address.clone());
    state.db.shards().register_node(payload.peer_id.clone(), payload.peer_address.clone());
    Json(ApiResponse::ok(serde_json::json!({
        "message": format!("Peer '{}' joined cluster successfully", payload.peer_id),
        "peer_id": payload.peer_id,
        "peer_address": payload.peer_address,
    })))
}

/// Simulate Failover Handler: `/v1/cluster/failover`
async fn cluster_trigger_failover(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.db.raft().trigger_election();
    let info = state.db.raft().get_info();
    Json(ApiResponse::ok(serde_json::json!({
        "message": "Election timeout triggered. Node promoted to new Leader",
        "new_term": info.term,
        "is_leader": info.is_leader,
    })))
}

/// Raft RequestVote RPC Handler: `/v1/cluster/raft/vote`
async fn raft_request_vote(
    State(state): State<Arc<AppState>>,
    Json(args): Json<RequestVoteArgs>,
) -> impl IntoResponse {
    let reply = state.db.raft().handle_request_vote(args);
    Json(reply)
}

/// Raft AppendEntries RPC Handler: `/v1/cluster/raft/append`
async fn raft_append_entries(
    State(state): State<Arc<AppState>>,
    Json(args): Json<AppendEntriesArgs>,
) -> impl IntoResponse {
    let reply = state.db.raft().handle_append_entries(args);
    Json(reply)
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
