//! Cluster, Raft RPC, and Geo-Replication handlers.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use faizdb_core::cluster::{AppendEntriesArgs, RequestVoteArgs};

use super::{ApiResponse, AppState};

#[derive(Debug, Deserialize)]
pub struct JoinClusterRequest {
    pub peer_id: String,
    pub peer_address: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRegionRequest {
    pub region_id: String,
    pub endpoint: String,
}

#[derive(Debug, Deserialize)]
pub struct GeoSyncRequest {
    pub deltas: Vec<faizdb_core::cluster::ReplicationDelta>,
}

/// GET /v1/cluster/status
pub async fn cluster_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let node_info = state.db.raft().get_info();
    let shard_dist = state.db.shards().get_distribution();
    Json(ApiResponse::ok(serde_json::json!({
        "node": node_info,
        "shards": shard_dist,
        "consensus": "Raft v1.0",
        "virtual_slots": 16384,
    })))
}

/// POST /v1/cluster/join
pub async fn cluster_join(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<JoinClusterRequest>,
) -> impl IntoResponse {
    state.db.raft().add_peer(payload.peer_id.clone(), payload.peer_address.clone());
    state.db.shards().register_node(payload.peer_id.clone(), payload.peer_address.clone());
    Json(ApiResponse::ok(serde_json::json!({
        "message": format!("Peer '{}' joined cluster", payload.peer_id),
        "peer_id": payload.peer_id,
        "peer_address": payload.peer_address,
    })))
}

/// POST /v1/cluster/failover — trigger Raft leader election
pub async fn cluster_trigger_failover(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.db.raft().trigger_election();
    let info = state.db.raft().get_info();
    Json(ApiResponse::ok(serde_json::json!({
        "message": "Election triggered. Node promoted to new Leader.",
        "new_term": info.term,
        "is_leader": info.is_leader,
    })))
}

/// POST /v1/cluster/raft/vote — Raft RequestVote RPC
pub async fn raft_request_vote(
    State(state): State<Arc<AppState>>,
    Json(args): Json<RequestVoteArgs>,
) -> impl IntoResponse {
    Json(state.db.raft().handle_request_vote(args))
}

/// POST /v1/cluster/raft/append — Raft AppendEntries RPC
pub async fn raft_append_entries(
    State(state): State<Arc<AppState>>,
    Json(args): Json<AppendEntriesArgs>,
) -> impl IntoResponse {
    Json(state.db.raft().handle_append_entries(args))
}

/// GET /v1/cluster/regions
pub async fn cluster_get_regions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let peers = state.geo_replication.list_peers();
    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "local_region": state.geo_replication.local_region,
        "peer_count": peers.len(),
        "regions": peers,
    }))))
}

/// POST /v1/cluster/regions
pub async fn cluster_register_region(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterRegionRequest>,
) -> impl IntoResponse {
    state.geo_replication.register_peer(&payload.region_id, &payload.endpoint);
    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "message": format!("Region '{}' registered", payload.region_id),
        "region_id": payload.region_id,
        "endpoint": payload.endpoint,
    }))))
}

/// POST /v1/cluster/geo-sync
pub async fn cluster_geo_sync(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GeoSyncRequest>,
) -> impl IntoResponse {
    let applied = payload.deltas.into_iter()
        .filter(|delta| state.geo_replication.apply_remote_delta(delta.clone()))
        .count();
    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "applied_deltas": applied,
        "version_vector": state.geo_replication.version_vector.read().clone(),
    }))))
}
