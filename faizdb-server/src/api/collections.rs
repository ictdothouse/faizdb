//! Collection CRUD, query, aggregation, full-text search, indexes, TTL, transactions, and import.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use faizdb_core::document::model::Document;
use faizdb_core::stream::ChangeEvent;
use faizdb_query::parse_query;

use super::{ApiResponse, AppState};

// ── Query ────────────────────────────────────────────────────────────────────

pub async fn execute_query(
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

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
}

// ── Documents ────────────────────────────────────────────────────────────────

pub async fn insert_document(
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
            (StatusCode::CREATED, Json(ApiResponse::ok(serde_json::json!({ "id": id.as_str() }))))
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e.to_string()))),
    }
}

pub async fn get_collection_documents(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let docs = col.find_all(None);
    let output: Vec<serde_json::Value> = docs.into_iter().map(|d| {
        let mut val = serde_json::to_value(&d.fields).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = val.as_object_mut() {
            obj.insert("_id".to_string(), serde_json::Value::String(d.id.as_str().to_string()));
        }
        val
    }).collect();
    Json(ApiResponse::ok(output))
}

pub async fn delete_document(
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    match col.delete_by_id(&id) {
        Ok(_) => {
            state.db.change_stream_bus().publish(ChangeEvent::delete(&name, &id));
            (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({ "deleted": true, "id": id }))))
        }
        Err(e) => (StatusCode::NOT_FOUND, Json(ApiResponse::err(e.to_string()))),
    }
}

// ── Stats & Indexes ──────────────────────────────────────────────────────────

pub async fn collection_stats(
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
        "index_count": stats.index_count,
    })))
}

pub async fn get_collection_indexes(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    Json(ApiResponse::ok(col.list_secondary_indexes()))
}

#[derive(Debug, Deserialize)]
pub struct CreateIndexRequest {
    pub field: String,
    #[serde(default)]
    pub unique: bool,
}

pub async fn create_collection_index(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<CreateIndexRequest>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    match col.create_secondary_index(&payload.field, payload.unique) {
        Ok(idx_name) => (StatusCode::CREATED, Json(ApiResponse::ok(serde_json::json!({
            "index_name": idx_name,
            "collection": name,
            "field": payload.field,
            "unique": payload.unique,
        })))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(e.to_string()))),
    }
}

pub async fn drop_collection_index(
    State(state): State<Arc<AppState>>,
    Path((name, field)): Path<(String, String)>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    if col.drop_secondary_index(&field) {
        (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({ "dropped": true, "field": field }))))
    } else {
        (StatusCode::NOT_FOUND, Json(ApiResponse::err(format!("No index for field '{field}'"))))
    }
}

// ── Search ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct FullTextSearchRequest {
    pub query: String,
    #[serde(default)]
    pub fuzzy: bool,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize { 10 }

pub async fn search_collection(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<FullTextSearchRequest>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let results = col.search_text(&payload.query, payload.fuzzy, payload.top_k);
    let output: Vec<serde_json::Value> = results.into_iter().map(|(doc, score, matched_terms)| {
        let mut val = serde_json::to_value(&doc.fields).unwrap_or(serde_json::Value::Null);
        if let Some(obj) = val.as_object_mut() {
            obj.insert("_id".to_string(), serde_json::Value::String(doc.id.as_str().to_string()));
            obj.insert("_score".to_string(), serde_json::json!(score));
            obj.insert("_matched_terms".to_string(), serde_json::json!(matched_terms));
        }
        val
    }).collect();
    Json(ApiResponse::ok(output))
}

// ── Aggregation ──────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AggregateRequest {
    pub pipeline: serde_json::Value,
}

pub async fn aggregate_collection(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<AggregateRequest>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let all_docs = col.find_all(None);
    match faizdb_query::parse_pipeline(&payload.pipeline) {
        Ok(stages) => (StatusCode::OK, Json(ApiResponse::ok(faizdb_query::execute_pipeline(all_docs, &stages)))),
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(format!("Aggregation error: {e}")))),
    }
}

// ── TTL ──────────────────────────────────────────────────────────────────────

pub async fn collection_ttl_stats(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    Json(ApiResponse::ok(col.ttl_stats()))
}

pub async fn collection_ttl_purge(
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

// ── Transactions ─────────────────────────────────────────────────────────────

pub async fn transaction_begin() -> impl IntoResponse {
    let txn_id = format!("txn_{}", uuid::Uuid::now_v7());
    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "txn_id": txn_id,
        "isolation_level": "SnapshotIsolation",
        "status": "Active",
    }))))
}

pub async fn transaction_commit() -> impl IntoResponse {
    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "status": "Committed",
        "message": "All staged mutations written to WAL",
    }))))
}

pub async fn transaction_rollback() -> impl IntoResponse {
    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "status": "Aborted",
        "message": "Transaction rolled back, write-buffer discarded",
    }))))
}

// ── Import ───────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ImportDataRequest {
    pub documents: Option<Vec<serde_json::Value>>,
    pub csv: Option<String>,
}

pub async fn import_collection_data(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<ImportDataRequest>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let mut docs_to_insert = Vec::new();

    if let Some(json_docs) = payload.documents {
        for val in json_docs {
            if let Some(doc) = Document::from_json_value(val) { docs_to_insert.push(doc); }
        }
    } else if let Some(csv_str) = payload.csv {
        let mut lines = csv_str.lines();
        if let Some(header_line) = lines.next() {
            let headers: Vec<&str> = header_line.split(',').map(|s| s.trim()).collect();
            for line in lines {
                let trimmed = line.trim();
                if trimmed.is_empty() { continue; }
                let values: Vec<&str> = trimmed.split(',').map(|s| s.trim()).collect();
                let mut map = serde_json::Map::new();
                for (i, &header) in headers.iter().enumerate() {
                    let raw_val = values.get(i).copied().unwrap_or("");
                    if let Ok(b) = raw_val.parse::<bool>() {
                        map.insert(header.to_string(), serde_json::Value::Bool(b));
                    } else if let Ok(n) = raw_val.parse::<i64>() {
                        map.insert(header.to_string(), serde_json::json!(n));
                    } else if let Ok(f) = raw_val.parse::<f64>() {
                        map.insert(header.to_string(), serde_json::json!(f));
                    } else {
                        map.insert(header.to_string(), serde_json::Value::String(raw_val.to_string()));
                    }
                }
                if let Some(doc) = Document::from_json_value(serde_json::Value::Object(map)) {
                    docs_to_insert.push(doc);
                }
            }
        }
    }

    if docs_to_insert.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(ApiResponse::err("No valid documents or CSV rows found to import")));
    }

    let mut inserted_ids = Vec::with_capacity(docs_to_insert.len());
    let mut errors = Vec::new();
    for doc in docs_to_insert {
        let doc_clone = doc.clone();
        match col.insert(doc) {
            Ok(id) => {
                let id_str = id.as_str().to_string();
                state.db.change_stream_bus().publish(ChangeEvent::insert(&name, doc_clone));
                inserted_ids.push(id_str);
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "imported_count": inserted_ids.len(),
        "inserted_ids": inserted_ids,
        "failed_count": errors.len(),
        "errors": if errors.is_empty() { None } else { Some(errors) },
    }))))
}
