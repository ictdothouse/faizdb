//! # FaizDB WebAssembly (WASM) Headless In-Browser Engine
//!
//! Exposes a standalone, memory-safe database microkernel compiling down to WebAssembly.
//! Allows running Document JSON collections, SQL queries, and HNSW high-dimensional
//! vector similarity search directly inside browser web workers, Cloudflare Workers,
//! Deno, and Node.js without any background network daemons.

use faizdb_core::document::collection::Collection;
use faizdb_core::document::model::{Document, Value};
use faizdb_vector::{DistanceMetric, HnswConfig, HnswIndex};
use parking_lot::RwLock;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Vector search hit structure for JSON serialization in WASM
#[derive(Serialize)]
pub struct WasmVectorHit {
    pub id: String,
    pub distance: f32,
    pub score: f32,
}

/// Standalone In-Memory WebAssembly Database Instance
#[wasm_bindgen]
pub struct FaizDbWasm {
    collections: Arc<RwLock<HashMap<String, Collection>>>,
    vector_indexes: Arc<RwLock<HashMap<String, HnswIndex>>>,
}

#[wasm_bindgen]
impl FaizDbWasm {
    /// Initialize a new in-memory FaizDB WebAssembly engine
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            collections: Arc::new(RwLock::new(HashMap::new())),
            vector_indexes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new document collection
    #[wasm_bindgen]
    pub fn create_collection(&self, name: &str) -> Result<bool, JsValue> {
        let mut cols = self.collections.write();
        if cols.contains_key(name) {
            return Ok(false);
        }
        cols.insert(name.to_string(), Collection::new(name));
        Ok(true)
    }

    /// Insert a JSON document into the specified collection and return its ID
    #[wasm_bindgen]
    pub fn insert(&self, collection_name: &str, json_str: &str) -> Result<String, JsValue> {
        let doc: Document = match serde_json::from_str(json_str) {
            Ok(d) => d,
            Err(_) => {
                let json_val: serde_json::Value = serde_json::from_str(json_str)
                    .map_err(|e| JsValue::from_str(&format!("Invalid JSON: {e}")))?;
                if let serde_json::Value::Object(obj) = json_val {
                    let mut d = Document::new();
                    for (k, v) in obj {
                        d.set(k, Value::from(v));
                    }
                    d
                } else {
                    return Err(JsValue::from_str("Expected JSON object for document"));
                }
            }
        };

        let mut cols = self.collections.write();
        let collection = cols
            .entry(collection_name.to_string())
            .or_insert_with(|| Collection::new(collection_name));

        let doc_id = doc.id.to_string();
        collection
            .insert(doc)
            .map_err(|e| JsValue::from_str(&format!("Insert error: {e}")))?;

        Ok(doc_id)
    }

    /// Retrieve a document by ID as a JSON string (or None if not found)
    #[wasm_bindgen]
    pub fn find_by_id(&self, collection_name: &str, id: &str) -> Result<Option<String>, JsValue> {
        let cols = self.collections.read();
        let collection = cols
            .get(collection_name)
            .ok_or_else(|| JsValue::from_str(&format!("Collection '{collection_name}' not found")))?;

        match collection.find_by_id(id) {
            Ok(doc) => {
                let json = serde_json::to_string(&doc)
                    .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))?;
                Ok(Some(json))
            }
            Err(_) => Ok(None),
        }
    }

    /// Count documents in a collection
    #[wasm_bindgen]
    pub fn count(&self, collection_name: &str) -> Result<usize, JsValue> {
        let cols = self.collections.read();
        let collection = cols
            .get(collection_name)
            .ok_or_else(|| JsValue::from_str(&format!("Collection '{collection_name}' not found")))?;

        Ok(collection.count(&[]) as usize)
    }

    /// Create an HNSW vector index in memory
    /// metric can be "cosine", "euclidean", or "dot"
    #[wasm_bindgen]
    pub fn create_vector_index(
        &self,
        name: &str,
        dimension: usize,
        metric_str: &str,
    ) -> Result<bool, JsValue> {
        let metric = match metric_str.to_lowercase().as_str() {
            "cosine" => DistanceMetric::Cosine,
            "euclidean" | "l2" => DistanceMetric::Euclidean,
            "dot" | "inner_product" => DistanceMetric::DotProduct,
            other => return Err(JsValue::from_str(&format!("Unknown metric '{other}'"))),
        };

        let config = HnswConfig::new(dimension, metric);
        let index = HnswIndex::new(config);

        let mut idxs = self.vector_indexes.write();
        idxs.insert(name.to_string(), index);
        Ok(true)
    }

    /// Insert a vector embedding into an HNSW index
    #[wasm_bindgen]
    pub fn insert_vector(
        &self,
        index_name: &str,
        id: &str,
        vector: &[f32],
    ) -> Result<bool, JsValue> {
        let mut idxs = self.vector_indexes.write();
        let index = idxs
            .get_mut(index_name)
            .ok_or_else(|| JsValue::from_str(&format!("Vector index '{index_name}' not found")))?;

        index
            .insert(id, vector.to_vec())
            .map_err(|e| JsValue::from_str(&format!("Vector insertion error: {e}")))?;

        Ok(true)
    }

    /// Search for nearest neighbors using HNSW vector similarity
    #[wasm_bindgen]
    pub fn vector_search(
        &self,
        index_name: &str,
        query_vector: &[f32],
        top_k: usize,
    ) -> Result<String, JsValue> {
        let idxs = self.vector_indexes.read();
        let index = idxs
            .get(index_name)
            .ok_or_else(|| JsValue::from_str(&format!("Vector index '{index_name}' not found")))?;

        let hits = index.search(query_vector, top_k);

        let wasm_hits: Vec<WasmVectorHit> = hits
            .into_iter()
            .map(|h| WasmVectorHit {
                id: h.id,
                distance: h.distance,
                score: (1.0 / (1.0 + h.distance)).clamp(0.0, 1.0),
            })
            .collect();

        serde_json::to_string(&wasm_hits)
            .map_err(|e| JsValue::from_str(&format!("Serialization error: {e}")))
    }

    /// Return engine version and capabilities
    #[wasm_bindgen]
    pub fn version(&self) -> String {
        "FaizDB WebAssembly Engine v0.1.0 (Headless in-browser preview)".to_string()
    }
}

impl Default for FaizDbWasm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_engine_document_lifecycle() {
        let db = FaizDbWasm::new();
        assert!(db.create_collection("users").unwrap());

        let doc_json = r#"{"name": "Ahmad Faiz", "role": "Architect", "score": 9900}"#;
        let id = db.insert("users", doc_json).unwrap();
        assert!(!id.is_empty());

        assert_eq!(db.count("users").unwrap(), 1);

        let retrieved = db.find_by_id("users", &id).unwrap();
        assert!(retrieved.is_some());
        assert!(retrieved.unwrap().contains("Ahmad Faiz"));
    }

    #[test]
    fn test_wasm_engine_vector_lifecycle() {
        let db = FaizDbWasm::new();
        assert!(db.create_vector_index("embeddings", 4, "cosine").unwrap());

        assert!(db.insert_vector("embeddings", "doc_1", &[1.0, 0.0, 0.0, 0.0]).unwrap());
        assert!(db.insert_vector("embeddings", "doc_2", &[0.0, 1.0, 0.0, 0.0]).unwrap());

        let results_json = db.vector_search("embeddings", &[1.0, 0.0, 0.0, 0.0], 2).unwrap();
        assert!(results_json.contains("doc_1"));
    }
}
