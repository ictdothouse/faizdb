//! FaizDB gRPC Service Implementation.

use std::sync::Arc;
use std::time::Instant;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tonic::{Request, Response, Status};

use faizdb_core::document::model::Document;
use faizdb_query::{parse_query, DatabaseContext};

use super::proto::{
    ChangeEventMsg, FaizDbService, HealthRequest, HealthResponse, InsertRequest,
    InsertResponse, QueryRequest, QueryResponse, StreamRequest, VectorHit,
    VectorSearchRequest, VectorSearchResponse,
};

/// Implementation of the FaizDb gRPC service
pub struct FaizDbGrpcService {
    db: Arc<DatabaseContext>,
    start_time: Instant,
}

impl FaizDbGrpcService {
    pub fn new(db: Arc<DatabaseContext>) -> Self {
        Self {
            db,
            start_time: Instant::now(),
        }
    }
}

#[tonic::async_trait]
impl FaizDbService for FaizDbGrpcService {
    type SubscribeChangeStreamStream = std::pin::Pin<
        Box<dyn tokio_stream::Stream<Item = Result<ChangeEventMsg, Status>> + Send + 'static>,
    >;

    async fn execute_query(
        &self,
        request: Request<QueryRequest>,
    ) -> Result<Response<QueryResponse>, Status> {
        let req = request.into_inner();
        let start = Instant::now();

        match parse_query(&req.query) {
            Ok(stmt) => match self.db.execute(stmt) {
                Ok(res) => {
                    let elapsed = start.elapsed().as_micros() as u64;
                    let result_json = serde_json::to_string(&res)
                        .unwrap_or_else(|_| "{}".to_string());
                    Ok(Response::new(QueryResponse {
                        success: true,
                        result_json,
                        execution_time_us: elapsed,
                        error_message: String::new(),
                    }))
                }
                Err(exec_err) => {
                    let elapsed = start.elapsed().as_micros() as u64;
                    Ok(Response::new(QueryResponse {
                        success: false,
                        result_json: String::new(),
                        execution_time_us: elapsed,
                        error_message: exec_err,
                    }))
                }
            },
            Err(parse_err) => {
                let elapsed = start.elapsed().as_micros() as u64;
                Ok(Response::new(QueryResponse {
                    success: false,
                    result_json: String::new(),
                    execution_time_us: elapsed,
                    error_message: format!("Syntax error: {parse_err}"),
                }))
            }
        }
    }

    async fn vector_search(
        &self,
        request: Request<VectorSearchRequest>,
    ) -> Result<Response<VectorSearchResponse>, Status> {
        let req = request.into_inner();
        let start = Instant::now();

        if req.vector.is_empty() {
            return Err(Status::invalid_argument("Search vector cannot be empty"));
        }

        let col = self.db.get_or_create_collection(&req.collection);
        let docs = col.find_all(None);

        let top_k = if req.top_k == 0 { 10 } else { req.top_k as usize };

        // Perform vector similarity calculation (Cosine similarity)
        let mut scored_hits = Vec::new();
        for doc in &docs {
            if let Some(faizdb_core::document::model::Value::Vector(v)) = doc.get("vector") {
                if v.len() == req.vector.len() {
                    let score = cosine_similarity(&req.vector, v);
                    let doc_json = serde_json::to_string(doc).unwrap_or_default();
                    scored_hits.push((score, doc.id.as_str().to_string(), doc_json));
                }
            }
        }

        // Sort descending by similarity score
        scored_hits.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored_hits.truncate(top_k);

        let hits = scored_hits
            .into_iter()
            .map(|(score, id, document_json)| VectorHit {
                id,
                score,
                document_json,
            })
            .collect();

        let elapsed = start.elapsed().as_micros() as u64;

        Ok(Response::new(VectorSearchResponse {
            hits,
            search_time_us: elapsed,
        }))
    }

    async fn insert_documents(
        &self,
        request: Request<InsertRequest>,
    ) -> Result<Response<InsertResponse>, Status> {
        let req = request.into_inner();
        let col = self.db.get_or_create_collection(&req.collection);

        let mut inserted_ids = Vec::with_capacity(req.documents_json.len());

        for json_str in req.documents_json {
            let val: serde_json::Value = serde_json::from_str(&json_str)
                .map_err(|e| Status::invalid_argument(format!("Invalid JSON: {e}")))?;

            let doc = Document::from_json_value(val)
                .ok_or_else(|| Status::invalid_argument("Expected JSON Object"))?;

            let doc_clone = doc.clone();
            let id = col.insert(doc).map_err(|e| Status::internal(e.to_string()))?;
            let id_str = id.as_str().to_string();

            // Emit change event
            self.db.change_stream_bus().publish(
                faizdb_core::stream::ChangeEvent::insert(&req.collection, doc_clone),
            );

            inserted_ids.push(id_str);
        }

        let count = inserted_ids.len() as u32;

        Ok(Response::new(InsertResponse {
            inserted_count: count,
            inserted_ids,
        }))
    }

    async fn subscribe_change_stream(
        &self,
        request: Request<StreamRequest>,
    ) -> Result<Response<Self::SubscribeChangeStreamStream>, Status> {
        let req = request.into_inner();
        let target_col = req.collection;

        let rx = self.db.change_stream_bus().subscribe();
        let broadcast_stream = BroadcastStream::new(rx);

        let out_stream = broadcast_stream.filter_map(move |item| match item {
            Ok(event) => {
                if target_col.is_empty() || target_col == "*" || event.collection == target_col {
                    Some(Ok(ChangeEventMsg {
                        event_type: format!("{:?}", event.operation_type),
                        collection: event.collection,
                        document_id: event.document_id,
                        payload_json: event
                            .full_document
                            .map(|d| serde_json::to_string(&d).unwrap_or_default())
                            .unwrap_or_default(),
                        timestamp_ms: event.timestamp.timestamp_millis() as u64,
                    }))
                } else {
                    None
                }
            }
            Err(_) => None,
        });

        Ok(Response::new(Box::pin(out_stream)))
    }

    async fn health_check(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        let uptime = self.start_time.elapsed().as_secs();

        Ok(Response::new(HealthResponse {
            status: "SERVING".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_seconds: uptime,
        }))
    }
}

/// Compute cosine similarity between two vectors
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}
