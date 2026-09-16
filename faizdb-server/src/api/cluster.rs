//! Cluster, Raft RPC, and Geo-Replication handlers.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
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
    state
        .db
        .raft()
        .add_peer(payload.peer_id.clone(), payload.peer_address.clone());
    state
        .db
        .shards()
        .register_node(payload.peer_id.clone(), payload.peer_address.clone());
    Json(ApiResponse::ok(serde_json::json!({
        "message": format!("Peer '{}' joined cluster", payload.peer_id),
        "peer_id": payload.peer_id,
        "peer_address": payload.peer_address,
    })))
}

async fn send_raft_rpc<Req: serde::Serialize, Resp: serde::de::DeserializeOwned>(
    peer_addr: &str,
    endpoint: &str,
    args: &Req,
) -> Option<Resp> {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let target = peer_addr
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    let host_port = target.split('/').next().unwrap_or(target);

    let stream_res = tokio::time::timeout(
        Duration::from_millis(500),
        tokio::net::TcpStream::connect(host_port),
    )
    .await;
    let mut stream = match stream_res {
        Ok(Ok(s)) => s,
        _ => return None,
    };

    let body = serde_json::to_string(args).ok()?;
    let req = format!(
        "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        endpoint,
        host_port,
        body.len(),
        body
    );

    if stream.write_all(req.as_bytes()).await.is_err() {
        return None;
    }
    let _ = stream.flush().await;

    let mut resp = Vec::new();
    let _ = tokio::time::timeout(Duration::from_millis(1000), stream.read_to_end(&mut resp)).await;
    let resp_str = String::from_utf8_lossy(&resp);
    if let Some(body_start) = resp_str.find("\r\n\r\n") {
        let json_part = &resp_str[body_start + 4..];
        serde_json::from_str(json_part).ok()
    } else {
        None
    }
}

/// Concrete HTTP Transport for Raft consensus RPC communication across nodes
#[derive(Debug, Clone, Default)]
pub struct HttpRaftTransport;

impl HttpRaftTransport {
    pub fn new() -> Self {
        Self
    }

    pub async fn send_vote(
        &self,
        peer_addr: &str,
        args: &RequestVoteArgs,
    ) -> Option<faizdb_core::cluster::RequestVoteReply> {
        send_raft_rpc(peer_addr, "/v1/cluster/raft/vote", args).await
    }

    pub async fn send_append(
        &self,
        peer_addr: &str,
        args: &AppendEntriesArgs,
    ) -> Option<faizdb_core::cluster::AppendEntriesReply> {
        send_raft_rpc(peer_addr, "/v1/cluster/raft/append", args).await
    }

    pub async fn send_snapshot(
        &self,
        peer_addr: &str,
        args: &faizdb_core::cluster::InstallSnapshotArgs,
    ) -> Option<faizdb_core::cluster::InstallSnapshotReply> {
        send_raft_rpc(peer_addr, "/v1/cluster/raft/snapshot", args).await
    }
}

/// POST /v1/cluster/failover — trigger active Raft leader election with real peer RPC dispatch
pub async fn cluster_trigger_failover(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let transport = HttpRaftTransport::new();
    let (term, vote_args) = state.db.raft().start_election();
    let peers = state.db.raft().list_peers();

    let mut votes_granted = 1; // Vote for self
    for (peer_id, peer_addr) in &peers {
        if let Some(reply) = transport.send_vote(peer_addr, &vote_args).await {
            if reply.vote_granted && state.db.raft().record_vote(peer_id, term, true) {
                votes_granted += 1;
            }
        }
    }

    // If single-node cluster, 1 vote is already majority (1/1)
    if peers.is_empty() {
        state.db.raft().trigger_election();
    }

    let info = state.db.raft().get_info();
    Json(ApiResponse::ok(serde_json::json!({
        "message": if info.is_leader {
            format!("Election won with {votes_granted} vote(s). Node promoted to Leader.")
        } else {
            format!("Election initiated for term {term}. Votes received: {votes_granted}/{}", info.quorum_size)
        },
        "term": info.term,
        "is_leader": info.is_leader,
        "quorum_size": info.quorum_size,
        "votes_received": votes_granted,
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

/// POST /v1/cluster/raft/snapshot — Raft InstallSnapshot RPC
pub async fn raft_install_snapshot(
    State(state): State<Arc<AppState>>,
    Json(args): Json<faizdb_core::cluster::InstallSnapshotArgs>,
) -> impl IntoResponse {
    Json(state.db.raft().handle_install_snapshot(args))
}

/// Spawn the Raft consensus background tick daemon driving elections and leader heartbeats
pub fn spawn_raft_tick_daemon(
    raft: Arc<faizdb_core::cluster::RaftNode>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(50));
        let transport = HttpRaftTransport::new();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    let action = raft.tick();
                    match action {
                        faizdb_core::cluster::RaftTickAction::None => {}
                        faizdb_core::cluster::RaftTickAction::SendHeartbeat => {
                            let heartbeats = raft.prepare_heartbeats();
                            for (_peer_id, (peer_addr, args)) in heartbeats {
                                let raft_clone = raft.clone();
                                let transport_clone = transport.clone();
                                tokio::spawn(async move {
                                    if let Some(reply) = transport_clone.send_append(&peer_addr, &args).await {
                                        if reply.term > args.term {
                                            raft_clone.step_down(reply.term);
                                        }
                                    }
                                });
                            }
                        }
                        faizdb_core::cluster::RaftTickAction::StartElection => {
                            let (term, vote_args) = raft.start_election();
                            let peers = raft.list_peers();
                            if peers.is_empty() {
                                raft.trigger_election();
                            } else {
                                for (peer_id, peer_addr) in peers {
                                    let raft_clone = raft.clone();
                                    let transport_clone = transport.clone();
                                    let vote_args_clone = vote_args.clone();
                                    tokio::spawn(async move {
                                        if let Some(reply) = transport_clone.send_vote(&peer_addr, &vote_args_clone).await {
                                            if reply.vote_granted {
                                                raft_clone.record_vote(&peer_id, term, true);
                                            }
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    tracing::info!("Raft background tick daemon shutting down");
                    break;
                }
            }
        }
    })
}

/// GET /v1/cluster/regions
pub async fn cluster_get_regions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let peers = state.geo_replication.list_peers();
    (
        StatusCode::OK,
        Json(ApiResponse::ok(serde_json::json!({
            "local_region": state.geo_replication.local_region,
            "peer_count": peers.len(),
            "regions": peers,
        }))),
    )
}

/// POST /v1/cluster/regions
pub async fn cluster_register_region(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RegisterRegionRequest>,
) -> impl IntoResponse {
    state
        .geo_replication
        .register_peer(&payload.region_id, &payload.endpoint);
    (
        StatusCode::OK,
        Json(ApiResponse::ok(serde_json::json!({
            "message": format!("Region '{}' registered", payload.region_id),
            "region_id": payload.region_id,
            "endpoint": payload.endpoint,
        }))),
    )
}

/// POST /v1/cluster/geo-sync
pub async fn cluster_geo_sync(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GeoSyncRequest>,
) -> impl IntoResponse {
    let applied = payload
        .deltas
        .into_iter()
        .filter(|delta| state.geo_replication.apply_remote_delta(delta.clone()))
        .count();
    (
        StatusCode::OK,
        Json(ApiResponse::ok(serde_json::json!({
            "applied_deltas": applied,
            "version_vector": state.geo_replication.version_vector.read().clone(),
        }))),
    )
}
