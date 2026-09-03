//! Knowledge Graph REST API handlers supporting vertices, edges, BFS traversal, and shortest path.

use std::sync::Arc;
use axum::{
    extract::{Path, Query, Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Deserialize;

use faizdb_core::document::model::Document;
use faizdb_graph::{Edge, Vertex};
use crate::api::{ApiResponse, AppState};

#[derive(Debug, Deserialize)]
pub struct CreateVertexRequest {
    pub id: String,
    pub label: String,
    pub properties: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEdgeRequest {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub weight: Option<f32>,
    pub properties: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct TraverseQuery {
    pub start: String,
    pub depth: Option<usize>,
    pub relation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ShortestPathQuery {
    pub start: String,
    pub target: String,
}

pub async fn create_vertex(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateVertexRequest>,
) -> impl IntoResponse {
    let doc = payload.properties
        .and_then(Document::from_json_value)
        .unwrap_or_default();

    let vertex = Vertex::with_properties(payload.id.clone(), payload.label.clone(), doc);
    state.db.graph_store().write().add_vertex(vertex.clone());

    if let Some(storage) = state.db.storage() {
        let key = format!("graph:v:{}", payload.id);
        if let Ok(val) = serde_json::to_vec(&vertex) {
            let _ = storage.put(key.as_bytes(), &val);
        }
    }

    (StatusCode::CREATED, Json(ApiResponse::ok(serde_json::json!({
        "id": payload.id,
        "label": payload.label,
        "status": "Created",
    }))))
}

pub async fn get_vertex(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let store = state.db.graph_store();
    let graph = store.read();
    match graph.get_vertex(&id) {
        Some(v) => {
            let props = serde_json::to_value(&v.properties).unwrap_or(serde_json::Value::Null);
            (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                "id": v.id,
                "label": v.label,
                "properties": props,
            }))))
        }
        None => (StatusCode::NOT_FOUND, Json(ApiResponse::err(format!("Vertex '{id}' not found")))),
    }
}

pub async fn create_edge(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateEdgeRequest>,
) -> impl IntoResponse {
    let doc = payload.properties
        .and_then(Document::from_json_value)
        .unwrap_or_default();

    let mut edge = Edge::with_weight(
        payload.from.clone(),
        payload.to.clone(),
        payload.relation.clone(),
        payload.weight.unwrap_or(1.0),
    );
    edge.properties = doc;
    state.db.graph_store().write().add_edge(edge.clone());

    if let Some(storage) = state.db.storage() {
        let key = format!("graph:e:{}:{}:{}", payload.from, payload.to, payload.relation);
        if let Ok(val) = serde_json::to_vec(&edge) {
            let _ = storage.put(key.as_bytes(), &val);
        }
    }

    (StatusCode::CREATED, Json(ApiResponse::ok(serde_json::json!({
        "from": payload.from,
        "to": payload.to,
        "relation": payload.relation,
        "status": "Created",
    }))))
}

pub async fn traverse_graph(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TraverseQuery>,
) -> impl IntoResponse {
    let depth = query.depth.unwrap_or(3);
    let store = state.db.graph_store();
    let graph = store.read();
    let paths = graph.traverse_bfs(&query.start, depth, query.relation.as_deref());

    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "start": query.start,
        "depth": depth,
        "visited_count": paths.len(),
        "paths": paths,
    }))))
}

pub async fn shortest_path(
    State(state): State<Arc<AppState>>,
    Query(query): Query<ShortestPathQuery>,
) -> impl IntoResponse {
    let store = state.db.graph_store();
    let graph = store.read();
    match graph.shortest_path(&query.start, &query.target) {
        Some(path) => (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
            "start": query.start,
            "target": query.target,
            "hop_count": path.len().saturating_sub(1),
            "path": path,
        })))),
        None => (StatusCode::NOT_FOUND, Json(ApiResponse::err("No path found between vertices"))),
    }
}

pub async fn graph_stats(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let store = state.db.graph_store();
    let graph = store.read();
    Json(ApiResponse::ok(serde_json::json!({
        "vertices": graph.vertex_count(),
        "edges": graph.edge_count(),
    })))
}
