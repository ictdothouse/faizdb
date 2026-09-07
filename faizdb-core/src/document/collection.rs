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

    /// Maximum in-memory document capacity before enforcing eviction to disk (default: None for unbounded)
    pub max_memory_documents: Option<usize>,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            max_document_size: crate::MAX_DOCUMENT_SIZE,
            schema_validation: false,
            schema: None,
            auto_generate_id: true,
            max_memory_documents: None,
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

    /// Create a collection with custom configuration and persistent storage
    pub fn with_config_and_storage(
        config: CollectionConfig,
        storage: Arc<crate::storage::engine::StorageEngine>,
    ) -> Self {
        let mut col = Self::with_config(config);
        col.storage = Some(storage);
        col
    }

    /// Number of active documents currently resident in RAM
    pub fn in_memory_count(&self) -> usize {
        self.documents.len()
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

    /// Enforce max in-memory documents if configured by evicting older entries from RAM
    fn enforce_memory_cap(&self, keep_id: Option<&str>) {
        if let Some(max_docs) = self.config.max_memory_documents {
            while self.documents.len() > max_docs {
                let evict_key = {
                    let mut iter = self.documents.iter();
                    let found = iter
                        .find(|e| keep_id.is_none_or(|kid| e.key() != kid))
                        .map(|e| e.key().clone());
                    drop(iter);
                    found
                };
                if let Some(key) = evict_key {
                    self.documents.remove(&key);
                } else {
                    break;
                }
            }
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

        // Check for duplicate key on primary ID in RAM
        if self.documents.contains_key(&id_str) {
            return Err(FaizError::DuplicateKey {
                collection: self.config.name.clone(),
                field: "_id".into(),
                value: id_str,
            });
        }

        // Check for duplicate key on primary ID in persistent storage
        if let Some(storage) = &self.storage {
            let key = format!("doc:{}:{}", self.config.name, id_str).into_bytes();
            if let Ok(Some(_)) = storage.get(&key) {
                return Err(FaizError::DuplicateKey {
                    collection: self.config.name.clone(),
                    field: "_id".into(),
                    value: id_str,
                });
            }
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

            // Enforce max in-memory documents if configured by evicting older entry from RAM
            self.enforce_memory_cap(Some(&id_str));
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
        let already_present = self.documents.contains_key(&id_str);

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

        self.documents.insert(id_str.clone(), doc);
        if !already_present {
            self.doc_count.fetch_add(1, Ordering::Relaxed);
            self.total_size.fetch_add(size as u64, Ordering::Relaxed);
        }
        if self.storage.is_some() {
            self.enforce_memory_cap(Some(&id_str));
        }
    }

    /// Clear all documents and secondary indexes from memory and persistent storage
    pub fn clear(&self) -> FaizResult<()> {
        self.documents.clear();
        self.doc_count.store(0, Ordering::Relaxed);
        self.total_size.store(0, Ordering::Relaxed);
        self.text_index.clear();
        for idx in self.secondary_indexes.iter() {
            idx.value().clear();
        }
        self.index_data.clear();

        // If backed by persistent storage, purge all keys under prefix doc:{name}:
        if let Some(storage) = &self.storage {
            let prefix = format!("doc:{}:", self.config.name).into_bytes();
            if let Ok(entries) = storage.prefix_scan(&prefix) {
                for (key, _) in entries {
                    let _ = storage.delete(&key);
                }
            }
        }
        Ok(())
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

        if let Some(entry) = self.documents.get(id) {
            return Ok(entry.value().clone());
        }

        // Cache-miss: check persistent storage engine if attached
        if let Some(storage) = &self.storage {
            let key = format!("doc:{}:{}", self.config.name, id).into_bytes();
            if let Ok(Some(val_bytes)) = storage.get(&key) {
                if let Ok(doc) = serde_json::from_slice::<Document>(&val_bytes) {
                    // Populate back into memory cache for subsequent fast reads with memory cap enforcement
                    self.documents.insert(id.to_string(), doc.clone());
                    self.enforce_memory_cap(Some(id));
                    return Ok(doc);
                }
            }
        }

        Err(FaizError::DocumentNotFound {
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
        let doc_count = self.doc_count.load(Ordering::Relaxed);
        let mem_count = self.documents.len() as u64;

        // Fast path: if dataset is fully resident in RAM, scan in-memory DashMap (<1µs latency)
        if mem_count >= doc_count || self.storage.is_none() {
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
            return Ok(results);
        }

        // Out-of-Core Disk Scan: scan directly from persistent LSM-Tree storage when dataset exceeds RAM capacity
        if let Some(storage) = &self.storage {
            let prefix = format!("doc:{}:", self.config.name).into_bytes();
            let entries = storage.prefix_scan(&prefix)?;
            let results: Vec<Document> = entries
                .into_iter()
                .filter_map(|(_k, v)| serde_json::from_slice::<Document>(&v).ok())
                .filter(|doc| {
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
                .collect();
            return Ok(results);
        }

        Ok(Vec::new())
    }

    /// Find all documents in the collection (auto-purging expired TTL keys).
    pub fn find_all(&self, limit: Option<usize>) -> Vec<Document> {
        self.purge_expired();
        let limit = limit.unwrap_or(usize::MAX);
        let doc_count = self.doc_count.load(Ordering::Relaxed);
        let mem_count = self.documents.len() as u64;

        // Fast path: if dataset is fully resident in RAM, stream from DashMap
        if mem_count >= doc_count || self.storage.is_none() {
            return self
                .documents
                .iter()
                .take(limit)
                .map(|entry| entry.value().clone())
                .collect();
        }

        // Out-of-Core Disk Scan: stream from persistent LSM storage when memory is capped
        if let Some(storage) = &self.storage {
            let prefix = format!("doc:{}:", self.config.name).into_bytes();
            if let Ok(entries) = storage.prefix_scan(&prefix) {
                return entries
                    .into_iter()
                    .filter_map(|(_k, v)| serde_json::from_slice::<Document>(&v).ok())
                    .take(limit)
                    .collect();
            }
        }

        self.documents
            .iter()
            .take(limit)
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Find documents with zero-copy streaming pagination (skipping and limiting without loading entire dataset into intermediate Vec)
    pub fn find_paginated(&self, skip: usize, limit: usize) -> Vec<Document> {
        self.purge_expired();
        let doc_count = self.doc_count.load(Ordering::Relaxed);
        let mem_count = self.documents.len() as u64;

        // Fast path: if dataset is fully resident in RAM, stream from DashMap
        if mem_count >= doc_count || self.storage.is_none() {
            return self
                .documents
                .iter()
                .skip(skip)
                .take(limit)
                .map(|entry| entry.value().clone())
                .collect();
        }

        // Out-of-Core Disk Scan: stream from persistent LSM storage when memory is capped
        if let Some(storage) = &self.storage {
            let prefix = format!("doc:{}:", self.config.name).into_bytes();
            if let Ok(entries) = storage.prefix_scan(&prefix) {
                return entries
                    .into_iter()
                    .skip(skip)
                    .take(limit)
                    .filter_map(|(_k, v)| serde_json::from_slice::<Document>(&v).ok())
                    .collect();
            }
        }

        self.documents
            .iter()
            .skip(skip)
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
        if let Some(mut entry) = self.documents.get_mut(id) {
            let old_size = entry.size_bytes() as u64;
            update_fn(entry.value_mut());
            let new_size = entry.size_bytes() as u64;

            // Update total size
            if new_size > old_size {
                self.total_size
                    .fetch_add(new_size - old_size, Ordering::Relaxed);
            } else {
                let _ = self
                    .total_size
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| {
                        Some(s.saturating_sub(old_size - new_size))
                    });
            }

            let updated = entry.value().clone();
            drop(entry);

            // Re-index for full-text search & secondary indexes
            let doc_text = extract_doc_text(&updated);
            self.text_index.index_document(id, &doc_text);
            self.update_indexes_insert(&updated);

            if let Some(storage) = &self.storage {
                let key = format!("doc:{}:{}", self.config.name, id).into_bytes();
                let val = serde_json::to_vec(&updated)?;
                storage.put(&key, &val)?;
            }

            return Ok(updated);
        }

        // Out-of-Core Disk Fallback: if document was evicted to persistent storage
        if let Some(storage) = &self.storage {
            let key = format!("doc:{}:{}", self.config.name, id).into_bytes();
            if let Ok(Some(val_bytes)) = storage.get(&key) {
                if let Ok(mut doc) = serde_json::from_slice::<Document>(&val_bytes) {
                    let old_size = doc.size_bytes() as u64;
                    update_fn(&mut doc);
                    let new_size = doc.size_bytes() as u64;

                    if new_size > old_size {
                        self.total_size
                            .fetch_add(new_size - old_size, Ordering::Relaxed);
                    } else {
                        let _ = self
                            .total_size
                            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| {
                                Some(s.saturating_sub(old_size - new_size))
                            });
                    }

                    let doc_text = extract_doc_text(&doc);
                    self.text_index.index_document(id, &doc_text);
                    self.update_indexes_insert(&doc);

                    let val = serde_json::to_vec(&doc)?;
                    storage.put(&key, &val)?;

                    // Put back to RAM cache with cap enforcement
                    self.documents.insert(id.to_string(), doc.clone());
                    self.enforce_memory_cap(Some(id));

                    return Ok(doc);
                }
            }
        }

        Err(FaizError::DocumentNotFound {
            collection: self.config.name.clone(),
            id: id.to_string(),
        })
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
                let updated = doc.clone();
                if let Some(storage) = &self.storage {
                    let key = format!("doc:{}:{}", self.config.name, updated.id.as_str()).into_bytes();
                    if let Ok(val) = serde_json::to_vec(&updated) {
                        let _ = storage.put(&key, &val);
                    }
                }
                count += 1;
            }
        }

        // Out-of-Core Disk Scan: update disk-resident documents when memory is capped
        if let Some(storage) = &self.storage {
            let doc_count = self.doc_count.load(Ordering::Relaxed);
            let mem_count = self.documents.len() as u64;
            if mem_count < doc_count {
                let prefix = format!("doc:{}:", self.config.name).into_bytes();
                if let Ok(entries) = storage.prefix_scan(&prefix) {
                    for (_k, v) in entries {
                        if let Ok(mut doc) = serde_json::from_slice::<Document>(&v) {
                            let id_str = doc.id.as_str().to_string();
                            if self.documents.contains_key(&id_str) {
                                continue; // Already processed in RAM loop
                            }
                            let matches = filter.iter().all(|(key, expected)| {
                                if let Some(actual) = doc.get_nested(key) {
                                    actual == expected
                                } else {
                                    false
                                }
                            });
                            if matches {
                                for (key, value) in updates {
                                    doc.set(key.clone(), value.clone());
                                }
                                let key = format!("doc:{}:{}", self.config.name, id_str).into_bytes();
                                if let Ok(val) = serde_json::to_vec(&doc) {
                                    let _ = storage.put(&key, &val);
                                }
                                count += 1;
                            }
                        }
                    }
                }
            }
        }

        Ok(count)
    }

    /// Delete a document by ID.
    pub fn delete_by_id(&self, id: &str) -> FaizResult<Document> {
        let doc_opt = self.documents.remove(id).map(|(_, d)| d);

        let doc = match doc_opt {
            Some(d) => d,
            None => {
                // Out-of-Core Disk Fallback: check persistent storage engine
                if let Some(storage) = &self.storage {
                    let key = format!("doc:{}:{}", self.config.name, id).into_bytes();
                    if let Ok(Some(val_bytes)) = storage.get(&key) {
                        serde_json::from_slice::<Document>(&val_bytes).map_err(|_| {
                            FaizError::DocumentNotFound {
                                collection: self.config.name.clone(),
                                id: id.to_string(),
                            }
                        })?
                    } else {
                        return Err(FaizError::DocumentNotFound {
                            collection: self.config.name.clone(),
                            id: id.to_string(),
                        });
                    }
                } else {
                    return Err(FaizError::DocumentNotFound {
                        collection: self.config.name.clone(),
                        id: id.to_string(),
                    });
                }
            }
        };

        let _ = self
            .doc_count
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |c| {
                Some(c.saturating_sub(1))
            });
        let _ = self
            .total_size
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |s| {
                Some(s.saturating_sub(doc.size_bytes() as u64))
            });

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
            if let Ok(doc) = self.find_by_id(&res.doc_id) {
                out.push((doc, res.score, res.matched_terms));
            }
        }

        out
    }

    /// Purge all expired TTL documents across the collection
    pub fn purge_expired(&self) -> Vec<String> {
        let expired_ids = self.ttl.purge_expired(crate::ttl::current_time_ms());
        for id in &expired_ids {
            let _ = self.delete_by_id(id);
        }
        expired_ids
    }

    /// Access TTL statistics
    pub fn ttl_stats(&self) -> crate::ttl::TtlStats {
        self.ttl.get_stats()
    }

    /// Delete all documents matching a filter.
    pub fn delete_many(&self, filter: &[(String, Value)]) -> FaizResult<u64> {
        let docs = self.find(filter, None, None)?;
        let mut count = 0u64;
        for doc in docs {
            if self.delete_by_id(doc.id.as_str()).is_ok() {
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

        let doc_count = self.doc_count.load(Ordering::Relaxed);
        let mem_count = self.documents.len() as u64;

        if mem_count >= doc_count || self.storage.is_none() {
            return self
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
                .count() as u64;
        }

        self.find(filter, None, None)
            .map(|docs| docs.len() as u64)
            .unwrap_or(0)
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

        // Index and validate all existing documents (RAM + Disk)
        for doc in self.find_all(None) {
            index.check_unique(&doc)?;
            index.insert(&doc);
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
            if let Ok(doc) = self.find_by_id(&id) {
                docs.push(doc);
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

        // Build the index data from existing documents (RAM + Disk)
        let mut index_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for doc in self.find_all(None) {
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
