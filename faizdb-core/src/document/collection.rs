//! Collection — a named group of documents (like a table in SQL, collection in MongoDB).
//!
//! Collections in FaizDB are:
//! - Schema-optional (can enforce schema or go schema-free)
//! - Automatically indexed on `_id`
//! - Support secondary indexes, unique constraints, and TTL
//! - Thread-safe for concurrent access

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;

use super::model::{Document, DocumentId, Value};
use crate::error::{FaizError, FaizResult};

/// Configuration for a collection
#[derive(Debug, Clone)]
pub struct CollectionConfig {
    /// Collection name
    pub name: String,

    /// Maximum document size in bytes (default: 256MB)
    pub max_document_size: usize,

    /// Whether to enable schema validation
    pub schema_validation: bool,

    /// Optional JSON Schema for validation
    pub schema: Option<serde_json::Value>,

    /// Whether to auto-generate IDs for documents without _id
    pub auto_generate_id: bool,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            max_document_size: crate::MAX_DOCUMENT_SIZE,
            schema_validation: false,
            schema: None,
            auto_generate_id: true,
        }
    }
}

/// Index type for a collection
#[derive(Debug, Clone)]
pub enum IndexType {
    /// Standard B-Tree index for equality and range queries
    BTree,
    /// Hash index for fast equality lookups
    Hash,
    /// Text index for full-text search
    Text,
    /// Vector index for similarity search (HNSW)
    Vector { dimensions: usize },
    /// Geospatial index
    Geo2D,
}

/// An index definition
#[derive(Debug, Clone)]
pub struct IndexDef {
    /// Index name
    pub name: String,
    /// Fields to index (with sort order: 1 = asc, -1 = desc)
    pub fields: Vec<(String, i8)>,
    /// Index type
    pub index_type: IndexType,
    /// Whether the index enforces uniqueness
    pub unique: bool,
    /// Whether the index is sparse (excludes null values)
    pub sparse: bool,
}

/// Statistics for a collection
#[derive(Debug, Clone)]
pub struct CollectionStats {
    /// Total number of documents
    pub document_count: u64,
    /// Total size of all documents in bytes
    pub total_size: u64,
    /// Average document size in bytes
    pub avg_document_size: u64,
    /// Number of indexes
    pub index_count: usize,
}

/// A collection of documents — the primary data container in FaizDB.
///
/// Thread-safe: uses `DashMap` for lock-free concurrent reads and
/// fine-grained locking for writes.
pub struct Collection {
    /// Collection configuration
    config: CollectionConfig,

    /// Primary document store (indexed by _id)
    /// Using DashMap for concurrent, lock-free access
    documents: DashMap<String, Document>,

    /// Secondary B-Tree index map (field name -> SecondaryIndex instance)
    secondary_indexes: DashMap<String, Arc<crate::document::index::SecondaryIndex>>,

    /// Secondary index definitions
    indexes: RwLock<Vec<IndexDef>>,

    /// Secondary index data: index_name -> field_value -> document_ids
    index_data: DashMap<String, BTreeMap<String, Vec<String>>>,

    /// Document count (atomic for lock-free reads)
    doc_count: AtomicU64,

    /// Total data size in bytes
    total_size: AtomicU64,

    /// Native Full-Text Inverted Index (BM25)
    text_index: crate::search::InvertedIndex,

    /// Time-To-Live (TTL) & Auto-Expiry Cache Scheduler
    ttl: crate::ttl::TtlManager,

    /// Optional underlying LSM-Tree storage engine for durability
    storage: Option<Arc<crate::storage::engine::StorageEngine>>,
}

impl Collection {
    /// Create a new collection with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            config: CollectionConfig {
                name: name.into(),
                ..Default::default()
            },
            documents: DashMap::new(),
            secondary_indexes: DashMap::new(),
            indexes: RwLock::new(Vec::new()),
            index_data: DashMap::new(),
            doc_count: AtomicU64::new(0),
            total_size: AtomicU64::new(0),
            text_index: crate::search::InvertedIndex::new(),
            ttl: crate::ttl::TtlManager::new(),
            storage: None,
        }
    }

    /// Create a collection backed by a persistent StorageEngine (WAL + MemTable + SSTables)
    pub fn with_storage(
        name: impl Into<String>,
        storage: Arc<crate::storage::engine::StorageEngine>,
    ) -> Self {
        let mut col = Self::new(name);
        col.storage = Some(storage);
        col
    }

    /// Set the persistent StorageEngine
    pub fn set_storage(&mut self, storage: Arc<crate::storage::engine::StorageEngine>) {
        self.storage = Some(storage);
    }

    /// Get reference to underlying StorageEngine if configured
    pub fn storage(&self) -> Option<Arc<crate::storage::engine::StorageEngine>> {
        self.storage.clone()
    }

    /// Create a collection with custom configuration
    pub fn with_config(config: CollectionConfig) -> Self {
        Self {
            config,
            documents: DashMap::new(),
            secondary_indexes: DashMap::new(),
            indexes: RwLock::new(Vec::new()),
            index_data: DashMap::new(),
            doc_count: AtomicU64::new(0),
            total_size: AtomicU64::new(0),
            text_index: crate::search::InvertedIndex::new(),
            ttl: crate::ttl::TtlManager::new(),
            storage: None,
        }
    }

    /// Get the collection name
    pub fn name(&self) -> &str {
        &self.config.name
    }

    /// Get collection statistics
    pub fn stats(&self) -> CollectionStats {
        let count = self.doc_count.load(Ordering::Relaxed);
        let total = self.total_size.load(Ordering::Relaxed);
        CollectionStats {
            document_count: count,
            total_size: total,
            avg_document_size: total.checked_div(count).unwrap_or(0),
            index_count: self.indexes.read().len(),
        }
    }

    // ── CRUD Operations ──────────────────────────────────────────

    /// Insert a document into the collection.
    ///
    /// Returns the document ID of the inserted document.
    ///
    /// # Errors
    /// - `DocumentTooLarge` if the document exceeds the size limit
    /// - `DuplicateKey` if a document with the same ID already exists
    /// - `SchemaValidation` if schema validation is enabled and fails
    pub fn insert(&self, doc: Document) -> FaizResult<DocumentId> {
        // Check document size
        let size = doc.size_bytes();
        if size > self.config.max_document_size {
            return Err(FaizError::DocumentTooLarge {
                size,
                max: self.config.max_document_size,
            });
        }

        let id = doc.id.clone();
        let id_str = id.as_str().to_string();

        // Check for duplicate key on primary ID
        if self.documents.contains_key(&id_str) {
            return Err(FaizError::DuplicateKey {
                collection: self.config.name.clone(),
                field: "_id".into(),
                value: id_str,
            });
        }

        // Check unique constraints across all active secondary indexes BEFORE mutating
        for idx_entry in self.secondary_indexes.iter() {
            idx_entry.value().check_unique(&doc)?;
        }

        // Update secondary B-Tree indexes
        for idx_entry in self.secondary_indexes.iter() {
            idx_entry.value().insert(&doc);
        }
        self.update_indexes_insert(&doc);

        // Index for Full-Text Search (BM25)
        let doc_text = extract_doc_text(&doc);
        self.text_index.index_document(&id_str, &doc_text);

        // Register TTL expiration if specified in document (_ttl or ttl in seconds)
        if let Some(ttl_val) = doc.get("_ttl").or_else(|| doc.get("ttl")) {
            if let Some(secs) = ttl_val.as_i64() {
                if secs > 0 {
                    self.ttl.set_expiry(&id_str, secs as u64);
                }
            }
        }

        // Insert into primary store
        self.documents.insert(id_str.clone(), doc.clone());
        self.doc_count.fetch_add(1, Ordering::Relaxed);
        self.total_size.fetch_add(size as u64, Ordering::Relaxed);

        // If storage engine is connected, persist through WAL and MemTable
        if let Some(storage) = &self.storage {
            let key = format!("doc:{}:{}", self.config.name, id_str).into_bytes();
            if let Ok(val) = serde_json::to_vec(&doc) {
                storage.put(&key, &val)?;
            }
        }

        Ok(id)
    }

    /// Insert multiple documents at once (bulk insert).
    ///
    /// Returns a vector of document IDs. More efficient than individual inserts
    /// as it batches index updates.
    pub fn insert_many(&self, docs: Vec<Document>) -> FaizResult<Vec<DocumentId>> {
        let mut ids = Vec::with_capacity(docs.len());

        for doc in docs {
            ids.push(self.insert(doc)?);
        }

        Ok(ids)
    }

    /// Load an existing document recovered from persistent storage into memory structures without re-persisting
    pub fn load_document(&self, doc: Document) {
        let id_str = doc.id.as_str().to_string();
        let size = doc.size_bytes();

        for idx_entry in self.secondary_indexes.iter() {
            idx_entry.value().insert(&doc);
        }
        self.update_indexes_insert(&doc);

        let doc_text = extract_doc_text(&doc);
        self.text_index.index_document(&id_str, &doc_text);

        if let Some(ttl_val) = doc.get("_ttl").or_else(|| doc.get("ttl")) {
            if let Some(secs) = ttl_val.as_i64() {
                if secs > 0 {
                    self.ttl.set_expiry(&id_str, secs as u64);
                }
            }
        }

        self.documents.insert(id_str, doc);
        self.doc_count.fetch_add(1, Ordering::Relaxed);
        self.total_size.fetch_add(size as u64, Ordering::Relaxed);
    }

    /// Find a document by its ID (with lazy TTL evaluation).
    pub fn find_by_id(&self, id: &str) -> FaizResult<Document> {
        if self.ttl.is_expired(id, crate::ttl::current_time_ms()) {
            let _ = self.delete_by_id(id);
            return Err(FaizError::DocumentNotFound {
                collection: self.config.name.clone(),
                id: id.to_string(),
            });
        }

        self.documents
            .get(id)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| FaizError::DocumentNotFound {
                collection: self.config.name.clone(),
                id: id.to_string(),
            })
    }

    /// Find all documents matching a filter.
    ///
    /// The filter is a set of key-value pairs that must all match.
    /// Supports nested field access with dot notation.
    ///
    /// # Example
    /// ```rust,ignore
    /// // Find users older than 25 in Kuala Lumpur
    /// let filter = vec![
    ///     ("city".to_string(), Value::String("KL".into())),
    /// ];
    /// let results = collection.find(&filter, None, None)?;
    /// ```
    pub fn find(
        &self,
        filter: &[(String, Value)],
        limit: Option<usize>,
        skip: Option<usize>,
    ) -> FaizResult<Vec<Document>> {
        self.purge_expired();
        let skip = skip.unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);

        let results: Vec<Document> = self
            .documents
            .iter()
            .filter(|entry| {
                let doc = entry.value();
                filter.iter().all(|(key, expected)| {
                    if let Some(actual) = doc.get_nested(key) {
                        actual == expected
                    } else {
                        false
                    }
                })
            })
            .skip(skip)
            .take(limit)
            .map(|entry| entry.value().clone())
            .collect();

        Ok(results)
    }

    /// Find all documents in the collection (auto-purging expired TTL keys).
    pub fn find_all(&self, limit: Option<usize>) -> Vec<Document> {
        self.purge_expired();
        let limit = limit.unwrap_or(usize::MAX);
        self.documents
            .iter()
            .take(limit)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Update a document by ID.
    ///
    /// The update function receives the current document and should modify it in place.
    pub fn update_by_id(
        &self,
        id: &str,
        update_fn: impl FnOnce(&mut Document),
    ) -> FaizResult<Document> {
        let mut entry = self
            .documents
            .get_mut(id)
            .ok_or_else(|| FaizError::DocumentNotFound {
                collection: self.config.name.clone(),
                id: id.to_string(),
            })?;

        let old_size = entry.size_bytes() as u64;
        update_fn(entry.value_mut());
        let new_size = entry.size_bytes() as u64;

        // Update total size
        if new_size > old_size {
            self.total_size
                .fetch_add(new_size - old_size, Ordering::Relaxed);
        } else {
            self.total_size
                .fetch_sub(old_size - new_size, Ordering::Relaxed);
        }

        let updated = entry.value().clone();
        if let Some(storage) = &self.storage {
            let key = format!("doc:{}:{}", self.config.name, id).into_bytes();
            let val = serde_json::to_vec(&updated)?;
            storage.put(&key, &val)?;
        }

        Ok(updated)
    }

    /// Update documents matching a filter using field-level updates.
    pub fn update_many(
        &self,
        filter: &[(String, Value)],
        updates: &[(String, Value)],
    ) -> FaizResult<u64> {
        let mut count = 0u64;

        for mut entry in self.documents.iter_mut() {
            let doc = entry.value();
            let matches = filter.iter().all(|(key, expected)| {
                if let Some(actual) = doc.get_nested(key) {
                    actual == expected
                } else {
                    false
                }
            });

            if matches {
                let doc = entry.value_mut();
                for (key, value) in updates {
                    doc.set(key.clone(), value.clone());
                }
                count += 1;
            }
        }

        Ok(count)
    }

    /// Delete a document by ID.
    pub fn delete_by_id(&self, id: &str) -> FaizResult<Document> {
        let (_, doc) = self
            .documents
            .remove(id)
            .ok_or_else(|| FaizError::DocumentNotFound {
                collection: self.config.name.clone(),
                id: id.to_string(),
            })?;

        self.doc_count.fetch_sub(1, Ordering::Relaxed);
        self.total_size
            .fetch_sub(doc.size_bytes() as u64, Ordering::Relaxed);

        // Remove from secondary indexes & text search index
        for idx_entry in self.secondary_indexes.iter() {
            idx_entry.value().remove(&doc);
        }
        self.update_indexes_delete(&doc);
        self.text_index.remove_document(id);

        // If storage engine is connected, persist tombstone through WAL and MemTable
        if let Some(storage) = &self.storage {
            let key = format!("doc:{}:{}", self.config.name, id).into_bytes();
            storage.delete(&key)?;
        }

        Ok(doc)
    }

    /// Full-Text Search with Okapi BM25 Ranking and Fuzzy typo-tolerance
    pub fn search_text(
        &self,
        query: &str,
        fuzzy: bool,
        top_k: usize,
    ) -> Vec<(Document, f64, Vec<String>)> {
        let results = self.text_index.search(query, fuzzy, top_k);
        let mut out = Vec::new();

        for res in results {
            if let Some(entry) = self.documents.get(&res.doc_id) {
                out.push((entry.value().clone(), res.score, res.matched_terms));
            }
        }

        out
    }

    /// Purge all expired TTL documents across the collection
    pub fn purge_expired(&self) -> Vec<String> {
        let expired_ids = self.ttl.purge_expired(crate::ttl::current_time_ms());
        for id in &expired_ids {
            if let Some((_, doc)) = self.documents.remove(id) {
                self.doc_count.fetch_sub(1, Ordering::Relaxed);
                self.total_size
                    .fetch_sub(doc.size_bytes() as u64, Ordering::Relaxed);
                self.update_indexes_delete(&doc);
                self.text_index.remove_document(id);
            }
        }
        expired_ids
    }

    /// Access TTL statistics
    pub fn ttl_stats(&self) -> crate::ttl::TtlStats {
        self.ttl.get_stats()
    }

    /// Delete all documents matching a filter.
    pub fn delete_many(&self, filter: &[(String, Value)]) -> FaizResult<u64> {
        let ids_to_delete: Vec<String> = self
            .documents
            .iter()
            .filter(|entry| {
                let doc = entry.value();
                filter.iter().all(|(key, expected)| {
                    if let Some(actual) = doc.get_nested(key) {
                        actual == expected
                    } else {
                        false
                    }
                })
            })
            .map(|entry| entry.key().clone())
            .collect();

        let mut count = 0u64;
        for id in ids_to_delete {
            if self.delete_by_id(&id).is_ok() {
                count += 1;
            }
        }

        Ok(count)
    }

    /// Count documents matching a filter (empty filter = count all)
    pub fn count(&self, filter: &[(String, Value)]) -> u64 {
        if filter.is_empty() {
            return self.doc_count.load(Ordering::Relaxed);
        }

        self.documents
            .iter()
            .filter(|entry| {
                let doc = entry.value();
                filter.iter().all(|(key, expected)| {
                    if let Some(actual) = doc.get_nested(key) {
                        actual == expected
                    } else {
                        false
                    }
                })
            })
            .count() as u64
    }

    // ── Index Operations ─────────────────────────────────────────

    /// Create a secondary B-Tree index on a specific field with optional unique constraint.
    pub fn create_secondary_index(&self, field: &str, unique: bool) -> FaizResult<String> {
        let index_name = format!("idx_{field}");
        if self.secondary_indexes.contains_key(&index_name) {
            return Ok(index_name);
        }

        let def = crate::document::index::SecondaryIndexDef {
            name: index_name.clone(),
            collection: self.config.name.clone(),
            field: field.to_string(),
            unique,
        };

        let index = Arc::new(crate::document::index::SecondaryIndex::new(def));

        // Index and validate all existing documents
        for entry in self.documents.iter() {
            let doc = entry.value();
            index.check_unique(doc)?;
            index.insert(doc);
        }

        self.secondary_indexes.insert(index_name.clone(), index);
        Ok(index_name)
    }

    /// Lookup documents matching field = value via secondary B-Tree index: O(log N)
    pub fn find_by_secondary_index(&self, field: &str, value: &Value) -> Option<Vec<Document>> {
        let index_name = format!("idx_{field}");
        let idx = self.secondary_indexes.get(&index_name)?;
        let doc_ids = idx.lookup(value);

        let mut docs = Vec::with_capacity(doc_ids.len());
        for id in doc_ids {
            if let Some(entry) = self.documents.get(&id) {
                docs.push(entry.value().clone());
            }
        }
        Some(docs)
    }

    /// Check if a secondary index exists on a field
    pub fn get_secondary_index(
        &self,
        field: &str,
    ) -> Option<Arc<crate::document::index::SecondaryIndex>> {
        let index_name = format!("idx_{field}");
        self.secondary_indexes
            .get(&index_name)
            .map(|i| i.value().clone())
    }

    /// List all secondary indexes
    pub fn list_secondary_indexes(&self) -> Vec<crate::document::index::SecondaryIndexDef> {
        self.secondary_indexes
            .iter()
            .map(|i| i.value().def.clone())
            .collect()
    }

    /// Drop a secondary index
    pub fn drop_secondary_index(&self, field: &str) -> bool {
        let index_name = format!("idx_{field}");
        self.secondary_indexes.remove(&index_name).is_some()
    }

    /// Create a secondary index on the collection (legacy IndexDef compatibility).
    pub fn create_index(&self, index_def: IndexDef) -> FaizResult<()> {
        if let Some((field, _)) = index_def.fields.first() {
            let _ = self.create_secondary_index(field, index_def.unique)?;
        }
        let mut indexes = self.indexes.write();

        // Check if index already exists
        if indexes.iter().any(|i| i.name == index_def.name) {
            return Ok(()); // Idempotent — no error if already exists
        }

        // Build the index data from existing documents
        let mut index_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for entry in self.documents.iter() {
            let doc = entry.value();
            for (field, _) in &index_def.fields {
                if let Some(value) = doc.get_nested(field) {
                    let key = format!("{value}");
                    index_map
                        .entry(key)
                        .or_default()
                        .push(doc.id.as_str().to_string());
                }
            }
        }

        self.index_data.insert(index_def.name.clone(), index_map);
        indexes.push(index_def);

        Ok(())
    }

    /// List all indexes on the collection.
    pub fn list_indexes(&self) -> Vec<IndexDef> {
        self.indexes.read().clone()
    }

    // ── Internal Index Helpers ───────────────────────────────────

    fn update_indexes_insert(&self, doc: &Document) {
        let indexes = self.indexes.read();
        for index_def in indexes.iter() {
            if let Some(mut index_map) = self.index_data.get_mut(&index_def.name) {
                for (field, _) in &index_def.fields {
                    if let Some(value) = doc.get_nested(field) {
                        let key = format!("{value}");
                        index_map
                            .entry(key)
                            .or_default()
                            .push(doc.id.as_str().to_string());
                    }
                }
            }
        }
    }

    fn update_indexes_delete(&self, doc: &Document) {
        let indexes = self.indexes.read();
        for index_def in indexes.iter() {
            if let Some(mut index_map) = self.index_data.get_mut(&index_def.name) {
                for (field, _) in &index_def.fields {
                    if let Some(value) = doc.get_nested(field) {
                        let key = format!("{value}");
                        if let Some(ids) = index_map.get_mut(&key) {
                            ids.retain(|id| id != doc.id.as_str());
                        }
                    }
                }
            }
        }
    }
}

impl std::fmt::Debug for Collection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Collection")
            .field("name", &self.config.name)
            .field("document_count", &self.doc_count.load(Ordering::Relaxed))
            .field("total_size", &self.total_size.load(Ordering::Relaxed))
            .finish()
    }
}

/// Extract all searchable string tokens from a document
fn extract_doc_text(doc: &Document) -> String {
    let mut parts = Vec::new();
    for v in doc.fields.values() {
        match v {
            Value::String(s) => parts.push(s.as_str()),
            Value::Array(arr) => {
                for item in arr {
                    if let Value::String(s) = item {
                        parts.push(s.as_str());
                    }
                }
            }
            _ => {}
        }
    }
    parts.join(" ")
}

// Make Collection safely shareable across threads
unsafe impl Send for Collection {}
unsafe impl Sync for Collection {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_collection() {
        let col = Collection::new("users");
        assert_eq!(col.name(), "users");
        assert_eq!(col.stats().document_count, 0);
    }

    #[test]
    fn test_insert_and_find() {
        let col = Collection::new("users");

        let doc = Document::new()
            .field("name", "Ahmad Faiz")
            .field("age", 30)
            .field("city", "KL");

        let id = col.insert(doc).unwrap();

        // Find by ID
        let found = col.find_by_id(id.as_str()).unwrap();
        assert_eq!(found.get("name").unwrap().as_str(), Some("Ahmad Faiz"));

        // Stats should update
        assert_eq!(col.stats().document_count, 1);
    }

    #[test]
    fn test_find_with_filter() {
        let col = Collection::new("users");

        col.insert(
            Document::new()
                .field("name", "Faiz")
                .field("age", 30)
                .field("city", "KL"),
        )
        .unwrap();

        col.insert(
            Document::new()
                .field("name", "Ali")
                .field("age", 25)
                .field("city", "Penang"),
        )
        .unwrap();

        col.insert(
            Document::new()
                .field("name", "Abu")
                .field("age", 35)
                .field("city", "KL"),
        )
        .unwrap();

        // Filter by city = KL
        let filter = vec![("city".to_string(), Value::String("KL".into()))];
        let results = col.find(&filter, None, None).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_update_document() {
        let col = Collection::new("users");

        let doc = Document::new().field("name", "Faiz").field("score", 85);
        let id = col.insert(doc).unwrap();

        // Update score
        col.update_by_id(id.as_str(), |doc| {
            doc.set("score", 100);
        })
        .unwrap();

        let updated = col.find_by_id(id.as_str()).unwrap();
        assert_eq!(updated.get("score").unwrap().as_i64(), Some(100));
    }

    #[test]
    fn test_delete_document() {
        let col = Collection::new("users");

        let doc = Document::new().field("name", "Faiz");
        let id = col.insert(doc).unwrap();

        assert_eq!(col.stats().document_count, 1);

        col.delete_by_id(id.as_str()).unwrap();

        assert_eq!(col.stats().document_count, 0);
        assert!(col.find_by_id(id.as_str()).is_err());
    }

    #[test]
    fn test_bulk_insert() {
        let col = Collection::new("logs");

        let docs: Vec<Document> = (0..1000)
            .map(|i| {
                Document::new()
                    .field("index", i)
                    .field("message", format!("Log entry {i}"))
            })
            .collect();

        let ids = col.insert_many(docs).unwrap();
        assert_eq!(ids.len(), 1000);
        assert_eq!(col.stats().document_count, 1000);
    }

    #[test]
    fn test_duplicate_key_error() {
        let col = Collection::new("users");

        let doc = Document::with_id("unique-id").field("name", "Faiz");
        col.insert(doc).unwrap();

        let dup = Document::with_id("unique-id").field("name", "Other");
        let result = col.insert(dup);
        assert!(result.is_err());
    }

    #[test]
    fn test_count_with_filter() {
        let col = Collection::new("products");

        for i in 0..50 {
            col.insert(
                Document::new()
                    .field("category", if i % 2 == 0 { "electronics" } else { "books" })
                    .field("price", i * 10),
            )
            .unwrap();
        }

        let filter = vec![("category".to_string(), Value::String("electronics".into()))];
        assert_eq!(col.count(&filter), 25);
        assert_eq!(col.count(&[]), 50);
    }
}
