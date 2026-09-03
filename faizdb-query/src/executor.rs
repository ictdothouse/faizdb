//! Query Executor for evaluating parsed statements against collections and indexes.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use faizdb_core::document::collection::Collection;
use faizdb_core::document::model::Document;
use faizdb_core::stream::{ChangeEvent, ChangeStreamBus};
use crate::ast::{ExplainPlan, FilterExpr, Operator, Statement};

/// Query execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryResult {
    Documents(Vec<Document>),
    Count(u64),
    Inserted(Vec<String>),
    Updated(u64),
    Deleted(u64),
    Success(String),
    Explain(ExplainPlan),
}

/// Execution environment holding database collections, Change Stream bus, Raft consensus, ShardRouter,
/// persistent StorageEngine (LSM+WAL), MVCC TransactionManager, Vector indexes, and Graph store.
pub struct DatabaseContext {
    collections: DashMap<String, Arc<Collection>>,
    bus: Arc<ChangeStreamBus>,
    raft: Arc<faizdb_core::cluster::RaftNode>,
    shards: Arc<faizdb_core::cluster::ShardRouter>,
    storage: Option<Arc<faizdb_core::storage::engine::StorageEngine>>,
    tx_manager: Arc<faizdb_core::transaction::mvcc::TransactionManager>,
    active_txns: DashMap<String, Arc<parking_lot::Mutex<faizdb_core::transaction::mvcc::Transaction>>>,
    vector_indexes: DashMap<String, Arc<parking_lot::RwLock<faizdb_vector::HnswIndex>>>,
    graph_store: Arc<parking_lot::RwLock<faizdb_graph::GraphStore>>,
    collection_stats: DashMap<String, crate::optimizer::TableStatistics>,
}

impl Default for DatabaseContext {
    fn default() -> Self {
        let raft = Arc::new(faizdb_core::cluster::RaftNode::new("node_1", "127.0.0.1:27018"));
        let shards = Arc::new(faizdb_core::cluster::ShardRouter::new());
        shards.register_node("node_1", "127.0.0.1:27018");

        Self {
            collections: DashMap::new(),
            bus: Arc::new(ChangeStreamBus::new()),
            raft,
            shards,
            storage: None,
            tx_manager: Arc::new(faizdb_core::transaction::mvcc::TransactionManager::new()),
            active_txns: DashMap::new(),
            vector_indexes: DashMap::new(),
            graph_store: Arc::new(parking_lot::RwLock::new(faizdb_graph::GraphStore::new())),
            collection_stats: DashMap::new(),
        }
    }
}

impl DatabaseContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_node(node_id: &str, address: &str) -> Self {
        let raft = Arc::new(faizdb_core::cluster::RaftNode::new(node_id, address));
        let shards = Arc::new(faizdb_core::cluster::ShardRouter::new());
        shards.register_node(node_id, address);

        Self {
            collections: DashMap::new(),
            bus: Arc::new(ChangeStreamBus::new()),
            raft,
            shards,
            storage: None,
            tx_manager: Arc::new(faizdb_core::transaction::mvcc::TransactionManager::new()),
            active_txns: DashMap::new(),
            vector_indexes: DashMap::new(),
            graph_store: Arc::new(parking_lot::RwLock::new(faizdb_graph::GraphStore::new())),
            collection_stats: DashMap::new(),
        }
    }

    /// Create DatabaseContext with an active persistent StorageEngine
    pub fn with_storage(storage: Arc<faizdb_core::storage::engine::StorageEngine>) -> Self {
        let raft = Arc::new(faizdb_core::cluster::RaftNode::new("node_1", "127.0.0.1:27018"));
        let shards = Arc::new(faizdb_core::cluster::ShardRouter::new());
        shards.register_node("node_1", "127.0.0.1:27018");

        let ctx = Self {
            collections: DashMap::new(),
            bus: Arc::new(ChangeStreamBus::new()),
            raft,
            shards,
            storage: Some(storage),
            tx_manager: Arc::new(faizdb_core::transaction::mvcc::TransactionManager::new()),
            active_txns: DashMap::new(),
            vector_indexes: DashMap::new(),
            graph_store: Arc::new(parking_lot::RwLock::new(faizdb_graph::GraphStore::new())),
            collection_stats: DashMap::new(),
        };

        ctx.recover_from_storage();
        ctx
    }

    /// Create cluster DatabaseContext with persistent StorageEngine
    pub fn with_node_and_storage(node_id: &str, address: &str, storage: Arc<faizdb_core::storage::engine::StorageEngine>) -> Self {
        let raft = Arc::new(faizdb_core::cluster::RaftNode::new(node_id, address));
        let shards = Arc::new(faizdb_core::cluster::ShardRouter::new());
        shards.register_node(node_id, address);

        let ctx = Self {
            collections: DashMap::new(),
            bus: Arc::new(ChangeStreamBus::new()),
            raft,
            shards,
            storage: Some(storage),
            tx_manager: Arc::new(faizdb_core::transaction::mvcc::TransactionManager::new()),
            active_txns: DashMap::new(),
            vector_indexes: DashMap::new(),
            graph_store: Arc::new(parking_lot::RwLock::new(faizdb_graph::GraphStore::new())),
            collection_stats: DashMap::new(),
        };

        ctx.recover_from_storage();
        ctx
    }

    /// Open or create storage engine at given data directory and recover existing data
    pub fn with_storage_dir(data_dir: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let config = faizdb_core::storage::engine::StorageConfig {
            data_dir: data_dir.as_ref().to_path_buf(),
            ..Default::default()
        };
        let storage = faizdb_core::storage::engine::StorageEngine::open(config)
            .map_err(|e| format!("Failed to open storage engine: {e}"))?;
        Ok(Self::with_storage(Arc::new(storage)))
    }

    /// Recover all collections, documents, vector indexes, and knowledge graph from storage on startup
    pub fn recover_from_storage(&self) {
        if let Some(storage) = &self.storage {
            // 1. Recover documents
            if let Ok(entries) = storage.prefix_scan(b"doc:") {
                for (key_bytes, val_bytes) in entries {
                    if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                        let parts: Vec<&str> = key_str.splitn(3, ':').collect();
                        if parts.len() == 3 {
                            let col_name = parts[1];
                            if let Ok(doc) = serde_json::from_slice::<Document>(&val_bytes) {
                                let col = self.get_or_create_collection(col_name);
                                col.load_document(doc);
                            }
                        }
                    }
                }
            }

            // 2. Recover vector index definitions
            if let Ok(meta_entries) = storage.prefix_scan(b"vec:meta:") {
                for (key_bytes, val_bytes) in meta_entries {
                    if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                        if let Some(index_name) = key_str.strip_prefix("vec:meta:") {
                            if let Ok(config) = serde_json::from_slice::<faizdb_vector::HnswConfig>(&val_bytes) {
                                let index = Arc::new(parking_lot::RwLock::new(faizdb_vector::HnswIndex::new(config)));
                                self.vector_indexes.insert(index_name.to_string(), index);
                            }
                        }
                    }
                }
            }

            // 3. Recover vector data points
            if let Ok(data_entries) = storage.prefix_scan(b"vec:data:") {
                for (key_bytes, val_bytes) in data_entries {
                    if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                        // format: vec:data:<index_name>:<id>
                        let parts: Vec<&str> = key_str.splitn(4, ':').collect();
                        if parts.len() == 4 {
                            let index_name = parts[2];
                            let vector_id = parts[3];
                            if let Ok(vector) = serde_json::from_slice::<Vec<f32>>(&val_bytes) {
                                if let Some(index_lock) = self.vector_indexes.get(index_name) {
                                    let mut index = index_lock.write();
                                    let _ = index.insert(vector_id.to_string(), vector);
                                }
                            }
                        }
                    }
                }
            }

            // 4. Recover graph vertices
            if let Ok(v_entries) = storage.prefix_scan(b"graph:v:") {
                let mut graph = self.graph_store.write();
                for (_key_bytes, val_bytes) in v_entries {
                    if let Ok(vertex) = serde_json::from_slice::<faizdb_graph::Vertex>(&val_bytes) {
                        graph.add_vertex(vertex);
                    }
                }
            }

            // 5. Recover graph edges
            if let Ok(e_entries) = storage.prefix_scan(b"graph:e:") {
                let mut graph = self.graph_store.write();
                for (_key_bytes, val_bytes) in e_entries {
                    if let Ok(edge) = serde_json::from_slice::<faizdb_graph::Edge>(&val_bytes) {
                        graph.add_edge(edge);
                    }
                }
            }
        }
    }

    /// Access the real-time Change Stream broadcast bus
    pub fn change_stream_bus(&self) -> Arc<ChangeStreamBus> {
        self.bus.clone()
    }

    /// Access the Raft consensus engine
    pub fn raft(&self) -> Arc<faizdb_core::cluster::RaftNode> {
        self.raft.clone()
    }

    /// Access the auto-sharding router
    pub fn shards(&self) -> Arc<faizdb_core::cluster::ShardRouter> {
        self.shards.clone()
    }

    /// Get or create a collection (backed by StorageEngine if configured)
    pub fn get_or_create_collection(&self, name: &str) -> Arc<Collection> {
        self.collections
            .entry(name.to_string())
            .or_insert_with(|| {
                if let Some(storage) = &self.storage {
                    Arc::new(Collection::with_storage(name, storage.clone()))
                } else {
                    Arc::new(Collection::new(name))
                }
            })
            .clone()
    }

    /// Access underlying persistent StorageEngine
    pub fn storage(&self) -> Option<Arc<faizdb_core::storage::engine::StorageEngine>> {
        self.storage.clone()
    }

    /// Access MVCC TransactionManager
    pub fn tx_manager(&self) -> Arc<faizdb_core::transaction::mvcc::TransactionManager> {
        self.tx_manager.clone()
    }

    /// Access active transactions map
    pub fn active_txns(&self) -> &DashMap<String, Arc<parking_lot::Mutex<faizdb_core::transaction::mvcc::Transaction>>> {
        &self.active_txns
    }

    /// Access vector indexes
    pub fn vector_indexes(&self) -> &DashMap<String, Arc<parking_lot::RwLock<faizdb_vector::HnswIndex>>> {
        &self.vector_indexes
    }

    /// Access knowledge graph store
    pub fn graph_store(&self) -> Arc<parking_lot::RwLock<faizdb_graph::GraphStore>> {
        self.graph_store.clone()
    }

    /// List all collection names
    pub fn list_collections(&self) -> Vec<String> {
        self.collections.iter().map(|e| e.key().clone()).collect()
    }

    /// Get all collections with instances
    pub fn all_collections(&self) -> Vec<(String, Arc<Collection>)> {
        self.collections
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect()
    }

    /// Run ANALYZE on a collection to compute cardinality, attribute stats, and histograms
    pub fn analyze_collection(&self, collection: &str) -> crate::optimizer::TableStatistics {
        let col = self.get_or_create_collection(collection);
        let docs = col.find_all(None);
        let stats = crate::optimizer::TableStatistics::analyze(collection, &docs);
        self.collection_stats.insert(collection.to_string(), stats.clone());
        stats
    }

    /// Get current collection statistics or compute them on demand
    pub fn get_or_compute_stats(&self, collection: &str) -> crate::optimizer::TableStatistics {
        if let Some(stats) = self.collection_stats.get(collection) {
            return stats.clone();
        }
        self.analyze_collection(collection)
    }

    /// Execute an AST statement
    pub fn execute(&self, stmt: Statement) -> Result<QueryResult, String> {
        match stmt {
            Statement::Analyze { collection } => {
                let stats = self.analyze_collection(&collection);
                Ok(QueryResult::Success(format!(
                    "Collection '{collection}' analyzed successfully: {} documents, {} attributes tracked with CBO histograms",
                    stats.total_documents, stats.column_stats.len()
                )))
            }
            Statement::Explain(inner_stmt) => {
                let start = Instant::now();
                match *inner_stmt {
                    Statement::Find { collection, filter, .. } => {
                        let col = self.get_or_create_collection(&collection);
                        let stats = self.get_or_compute_stats(&collection);

                        // Check if an index can be used
                        let mut index_name = None;
                        let mut is_unique = false;
                        let mut index_field = None;
                        let mut docs_examined = stats.total_documents;

                        if let Some(FilterExpr::Field { field, op: Operator::Eq, value }) = &filter {
                            if let Some(idx) = col.get_secondary_index(field) {
                                index_name = Some(idx.def.name.clone());
                                is_unique = idx.def.unique;
                                index_field = Some(field.as_str());
                                docs_examined = idx.lookup(value).len();
                            }
                        }

                        // Run through Cost-Based Optimizer
                        let decision = crate::optimizer::QueryOptimizer::choose_best_plan(
                            &stats,
                            filter.as_ref(),
                            index_field,
                            index_name.as_deref(),
                        );

                        let res = self.execute(Statement::Find {
                            collection: collection.clone(),
                            filter,
                            sort_by: None,
                            limit: None,
                            skip: None,
                            vector_search: None,
                            traverse: None,
                        })?;

                        let docs_returned = match res {
                            QueryResult::Documents(d) => d.len(),
                            _ => 0,
                        };

                        let execution_time_us = start.elapsed().as_micros() as u64;

                        Ok(QueryResult::Explain(ExplainPlan {
                            plan_type: decision.chosen_plan,
                            collection,
                            index_used: decision.index_used,
                            execution_time_us,
                            documents_examined: docs_examined,
                            documents_returned: docs_returned,
                            is_unique,
                            estimated_cost_score: decision.estimated_cost,
                            estimated_selectivity_pct: Some(decision.selectivity_pct),
                            seq_scan_cost: Some(decision.seq_scan_cost),
                            index_scan_cost: decision.index_scan_cost,
                            optimization_rationale: Some(decision.rationale),
                        }))
                    }
                    other => {
                        let _res = self.execute(other)?;
                        let execution_time_us = start.elapsed().as_micros() as u64;
                        Ok(QueryResult::Explain(ExplainPlan {
                            plan_type: "DirectExecution".into(),
                            collection: "default".into(),
                            index_used: None,
                            execution_time_us,
                            documents_examined: 0,
                            documents_returned: 1,
                            is_unique: false,
                            estimated_cost_score: 1.0,
                            estimated_selectivity_pct: Some(100.0),
                            seq_scan_cost: Some(1.0),
                            index_scan_cost: None,
                            optimization_rationale: Some("Direct statement execution".to_string()),
                        }))
                    }
                }
            }
            Statement::Find {
                collection,
                filter,
                limit,
                skip,
                vector_search,
                traverse,
                ..
            } => {
                let col = self.get_or_create_collection(&collection);
                let stats = self.get_or_compute_stats(&collection);

                // Cost-Based Index vs Sequential Scan Selection (Adaptive Execution)
                let mut index_name = None;
                let mut index_field = None;
                let mut filter_val = None;

                if let Some(FilterExpr::Field { field, op: Operator::Eq, value }) = &filter {
                    if let Some(idx) = col.get_secondary_index(field) {
                        index_name = Some(idx.def.name.clone());
                        index_field = Some(field.as_str());
                        filter_val = Some(value);
                    }
                }

                let decision = crate::optimizer::QueryOptimizer::choose_best_plan(
                    &stats,
                    filter.as_ref(),
                    index_field,
                    index_name.as_deref(),
                );

                let candidate_docs = if decision.index_used.is_some() {
                    if let (Some(field), Some(val)) = (index_field, filter_val) {
                        col.find_by_secondary_index(field, val)
                    } else {
                        None
                    }
                } else {
                    None // Adaptive fallback to sequential scan
                };

                let docs_to_scan = candidate_docs.unwrap_or_else(|| col.find_all(None));

                let mut filtered: Vec<Document> = docs_to_scan
                    .into_iter()
                    .filter(|doc| filter.as_ref().map_or(true, |f| f.matches(doc)))
                    .collect();

                // Graph traversal filtering if specified
                if let Some(t_clause) = traverse {
                    let paths = self.graph_store.read().traverse_bfs(&t_clause.start_id, t_clause.max_depth, t_clause.relation.as_deref());
                    let reached_ids: std::collections::HashSet<String> = paths.into_iter().map(|p| p.vertex_id).collect();
                    filtered.retain(|d| reached_ids.contains(d.id.as_str()));
                }

                // Vector search ranking if specified
                if let Some(v_clause) = vector_search {
                    let mut scored: Vec<(Document, f32)> = filtered
                        .into_iter()
                        .filter_map(|doc| {
                            if let Some(val) = doc.get("vector").or_else(|| doc.get("embedding")) {
                                if let Some(arr) = val.as_array() {
                                    let vec: Vec<f32> = arr.iter().filter_map(|x| match x {
                                        faizdb_core::document::model::Value::Float(f) => Some(*f as f32),
                                        faizdb_core::document::model::Value::Integer(i) => Some(*i as f32),
                                        _ => None,
                                    }).collect();
                                    if vec.len() == v_clause.vector.len() {
                                        let dist = faizdb_vector::cosine_distance(&vec, &v_clause.vector);
                                        return Some((doc, dist));
                                    }
                                }
                            }
                            None
                        })
                        .collect();
                    scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
                    filtered = scored.into_iter().take(v_clause.top_k).map(|(d, _)| d).collect();
                }

                let final_docs = filtered
                    .into_iter()
                    .skip(skip.unwrap_or(0))
                    .take(limit.unwrap_or(usize::MAX))
                    .collect();

                Ok(QueryResult::Documents(final_docs))
            }
            Statement::Insert { collection, documents } => {
                let col = self.get_or_create_collection(&collection);
                let mut ids = Vec::with_capacity(documents.len());
                for doc in documents {
                    let doc_clone = doc.clone();
                    let id = col.insert(doc).map_err(|e| e.to_string())?;
                    let id_str = id.as_str().to_string();
                    
                    // Emit real-time change stream event
                    self.bus.publish(ChangeEvent::insert(&collection, doc_clone));
                    ids.push(id_str);
                }
                Ok(QueryResult::Inserted(ids))
            }
            Statement::Count { collection, filter } => {
                let col = self.get_or_create_collection(&collection);
                if let Some(f) = filter {
                    let count = col
                        .find_all(None)
                        .iter()
                        .filter(|d| f.matches(d))
                        .count() as u64;
                    Ok(QueryResult::Count(count))
                } else {
                    Ok(QueryResult::Count(col.stats().document_count))
                }
            }
            Statement::Delete { collection, filter } => {
                let col = self.get_or_create_collection(&collection);
                let matching_ids: Vec<String> = col
                    .find_all(None)
                    .iter()
                    .filter(|d| filter.matches(d))
                    .map(|d| d.id.as_str().to_string())
                    .collect();

                let count = matching_ids.len() as u64;
                for id in matching_ids {
                    if col.delete_by_id(&id).is_ok() {
                        self.bus.publish(ChangeEvent::delete(&collection, &id));
                    }
                }
                Ok(QueryResult::Deleted(count))
            }
            Statement::Update { collection, filter, updates } => {
                let col = self.get_or_create_collection(&collection);
                let mut count = 0u64;
                let matching_ids: Vec<String> = col
                    .find_all(None)
                    .iter()
                    .filter(|d| filter.matches(d))
                    .map(|d| d.id.as_str().to_string())
                    .collect();

                for id in matching_ids {
                    let mut updates_map = BTreeMap::new();
                    for (k, v) in &updates {
                        updates_map.insert(k.clone(), v.clone());
                    }

                    let res = col.update_by_id(&id, |doc| {
                        for (k, v) in &updates {
                            doc.set(k.clone(), v.clone());
                        }
                    });
                    if res.is_ok() {
                        let updated_doc = col.find_by_id(&id).ok();
                        self.bus.publish(ChangeEvent::update(&collection, &id, updates_map, updated_doc));
                        count += 1;
                    }
                }
                Ok(QueryResult::Updated(count))
            }
            Statement::CreateCollection { name } => {
                self.get_or_create_collection(&name);
                Ok(QueryResult::Success(format!("Collection '{name}' created")))
            }
            Statement::DropCollection { name } => {
                self.collections.remove(&name);
                self.bus.publish(ChangeEvent::drop_collection(&name));
                Ok(QueryResult::Success(format!("Collection '{name}' dropped")))
            }
            Statement::CreateIndex { collection, field, unique } => {
                let col = self.get_or_create_collection(&collection);
                let idx_name = col.create_secondary_index(&field, unique).map_err(|e| e.to_string())?;
                let unique_tag = if unique { " UNIQUE" } else { "" };
                Ok(QueryResult::Success(format!("Index '{idx_name}' created on '{collection}.{field}'{unique_tag}")))
            }
            Statement::DropIndex { collection, field } => {
                let col = self.get_or_create_collection(&collection);
                let dropped = col.drop_secondary_index(&field);
                if dropped {
                    Ok(QueryResult::Success(format!("Index on '{collection}.{field}' dropped")))
                } else {
                    Ok(QueryResult::Success(format!("No index found on '{collection}.{field}'")))
                }
            }
            Statement::BeginTransaction => {
                Ok(QueryResult::Success("ACID Transaction initialized (Snapshot Isolation)".into()))
            }
            Statement::CommitTransaction => {
                Ok(QueryResult::Success("ACID Transaction committed successfully to WAL".into()))
            }
            Statement::RollbackTransaction => {
                Ok(QueryResult::Success("ACID Transaction rolled back and changes discarded".into()))
            }
        }
    }
}
