//! Shared types, middleware, and helpers for the FaizDB REST API.
//!
//! This module is the assembly point for all API submodules. Each route
//! group lives in its own file, keeping responsibility boundaries clear.

pub mod auth;
pub mod backup;
pub mod cluster;
pub mod collections;
pub mod graph;
pub mod health;
pub mod metrics;
pub mod middleware;
pub mod vector;
pub mod websocket;

use std::sync::Arc;
use std::time::Duration;

use axum::{
    http::StatusCode,
    middleware as axum_middleware,
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;

pub use middleware::AppState;

/// Unified JSON API response envelope
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
        Self { success: true, data: Some(data), error: None }
    }
    pub fn err(message: impl Into<String>) -> Self {
        Self { success: false, data: None, error: Some(message.into()) }
    }
}

/// Injected into request extensions after JWT validation — available to all handlers.
#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub username: String,
    pub role: faizdb_security::auth::Role,
}

/// Backup schedule config (shared between AppState and admin handlers)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupScheduleConfig {
    pub enabled: bool,
    pub frequency_minutes: u64,
    pub retention_days: u32,
    pub passphrase: Option<String>,
    pub last_run: Option<String>,
}

impl Default for BackupScheduleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            frequency_minutes: 1440,
            retention_days: 7,
            passphrase: None,
            last_run: None,
        }
    }
}

/// Assemble the full Axum Router with all route groups and middleware layers.
pub fn create_router(state: Arc<AppState>) -> Router {
    // Public — no auth required
    let public_routes = Router::new()
        .route("/v1/health", get(health::health_check))
        .route("/v1/info", get(health::server_info))
        .route("/v1/auth/login", post(auth::auth_login))
        .route("/metrics", get(metrics::metrics_handler))
        .route("/v1/metrics", get(metrics::metrics_handler))
        .route("/v1/system/profile", get(metrics::system_profile_handler));

    // Read-only — requires any valid JWT
    let read_routes = Router::new()
        .route("/v1/collections/{name}/documents", get(collections::get_collection_documents))
        .route("/v1/collections/{name}/stats", get(collections::collection_stats))
        .route("/v1/collections/{name}/indexes", get(collections::get_collection_indexes))
        .route("/v1/collections/{name}/search", post(collections::search_collection))
        .route("/v1/collections/{name}/ttl/stats", get(collections::collection_ttl_stats))
        .route("/v1/subscribe", get(websocket::ws_global_subscribe))
        .route("/v1/collections/{name}/watch", get(websocket::ws_collection_watch))
        .route("/v1/backup/list", get(backup::backup_list))
        .route("/v1/auth/whoami", get(auth::auth_whoami))
        .route("/v1/vector/indexes", get(vector::list_vector_indexes))
        .route("/v1/graph/vertices/{id}", get(graph::get_vertex))
        .route("/v1/graph/traverse", get(graph::traverse_graph))
        .route("/v1/graph/shortest_path", get(graph::shortest_path))
        .route("/v1/graph/stats", get(graph::graph_stats))
        .layer(axum_middleware::from_fn_with_state(state.clone(), middleware::client_auth_middleware));

    // Write — requires Admin or ReadWrite
    let write_routes = Router::new()
        .route("/v1/query", post(collections::execute_query))
        .route("/v1/collections/{name}/documents", post(collections::insert_document))
        .route(
            "/v1/collections/{name}/documents/{id}",
            delete(collections::delete_document)
                .put(collections::update_document_put)
                .patch(collections::update_document_patch),
        )
        .route("/v1/collections/{name}/indexes", post(collections::create_collection_index))
        .route("/v1/collections/{name}/indexes/{field}", delete(collections::drop_collection_index))
        .route("/v1/collections/{name}/insert", post(collections::insert_document))
        .route("/v1/collections/{name}/import", post(collections::import_collection_data))
        .route("/v1/collections/{name}/aggregate", post(collections::aggregate_collection))
        .route("/v1/collections/{name}/ttl/purge", post(collections::collection_ttl_purge))
        .route("/v1/transaction/begin", post(collections::transaction_begin))
        .route("/v1/transaction/commit", post(collections::transaction_commit))
        .route("/v1/transaction/rollback", post(collections::transaction_rollback))
        .route("/v1/vector/index", post(vector::create_vector_index))
        .route("/v1/vector/insert", post(vector::insert_vector))
        .route("/v1/vector/search", post(vector::search_vector))
        .route("/v1/graph/vertices", post(graph::create_vertex))
        .route("/v1/graph/edges", post(graph::create_edge))
        .route("/v1/backup/create", post(backup::backup_create))
        .layer(axum_middleware::from_fn(middleware::rbac_write_middleware))
        .layer(axum_middleware::from_fn_with_state(state.clone(), middleware::client_auth_middleware));

    // Admin-only — strictly requires Admin role
    let admin_routes = Router::new()
        .route("/v1/audit/logs", get(health::get_audit_logs))
        .route("/v1/auth/token", post(auth::generate_token_handler))
        .route("/v1/backup/restore", post(backup::backup_restore))
        .route("/v1/backup/schedule", get(backup::get_backup_schedule).post(backup::update_backup_schedule))
        .route("/v1/users", get(auth::list_users).post(auth::create_user))
        .route("/v1/users/{username}", delete(auth::delete_user))
        .route("/v1/users/{username}/password", put(auth::update_user_password))
        .layer(axum_middleware::from_fn(middleware::rbac_admin_middleware))
        .layer(axum_middleware::from_fn_with_state(state.clone(), middleware::client_auth_middleware));

    // Cluster RPC — validated by cluster token
    let cluster_routes = Router::new()
        .route("/v1/cluster/status", get(cluster::cluster_status))
        .route("/v1/cluster/join", post(cluster::cluster_join))
        .route("/v1/cluster/failover", post(cluster::cluster_trigger_failover))
        .route("/v1/cluster/raft/vote", post(cluster::raft_request_vote))
        .route("/v1/cluster/raft/append", post(cluster::raft_append_entries))
        .route("/v1/cluster/regions", get(cluster::cluster_get_regions).post(cluster::cluster_register_region))
        .route("/v1/cluster/geo-sync", post(cluster::cluster_geo_sync))
        .layer(axum_middleware::from_fn(middleware::cluster_auth_middleware));

    Router::new()
        .merge(public_routes)
        .merge(read_routes)
        .merge(write_routes)
        .merge(admin_routes)
        .merge(cluster_routes)
        .layer(
            ServiceBuilder::new()
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(
                        std::env::var("FAIZDB_REQUEST_TIMEOUT_SECS")
                            .ok().and_then(|v| v.parse().ok()).unwrap_or(30),
                    ),
                ))
                .layer(axum_middleware::from_fn_with_state(state.clone(), middleware::trace_middleware))
                .layer(axum_middleware::from_fn(middleware::request_id_middleware))
                .layer(axum_middleware::from_fn(middleware::audit_middleware))
                .layer(axum_middleware::from_fn(middleware::rate_limit_middleware))
                .layer(axum_middleware::from_fn(middleware::payload_size_middleware))
                .layer(axum_middleware::from_fn(middleware::cors_middleware)),
        )
        .with_state(state)
}
