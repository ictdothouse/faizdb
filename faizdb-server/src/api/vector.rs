//! Vector Search REST API handlers powered by HNSW index.

use std::sync::Arc;
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};

use faizdb_vector::{HnswConfig, HnswIndex, DistanceMetric, QuantizationType};
use crate::api::{ApiResponse, AppState};

#[derive(Debug, Deserialize)]
pub struct CreateVectorIndexRequest {
    pub name: String,
    pub dimensions: usize,
    pub metric: Option<String>,
    pub max_m: Option<usize>,
    pub ef_construction: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct InsertVectorRequest {
    pub index_name: String,
    pub id: String,
    pub vector: Vec<f32>,
}

#[derive(Debug, Deserialize)]
pub struct SearchVectorRequest {
    pub index_name: String,
    pub query: Vec<f32>,
    pub top_k: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct VectorSearchResultItem {
    pub id: String,
    pub distance: f32,
    pub score: f32,
}

pub async fn create_vector_index(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateVectorIndexRequest>,
) -> impl IntoResponse {
    let metric = match payload.metric.as_deref() {
        Some("euclidean") | Some("l2") => DistanceMetric::Euclidean,
        Some("dot") | Some("dot_product") => DistanceMetric::DotProduct,
        Some("manhattan") => DistanceMetric::Manhattan,
        _ => DistanceMetric::Cosine,
    };

    let m = payload.max_m.unwrap_or(16);
    let config = HnswConfig {
        dimensions: payload.dimensions,
        metric,
        quantization: QuantizationType::None,
        m,
        m0: m * 2,
        ef_construction: payload.ef_construction.unwrap_or(100),
        ef_search: 64,
        ml: 1.0 / (m as f64).ln(),
    };

    let index = Arc::new(parking_lot::RwLock::new(HnswIndex::new(config.clone())));
    state.db.vector_indexes().insert(payload.name.clone(), index);

    if let Some(storage) = state.db.storage() {
        let key = format!("vec:meta:{}", payload.name);
        if let Ok(val) = serde_json::to_vec(&config) {
            let _ = storage.put(key.as_bytes(), &val);
        }
    }

    (StatusCode::CREATED, Json(ApiResponse::ok(serde_json::json!({
        "index_name": payload.name,
        "dimensions": payload.dimensions,
        "metric": format!("{:?}", metric),
        "status": "Ready",
    }))))
}

pub async fn insert_vector(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InsertVectorRequest>,
) -> impl IntoResponse {
    let index_lock = match state.db.vector_indexes().get(&payload.index_name) {
        Some(idx) => idx.clone(),
        None => {
            // Auto-create index if missing using the vector's dimension
            let config = HnswConfig {
                dimensions: payload.vector.len(),
                metric: DistanceMetric::Cosine,
                ..Default::default()
            };
            if let Some(storage) = state.db.storage() {
                let key = format!("vec:meta:{}", payload.index_name);
                if let Ok(val) = serde_json::to_vec(&config) {
                    let _ = storage.put(key.as_bytes(), &val);
                }
            }
            let new_idx = Arc::new(parking_lot::RwLock::new(HnswIndex::new(config)));
            state.db.vector_indexes().insert(payload.index_name.clone(), new_idx.clone());
            new_idx
        }
    };

    let mut index = index_lock.write();
    let total_nodes = index.len() + 1;
    match index.insert(payload.id.clone(), payload.vector.clone()) {
        Ok(_) => {
            if let Some(storage) = state.db.storage() {
                let key = format!("vec:data:{}:{}", payload.index_name, payload.id);
                if let Ok(val) = serde_json::to_vec(&payload.vector) {
                    let _ = storage.put(key.as_bytes(), &val);
                }
            }
            (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                "id": payload.id,
                "index_name": payload.index_name,
                "total_nodes": total_nodes,
            }))))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(format!("Failed to insert vector: {e}")))),
    }
}

pub async fn search_vector(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SearchVectorRequest>,
) -> impl IntoResponse {
    let index_lock = match state.db.vector_indexes().get(&payload.index_name) {
        Some(idx) => idx.clone(),
        None => return (StatusCode::NOT_FOUND, Json(ApiResponse::err(format!("Vector index '{}' not found", payload.index_name)))),
    };

    let top_k = payload.top_k.unwrap_or(10);
    let index = index_lock.read();
    let results = index.search(&payload.query, top_k);

    let mapped: Vec<VectorSearchResultItem> = results
        .into_iter()
        .map(|r| {
            let score = 1.0 / (1.0 + r.distance);
            VectorSearchResultItem {
                id: r.id,
                distance: r.distance,
                score,
            }
        })
        .collect();

    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "index_name": payload.index_name,
        "query_dimensions": payload.query.len(),
        "top_k": top_k,
        "results": mapped,
    }))))
}

pub async fn list_vector_indexes(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let indexes: Vec<serde_json::Value> = state
        .db
        .vector_indexes()
        .iter()
        .map(|entry| {
            let idx = entry.value().read();
            serde_json::json!({
                "name": entry.key(),
                "dimensions": idx.config.dimensions,
                "metric": format!("{:?}", idx.config.metric),
                "total_vectors": idx.len(),
            })
        })
        .collect();

    Json(ApiResponse::ok(indexes))
}
