//! Collection CRUD, query, aggregation, full-text search, indexes, TTL, transactions, and import.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
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

#[derive(Debug, Deserialize, Default)]
pub struct TxnQuery {
    pub txn_id: Option<String>,
}

// ── Documents ────────────────────────────────────────────────────────────────

pub async fn insert_document(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    headers: HeaderMap,
    Query(query): Query<TxnQuery>,
    Json(mut doc_val): Json<serde_json::Value>,
) -> impl IntoResponse {
    let txn_id = headers
        .get("x-txn-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .or(query.txn_id)
        .or_else(|| {
            if let Some(obj) = doc_val.as_object_mut() {
                obj.remove("_txn_id")
                    .or_else(|| obj.remove("txn_id"))
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
            } else {
                None
            }
        });

    let doc = match Document::from_json_value(doc_val) {
        Some(d) => d,
        None => return (StatusCode::BAD_REQUEST, Json(ApiResponse::err("Expected JSON object"))),
    };

    // If client supplied a transaction ID, stage the mutation into the transaction write buffer
    if let Some(ref tid) = txn_id {
        if let Some(txn_mutex) = state.db.active_txns().get(tid) {
            let doc_id = doc.id.as_str().to_string();
            let key = format!("doc:{}:{}", name, doc_id);
            let val = match serde_json::to_vec(&doc) {
                Ok(b) => b,
                Err(e) => return (StatusCode::BAD_REQUEST, Json(ApiResponse::err(e.to_string()))),
            };
            let mut txn = txn_mutex.lock();
            if let Err(e) = txn.put(key.into_bytes(), val) {
                return (StatusCode::BAD_REQUEST, Json(ApiResponse::err(e.to_string())));
            }
            return (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                "id": doc_id,
                "staged": true,
                "txn_id": tid,
                "collection": name,
            }))));
        } else {
            return (StatusCode::NOT_FOUND, Json(ApiResponse::err(format!("Transaction '{tid}' not found or already closed"))));
        }
    }

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
    headers: HeaderMap,
    Query(query): Query<TxnQuery>,
) -> impl IntoResponse {
    let txn_id = headers
        .get("x-txn-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .or(query.txn_id);

    // If client supplied a transaction ID, stage deletion into the transaction write buffer
    if let Some(ref tid) = txn_id {
        if let Some(txn_mutex) = state.db.active_txns().get(tid) {
            let key = format!("doc:{}:{}", name, id);
            let mut txn = txn_mutex.lock();
            if let Err(e) = txn.delete(key.into_bytes()) {
                return (StatusCode::BAD_REQUEST, Json(ApiResponse::err(e.to_string())));
            }
            return (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                "deleted": true,
                "staged": true,
                "id": id,
                "txn_id": tid,
                "collection": name,
            }))));
        } else {
            return (StatusCode::NOT_FOUND, Json(ApiResponse::err(format!("Transaction '{tid}' not found or already closed"))));
        }
    }

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

#[derive(Debug, Deserialize, Default)]
pub struct TransactionRequest {
    pub txn_id: Option<String>,
}

pub async fn transaction_begin(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let txn = state.db.tx_manager().begin();
    let txn_id = format!("txn_{}", txn.id);
    let snapshot_ts = txn.snapshot_ts();
    state.db.active_txns().insert(txn_id.clone(), Arc::new(parking_lot::Mutex::new(txn)));

    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
        "txn_id": txn_id,
        "isolation_level": "SnapshotIsolation",
        "snapshot_ts": snapshot_ts,
        "status": "Active",
    }))))
}

pub async fn transaction_commit(
    State(state): State<Arc<AppState>>,
    body: Option<Json<TransactionRequest>>,
) -> impl IntoResponse {
    let txn_id = body.and_then(|b| b.0.txn_id);
    if let Some(id) = txn_id {
        if let Some(txn_entry) = state.db.active_txns().get(&id) {
            let mut txn = txn_entry.value().lock();

            // 1. Atomically claim the transaction by transitioning from Active to Committing
            if let Err(e) = txn.try_set_committing() {
                return (
                    StatusCode::CONFLICT,
                    Json(ApiResponse::err(format!("Transaction cannot be committed: {e}"))),
                );
            }

            // 2. Validate snapshot isolation and commit atomically in TransactionManager FIRST
            // This guarantees rejected writes are never published to durable storage!
            match state.db.tx_manager().commit(&mut txn) {
                Ok(()) => {
                    let writes = txn.write_buffer().clone();

                    // 3. Validation succeeded: persist staged writes to durable disk storage
                    if let Some(storage) = state.db.storage() {
                        for (key, write) in &writes {
                            let res = match write {
                                faizdb_core::transaction::mvcc::TxnWrite::Put(val) => {
                                    storage.put(key, val.as_slice())
                                }
                                faizdb_core::transaction::mvcc::TxnWrite::Delete => {
                                    storage.delete(key)
                                }
                            };
                            if let Err(e) = res {
                                return (
                                    StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(ApiResponse::err(format!(
                                        "Transaction commit failed to persist write for key '{}': {e}",
                                        String::from_utf8_lossy(key)
                                    ))),
                                );
                            }
                        }
                    }

                    // 4. Apply staged mutations into in-memory collections and publish change stream events
                    for (key_bytes, write) in &writes {
                        if let Ok(key_str) = std::str::from_utf8(key_bytes) {
                            if key_str.starts_with("doc:") {
                                let parts: Vec<&str> = key_str.splitn(3, ':').collect();
                                if parts.len() == 3 {
                                    let col_name = parts[1];
                                    let doc_id = parts[2];
                                    let col = state.db.get_or_create_collection(col_name);
                                    match write {
                                        faizdb_core::transaction::mvcc::TxnWrite::Put(val) => {
                                            if let Ok(doc) = serde_json::from_slice::<Document>(val) {
                                                col.load_document(doc.clone());
                                                state.db.change_stream_bus().publish(ChangeEvent::insert(col_name, doc));
                                            }
                                        }
                                        faizdb_core::transaction::mvcc::TxnWrite::Delete => {
                                            let _ = col.delete_by_id(doc_id);
                                            state.db.change_stream_bus().publish(ChangeEvent::delete(col_name, doc_id));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    drop(txn);
                    state.db.active_txns().remove(&id);

                    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                        "txn_id": id,
                        "status": "Committed",
                        "staged_writes_count": writes.len(),
                        "message": "All staged mutations verified with snapshot isolation and written to WAL",
                    }))))
                }
                Err(e) => {
                    // Conflict or validation failure: remove from active transactions
                    drop(txn);
                    state.db.active_txns().remove(&id);
                    (StatusCode::CONFLICT, Json(ApiResponse::err(format!("Transaction commit conflict: {e}"))))
                }
            }
        } else {
            (StatusCode::NOT_FOUND, Json(ApiResponse::err("Transaction not found or already closed")))
        }
    } else {
        (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
            "status": "Committed",
            "message": "All staged mutations written to WAL",
        }))))
    }
}

pub async fn transaction_rollback(
    State(state): State<Arc<AppState>>,
    body: Option<Json<TransactionRequest>>,
) -> impl IntoResponse {
    let txn_id = body.and_then(|b| b.0.txn_id);
    if let Some(id) = txn_id {
        if let Some(txn_entry) = state.db.active_txns().get(&id) {
            let mut txn = txn_entry.value().lock();
            match txn.try_abort() {
                Ok(()) => {
                    state.db.tx_manager().abort(&mut txn);
                    drop(txn);
                    state.db.active_txns().remove(&id);
                    (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                        "txn_id": id,
                        "status": "Aborted",
                        "message": "Transaction rolled back, write-buffer discarded",
                    }))))
                }
                Err(e) => {
                    (StatusCode::CONFLICT, Json(ApiResponse::err(format!("Cannot abort transaction: {e}"))))
                }
            }
        } else {
            (StatusCode::NOT_FOUND, Json(ApiResponse::err("Transaction not found or already closed")))
        }
    } else {
        (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
            "status": "Aborted",
            "message": "Transaction write-buffer discarded",
        }))))
    }
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
