//! Health, metrics, info, and audit log handlers.

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;

use super::{ApiResponse, AppState};

pub async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "engine": "FaizDB",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

/// GET /v1/health/liveness — Kubernetes liveness probe (checks process vitality)
pub async fn liveness_check() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "alive",
            "engine": "FaizDB",
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
}

/// GET /v1/health/readiness — Kubernetes readiness probe (checks storage, Raft, collections)
pub async fn readiness_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let collections = state.db.list_collections();
    let raft_info = state.db.raft().get_info();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ready",
            "engine": "FaizDB",
            "storage_initialized": true,
            "collections_count": collections.len(),
            "raft_role": raft_info.role,
            "raft_term": raft_info.term,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })),
    )
}

pub async fn server_info() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "FaizDB Server",
        "version": env!("CARGO_PKG_VERSION"),
        "creator": "Ahmad Faiz",
        "features": [
            "document", "vector_hnsw", "knowledge_graph", "acid_transactions", "faizql",
            "change_streams", "websockets"
        ],
        "experimental_features": [
            "raft_consensus_clustering", "auto_sharding"
        ],
        "consensus_mode": "single_process_raft_verified",
    }))
}

static SERVER_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// GET /v1/metrics — Prometheus-compatible text exposition
pub async fn metrics_handler() -> impl IntoResponse {
    let uptime = SERVER_START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs();
    let blocked_ips = super::middleware::get_blocklist().len();
    let tracked_ips = super::middleware::get_rate_limiter().len();
    let version = env!("CARGO_PKG_VERSION");

    let metrics = format!(
        "# HELP faizdb_uptime_seconds Total server uptime in seconds\n\
         # TYPE faizdb_uptime_seconds counter\n\
         faizdb_uptime_seconds {uptime}\n\
         \n\
         # HELP faizdb_blocked_ips_total IPs currently on the permanent blocklist\n\
         # TYPE faizdb_blocked_ips_total gauge\n\
         faizdb_blocked_ips_total {blocked_ips}\n\
         \n\
         # HELP faizdb_rate_tracked_ips IPs currently tracked by rate limiter\n\
         # TYPE faizdb_rate_tracked_ips gauge\n\
         faizdb_rate_tracked_ips {tracked_ips}\n\
         \n\
         # HELP faizdb_build_info Static build information\n\
         # TYPE faizdb_build_info gauge\n\
         faizdb_build_info{{version=\"{version}\",engine=\"FaizDB\"}} 1\n",
    );

    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        metrics,
    )
}

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    pub limit: Option<usize>,
}

/// GET /v1/audit/logs — tail the JSON-Lines audit log (admin only)
pub async fn get_audit_logs(Query(params): Query<AuditLogQuery>) -> impl IntoResponse {
    let log_path =
        std::env::var("FAIZDB_AUDIT_LOG").unwrap_or_else(|_| "./logs/audit.jsonl".to_string());
    let path = std::path::Path::new(&log_path);
    if !path.exists() {
        return Json(ApiResponse::ok(Vec::<serde_json::Value>::new()));
    }
    let content = match tokio::fs::read_to_string(path).await {
        Ok(c) => c,
        Err(e) => return Json(ApiResponse::err(format!("Failed to read audit log: {e}"))),
    };
    let limit = params.limit.unwrap_or(100);
    let mut logs: Vec<serde_json::Value> = content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    logs.reverse();
    logs.truncate(limit);
    Json(ApiResponse::ok(logs))
}
