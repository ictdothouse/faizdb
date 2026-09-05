//! Query Executor for evaluating parsed statements against collections and indexes.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;

use crate::ast::{ExplainPlan, FilterExpr, Operator, Statement};
use faizdb_core::document::collection::Collection;
use faizdb_core::document::model::{Document, Value};
use faizdb_core::stream::{ChangeEvent, ChangeStreamBus};

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
    active_txns:
        DashMap<String, Arc<parking_lot::Mutex<faizdb_core::transaction::mvcc::Transaction>>>,
    vector_indexes: DashMap<String, Arc<parking_lot::RwLock<faizdb_vector::HnswIndex>>>,
    graph_store: Arc<parking_lot::RwLock<faizdb_graph::GraphStore>>,
    semantic_cache: Arc<faizdb_graph::SemanticCache>,
    collection_stats: DashMap<String, crate::optimizer::TableStatistics>,
}

impl Default for DatabaseContext {
    fn default() -> Self {
        let raft = Arc::new(faizdb_core::cluster::RaftNode::new(
            "node_1",
            "127.0.0.1:27018",
        ));
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
            semantic_cache: Arc::new(faizdb_graph::SemanticCache::default()),
            collection_stats: DashMap::new(),
        }
    }
}

impl DatabaseContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_node(node_id: &str, address: &str) -> Result<Self, String> {
        let storage_path = format!("faizdb_node_{}_data", node_id);
        let storage = faizdb_core::storage::engine::StorageEngine::open_default(&storage_path)
            .map_err(|e| format!("Failed to open node storage engine at '{storage_path}': {e}"))
            .map(Arc::new)?;

        let shards = Arc::new(faizdb_core::cluster::ShardRouter::new());
        shards.register_node(node_id, address);

        let ctx = Self {
            collections: DashMap::new(),
            bus: Arc::new(ChangeStreamBus::new()),
            raft: Arc::new(faizdb_core::cluster::RaftNode::new(node_id, address)),
            shards,
            storage: Some(storage),
            tx_manager: Arc::new(faizdb_core::transaction::mvcc::TransactionManager::new()),
            active_txns: DashMap::new(),
            vector_indexes: DashMap::new(),
            graph_store: Arc::new(parking_lot::RwLock::new(faizdb_graph::GraphStore::new())),
            semantic_cache: Arc::new(faizdb_graph::SemanticCache::default()),
            collection_stats: DashMap::new(),
        };
        ctx.recover_from_storage()?;
        Ok(ctx)
    }

    /// Create DatabaseContext with persistent StorageEngine and propagate recovery errors
    pub fn try_with_storage(
        storage: Arc<faizdb_core::storage::engine::StorageEngine>,
    ) -> Result<Self, String> {
        let raft = Arc::new(faizdb_core::cluster::RaftNode::new(
            "node_1",
            "127.0.0.1:27018",
        ));
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
            semantic_cache: Arc::new(faizdb_graph::SemanticCache::default()),
            collection_stats: DashMap::new(),
        };

        ctx.recover_from_storage()?;
        Ok(ctx)
    }

    /// Create DatabaseContext with an active persistent StorageEngine
    pub fn with_storage(storage: Arc<faizdb_core::storage::engine::StorageEngine>) -> Self {
        let raft = Arc::new(faizdb_core::cluster::RaftNode::new(
            "node_1",
            "127.0.0.1:27018",
        ));
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
            semantic_cache: Arc::new(faizdb_graph::SemanticCache::default()),
            collection_stats: DashMap::new(),
        };

        let _ = ctx.recover_from_storage();
        ctx
    }

    /// Create cluster DatabaseContext with persistent StorageEngine
    pub fn with_node_and_storage(
        node_id: &str,
        address: &str,
        storage: Arc<faizdb_core::storage::engine::StorageEngine>,
    ) -> Self {
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
            semantic_cache: Arc::new(faizdb_graph::SemanticCache::default()),
            collection_stats: DashMap::new(),
        };

        let _ = ctx.recover_from_storage();
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
        Self::try_with_storage(Arc::new(storage))
    }

    /// Recover all collections, documents, vector indexes, and knowledge graph from storage on startup
    pub fn recover_from_storage(&self) -> Result<(), String> {
        if let Some(storage) = &self.storage {
            // 1. Recover documents
            let entries = storage
                .prefix_scan(b"doc:")
                .map_err(|e| format!("Failed to scan documents from storage: {e}"))?;
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

            // 2. Recover vector index definitions
            let meta_entries = storage
                .prefix_scan(b"vec:meta:")
                .map_err(|e| format!("Failed to scan vector index metadata from storage: {e}"))?;
            for (key_bytes, val_bytes) in meta_entries {
                if let Ok(key_str) = std::str::from_utf8(&key_bytes) {
                    if let Some(index_name) = key_str.strip_prefix("vec:meta:") {
                        if let Ok(config) =
                            serde_json::from_slice::<faizdb_vector::HnswConfig>(&val_bytes)
                        {
                            let index = Arc::new(parking_lot::RwLock::new(
                                faizdb_vector::HnswIndex::new(config),
                            ));
                            self.vector_indexes.insert(index_name.to_string(), index);
                        }
                    }
                }
            }

            // 3. Recover vector data points
            let data_entries = storage
                .prefix_scan(b"vec:data:")
                .map_err(|e| format!("Failed to scan vector data points from storage: {e}"))?;
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

            // 4. Recover graph vertices
            let v_entries = storage
                .prefix_scan(b"graph:v:")
                .map_err(|e| format!("Failed to scan graph vertices from storage: {e}"))?;
            {
                let mut graph = self.graph_store.write();
                for (_key_bytes, val_bytes) in v_entries {
                    if let Ok(vertex) = serde_json::from_slice::<faizdb_graph::Vertex>(&val_bytes) {
                        graph.add_vertex(vertex);
                    }
                }
            }

            // 5. Recover graph edges
            let e_entries = storage
                .prefix_scan(b"graph:e:")
                .map_err(|e| format!("Failed to scan graph edges from storage: {e}"))?;
            {
                let mut graph = self.graph_store.write();
                for (_key_bytes, val_bytes) in e_entries {
                    if let Ok(edge) = serde_json::from_slice::<faizdb_graph::Edge>(&val_bytes) {
                        graph.add_edge(edge);
                    }
                }
            }
        }
        Ok(())
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

    /// Drop a collection from DatabaseContext
    pub fn drop_collection(&self, name: &str) -> bool {
        let removed = self.collections.remove(name).is_some();
        if removed {
            self.bus.publish(ChangeEvent::drop_collection(name));
        }
        removed
    }

    /// Trigger LSM-Tree compaction on persistent storage engine if open
    pub fn compact(&self) -> faizdb_core::error::FaizResult<usize> {
        if let Some(storage) = &self.storage {
            storage.compact()
        } else {
            Ok(0)
        }
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
    pub fn active_txns(
        &self,
    ) -> &DashMap<String, Arc<parking_lot::Mutex<faizdb_core::transaction::mvcc::Transaction>>>
    {
        &self.active_txns
    }

    /// Reap abandoned/idle transactions exceeding the timeout limit.
    /// Returns the number of expired transactions safely aborted and removed.
    pub fn reap_expired_transactions(&self, timeout: std::time::Duration) -> usize {
        let mut expired = Vec::new();
        for entry in self.active_txns.iter() {
            let txn = entry.value().lock();
            if txn.is_expired(timeout) {
                expired.push(entry.key().clone());
            }
        }
        let count = expired.len();
        for id in expired {
            if let Some((_, m)) = self.active_txns.remove(&id) {
                let mut txn = m.lock();
                let _ = self.tx_manager.abort(&mut txn);
                tracing::warn!(
                    "Reaped expired idle transaction '{id}' after timeout of {}s",
                    timeout.as_secs()
                );
            }
        }
        count
    }

    /// Explicitly flush in-memory data (MemTable) and fsync WAL to disk for clean shutdown.
    pub fn flush(&self) -> Result<(), String> {
        if let Some(storage) = &self.storage {
            storage
                .close()
                .map_err(|e| format!("Failed to flush storage: {e}"))?;
        }
        Ok(())
    }

    /// Access vector indexes
    pub fn vector_indexes(
        &self,
    ) -> &DashMap<String, Arc<parking_lot::RwLock<faizdb_vector::HnswIndex>>> {
        &self.vector_indexes
    }

    /// Access knowledge graph store
    pub fn graph_store(&self) -> Arc<parking_lot::RwLock<faizdb_graph::GraphStore>> {
        self.graph_store.clone()
    }

    /// Access semantic cache for GraphRAG and vector prompt embeddings
    pub fn semantic_cache(&self) -> Arc<faizdb_graph::SemanticCache> {
        self.semantic_cache.clone()
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
        self.collection_stats
            .insert(collection.to_string(), stats.clone());
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
                    Statement::Find {
                        collection, filter, ..
                    } => {
                        let col = self.get_or_create_collection(&collection);
                        let stats = self.get_or_compute_stats(&collection);

                        // Check if an index can be used
                        let mut index_name = None;
                        let mut is_unique = false;
                        let mut index_field = None;
                        let mut docs_examined = stats.total_documents;

                        if let Some(FilterExpr::Field {
                            field,
                            op: Operator::Eq,
                            value,
                        }) = &filter
                        {
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
                            joins: Vec::new(),
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
                sort_by,
                limit,
                skip,
                vector_search,
                traverse,
                joins,
            } => {
                let col = self.get_or_create_collection(&collection);
                let stats = self.get_or_compute_stats(&collection);

                // Cost-Based Index vs Sequential Scan Selection (Adaptive Execution)
                let mut index_name = None;
                let mut index_field = None;
                let mut filter_val = None;

                if let Some(FilterExpr::Field {
                    field,
                    op: Operator::Eq,
                    value,
                }) = &filter
                {
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

                let docs_to_scan = candidate_docs.unwrap_or_else(|| {
                    // Limit pushdown: If there are no filters, no sorts, no joins, and no vector/graph clauses,
                    // avoid allocating the full collection into memory by scanning only the required limit + skip items.
                    if filter.is_none()
                        && sort_by.is_none()
                        && joins.is_empty()
                        && vector_search.is_none()
                        && traverse.is_none()
                    {
                        if let Some(l) = limit {
                            col.find_all(Some(skip.unwrap_or(0).saturating_add(l)))
                        } else {
                            col.find_all(None)
                        }
                    } else {
                        col.find_all(None)
                    }
                });

                let mut filtered: Vec<Document> = docs_to_scan
                    .into_iter()
                    .filter(|doc| filter.as_ref().is_none_or(|f| f.matches(doc)))
                    .collect();

                // Graph traversal filtering if specified
                if let Some(ref t_clause) = traverse {
                    let paths = self.graph_store.read().traverse_bfs(
                        &t_clause.start_id,
                        t_clause.max_depth,
                        t_clause.relation.as_deref(),
                    );
                    let reached_ids: std::collections::HashSet<String> =
                        paths.into_iter().map(|p| p.vertex_id).collect();
                    filtered.retain(|d| reached_ids.contains(d.id.as_str()));
                }

                // Vector search ranking if specified
                if let Some(v_clause) = vector_search {
                    // Check if an accelerated HNSW index exists:
                    // 1. Check v_clause.index_name if explicitly provided
                    // 2. Otherwise check collection name
                    // 3. Otherwise if there is exactly 1 index registered, use it as fallback
                    let resolved_idx_opt = if let Some(ref custom_idx) = v_clause.index_name {
                        if let Some(idx_lock) = self.vector_indexes.get(custom_idx) {
                            Some(idx_lock.clone())
                        } else {
                            return Err(format!("Vector index '{custom_idx}' not found"));
                        }
                    } else if let Some(idx_lock) = self.vector_indexes.get(&collection) {
                        Some(idx_lock.clone())
                    } else if self.vector_indexes.len() == 1 {
                        self.vector_indexes
                            .iter()
                            .next()
                            .map(|entry| entry.value().clone())
                    } else {
                        None
                    };

                    if let Some(idx_lock) = resolved_idx_opt {
                        let idx = idx_lock.read();
                        if v_clause.vector.len() != idx.config.dimensions {
                            return Err(format!(
                                "Query vector dimension mismatch: expected {}, got {}",
                                idx.config.dimensions,
                                v_clause.vector.len()
                            ));
                        }
                        let candidate_k = if filter.is_some() || traverse.is_some() {
                            std::cmp::max(v_clause.top_k * 10, 100)
                                .min(idx.len().max(v_clause.top_k))
                        } else {
                            v_clause.top_k
                        };
                        let hits = idx.search(&v_clause.vector, candidate_k);
                        let id_rank: std::collections::HashMap<String, usize> = hits
                            .into_iter()
                            .enumerate()
                            .map(|(rank, hit)| (hit.id, rank))
                            .collect();

                        filtered.retain(|d| id_rank.contains_key(d.id.as_str()));
                        filtered.sort_by_key(|d| {
                            id_rank.get(d.id.as_str()).copied().unwrap_or(usize::MAX)
                        });
                        filtered.truncate(v_clause.top_k);
                    } else {
                        let mut scored: Vec<(Document, f32)> = filtered
                            .into_iter()
                            .filter_map(|doc| {
                                if let Some(val) =
                                    doc.get("vector").or_else(|| doc.get("embedding"))
                                {
                                    if let Some(arr) = val.as_array() {
                                        let vec: Vec<f32> = arr
                                            .iter()
                                            .filter_map(|x| match x {
                                                faizdb_core::document::model::Value::Float(f) => {
                                                    Some(*f as f32)
                                                }
                                                faizdb_core::document::model::Value::Integer(i) => {
                                                    Some(*i as f32)
                                                }
                                                _ => None,
                                            })
                                            .collect();
                                        if vec.len() == v_clause.vector.len() {
                                            let dist = faizdb_vector::cosine_distance(
                                                &vec,
                                                &v_clause.vector,
                                            );
                                            return Some((doc, dist));
                                        }
                                    }
                                }
                                None
                            })
                            .collect();
                        scored.sort_by(|a, b| {
                            a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)
                        });
                        filtered = scored
                            .into_iter()
                            .take(v_clause.top_k)
                            .map(|(d, _)| d)
                            .collect();
                    }
                }

                // Relational JOIN execution (Hash Join)
                for join in &joins {
                    let joined_col = self.get_or_create_collection(&join.collection);
                    let right_docs = joined_col.find_all(None);

                    let left_key = join.on_left.split('.').next_back().unwrap_or(&join.on_left);
                    let right_key = join
                        .on_right
                        .split('.')
                        .next_back()
                        .unwrap_or(&join.on_right);

                    let mut right_hash: std::collections::HashMap<String, Vec<Document>> =
                        std::collections::HashMap::new();
                    for rdoc in right_docs {
                        let key_val_str = match right_key {
                            "id" | "_id" => rdoc.id.as_str().to_string(),
                            other => match rdoc.get_nested(other) {
                                Some(Value::String(s)) => s.clone(),
                                Some(Value::Integer(i)) => i.to_string(),
                                Some(v) => format!("{v:?}"),
                                None => continue,
                            },
                        };
                        right_hash.entry(key_val_str).or_default().push(rdoc);
                    }

                    let mut joined_results = Vec::new();
                    for left_doc in filtered {
                        let left_val_str = match left_key {
                            "id" | "_id" => left_doc.id.as_str().to_string(),
                            other => match left_doc.get_nested(other) {
                                Some(Value::String(s)) => s.clone(),
                                Some(Value::Integer(i)) => i.to_string(),
                                Some(v) => format!("{v:?}"),
                                None => String::new(),
                            },
                        };

                        if let Some(matching_rights) = right_hash.get(&left_val_str) {
                            for right_doc in matching_rights {
                                let mut combined = left_doc.clone();
                                for (k, v) in &right_doc.fields {
                                    combined
                                        .fields
                                        .insert(format!("{}_{}", join.collection, k), v.clone());
                                    if !combined.fields.contains_key(k) {
                                        combined.fields.insert(k.clone(), v.clone());
                                    }
                                }
                                joined_results.push(combined);
                            }
                        } else if join.join_type == crate::ast::JoinType::Left {
                            joined_results.push(left_doc);
                        }
                    }
                    filtered = joined_results;
                }

                // Apply sort_by if specified
                if let Some((ref field, dir)) = sort_by {
                    filtered.sort_by(|a, b| {
                        let va = a.get_nested(field);
                        let vb = b.get_nested(field);
                        let cmp = match (va, vb) {
                            (Some(x), Some(y)) => match (x, y) {
                                (Value::Integer(ix), Value::Integer(iy)) => ix.cmp(iy),
                                (Value::Float(fx), Value::Float(fy)) => {
                                    fx.partial_cmp(fy).unwrap_or(std::cmp::Ordering::Equal)
                                }
                                (Value::Integer(ix), Value::Float(fy)) => (*ix as f64)
                                    .partial_cmp(fy)
                                    .unwrap_or(std::cmp::Ordering::Equal),
                                (Value::Float(fx), Value::Integer(iy)) => fx
                                    .partial_cmp(&(*iy as f64))
                                    .unwrap_or(std::cmp::Ordering::Equal),
                                (Value::String(sx), Value::String(sy)) => sx.cmp(sy),
                                (Value::Boolean(bx), Value::Boolean(by)) => bx.cmp(by),
                                _ => std::cmp::Ordering::Equal,
                            },
                            (Some(_), None) => std::cmp::Ordering::Greater,
                            (None, Some(_)) => std::cmp::Ordering::Less,
                            (None, None) => std::cmp::Ordering::Equal,
                        };
                        if dir < 0 {
                            cmp.reverse()
                        } else {
                            cmp
                        }
                    });
                }

                let final_docs = filtered
                    .into_iter()
                    .skip(skip.unwrap_or(0))
                    .take(limit.unwrap_or(usize::MAX))
                    .collect();

                Ok(QueryResult::Documents(final_docs))
            }
            Statement::Insert {
                collection,
                documents,
            } => {
                let col = self.get_or_create_collection(&collection);
                let mut ids = Vec::with_capacity(documents.len());
                for doc in documents {
                    let doc_clone = doc.clone();
                    let id = col.insert(doc).map_err(|e| e.to_string())?;
                    let id_str = id.as_str().to_string();

                    // Emit real-time change stream event
                    self.bus
                        .publish(ChangeEvent::insert(&collection, doc_clone));
                    ids.push(id_str);
                }
                Ok(QueryResult::Inserted(ids))
            }
            Statement::Count { collection, filter } => {
                let col = self.get_or_create_collection(&collection);
                if let Some(f) = filter {
                    let count = col.find_all(None).iter().filter(|d| f.matches(d)).count() as u64;
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
            Statement::Update {
                collection,
                filter,
                updates,
            } => {
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
                            if let Value::String(s) = v {
                                let trimmed_s = s.trim();
                                if let Some(rest) = trimmed_s.strip_prefix(k.as_str()) {
                                    let rest = rest.trim();
                                    if let Some(num_str) = rest.strip_prefix('+') {
                                        if let Ok(delta) = num_str.trim().parse::<i64>() {
                                            let cur =
                                                doc.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
                                            doc.set(k.clone(), Value::Integer(cur + delta));
                                            continue;
                                        } else if let Ok(delta) = num_str.trim().parse::<f64>() {
                                            let cur =
                                                doc.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                                            doc.set(k.clone(), Value::Float(cur + delta));
                                            continue;
                                        }
                                    } else if let Some(num_str) = rest.strip_prefix('-') {
                                        if let Ok(delta) = num_str.trim().parse::<i64>() {
                                            let cur =
                                                doc.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
                                            doc.set(k.clone(), Value::Integer(cur - delta));
                                            continue;
                                        } else if let Ok(delta) = num_str.trim().parse::<f64>() {
                                            let cur =
                                                doc.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                                            doc.set(k.clone(), Value::Float(cur - delta));
                                            continue;
                                        }
                                    }
                                }
                            }
                            doc.set(k.clone(), v.clone());
                        }
                    });
                    if res.is_ok() {
                        let updated_doc = col.find_by_id(&id).ok();
                        self.bus.publish(ChangeEvent::update(
                            &collection,
                            &id,
                            updates_map,
                            updated_doc,
                        ));
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
            Statement::CreateIndex {
                collection,
                field,
                unique,
            } => {
                let col = self.get_or_create_collection(&collection);
                let idx_name = col
                    .create_secondary_index(&field, unique)
                    .map_err(|e| e.to_string())?;
                let unique_tag = if unique { " UNIQUE" } else { "" };
                Ok(QueryResult::Success(format!(
                    "Index '{idx_name}' created on '{collection}.{field}'{unique_tag}"
                )))
            }
            Statement::DropIndex { collection, field } => {
                let col = self.get_or_create_collection(&collection);
                let dropped = col.drop_secondary_index(&field);
                if dropped {
                    Ok(QueryResult::Success(format!(
                        "Index on '{collection}.{field}' dropped"
                    )))
                } else {
                    Ok(QueryResult::Success(format!(
                        "No index found on '{collection}.{field}'"
                    )))
                }
            }
            Statement::CreateEdge {
                from,
                to,
                relation,
                weight,
                properties,
            } => {
                let mut edge = faizdb_graph::Edge::new(&from, &to, &relation);
                if let Some(w) = weight {
                    edge.weight = w;
                }
                if let Some(props) = properties {
                    edge.properties = props;
                }
                self.graph_store.write().add_edge(edge);
                Ok(QueryResult::Success(format!(
                    "Graph edge from '{from}' to '{to}' via '{relation}' created successfully"
                )))
            }
            Statement::DeleteEdge {
                from,
                to,
                relation,
            } => {
                let removed = self
                    .graph_store
                    .write()
                    .remove_edge(&from, &to, relation.as_deref());
                if removed {
                    Ok(QueryResult::Success(format!(
                        "Graph edge from '{from}' to '{to}' deleted successfully"
                    )))
                } else {
                    Err(format!("Graph edge from '{from}' to '{to}' not found"))
                }
            }
            Statement::BeginTransaction => Ok(QueryResult::Success(
                "ACID Transaction initialized (Snapshot Isolation)".into(),
            )),

            Statement::CommitTransaction => Ok(QueryResult::Success(
                "ACID Transaction committed successfully to WAL".into(),
            )),
            Statement::RollbackTransaction => Ok(QueryResult::Success(
                "ACID Transaction rolled back and changes discarded".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_query;

    #[test]
    fn test_executor_graphrag_traverse_exact_nodes() {
        let ctx = DatabaseContext::new();
        let col = ctx.get_or_create_collection("prod");

        // Insert documents p1, p2, p3
        let mut d1 = Document::new();
        d1.set("_id", "p1");
        d1.set("cat", "tech");
        col.insert(d1).unwrap();

        let mut d2 = Document::new();
        d2.set("_id", "p2");
        d2.set("cat", "tech");
        col.insert(d2).unwrap();

        let mut d3 = Document::new();
        d3.set("_id", "p3");
        d3.set("cat", "tech");
        col.insert(d3).unwrap();

        // Connect p1 -> p2 in graph store
        ctx.graph_store
            .write()
            .add_vertex(faizdb_graph::Vertex::new("p1", "Product"));
        ctx.graph_store
            .write()
            .add_vertex(faizdb_graph::Vertex::new("p2", "Product"));
        ctx.graph_store
            .write()
            .add_vertex(faizdb_graph::Vertex::new("p3", "Product"));
        ctx.graph_store
            .write()
            .add_edge(faizdb_graph::Edge::new("p1", "p2", "related"));

        // 1. FIND prod TRAVERSE FROM "p1" DEPTH 1
        // Must return p1 and p2, NOT p3!
        let stmt = parse_query("FIND prod TRAVERSE FROM 'p1' DEPTH 1").unwrap();
        let res = ctx.execute(stmt).unwrap();
        match res {
            QueryResult::Documents(docs) => {
                let ids: Vec<String> = docs.iter().map(|d| d.id.as_str().to_string()).collect();
                assert!(ids.contains(&"p1".to_string()));
                assert!(ids.contains(&"p2".to_string()));
                assert!(
                    !ids.contains(&"p3".to_string()),
                    "p3 has no edge from p1 and must not be returned!"
                );
                assert_eq!(docs.len(), 2);
            }
            _ => panic!("Expected QueryResult::Documents"),
        }

        // 2. FIND prod WHERE cat = 'tech' TRAVERSE FROM "p1" DEPTH 1
        // Both filter and graph traversal must be satisfied!
        let stmt = parse_query("FIND prod WHERE cat = 'tech' TRAVERSE FROM 'p1' DEPTH 1").unwrap();
        let res = ctx.execute(stmt).unwrap();
        match res {
            QueryResult::Documents(docs) => {
                let ids: Vec<String> = docs.iter().map(|d| d.id.as_str().to_string()).collect();
                assert!(ids.contains(&"p1".to_string()));
                assert!(ids.contains(&"p2".to_string()));
                assert!(!ids.contains(&"p3".to_string()));
                assert_eq!(docs.len(), 2);
            }
            _ => panic!("Expected QueryResult::Documents"),
        }
    }

    #[test]
    fn test_executor_vector_using_index_and_error_handling() {
        let ctx = DatabaseContext::new();
        let col = ctx.get_or_create_collection("prod");

        let mut d1 = Document::new();
        d1.set("_id", "p1");
        d1.set("name", "Doc 1");
        col.insert(d1).unwrap();

        // Create vector index named 'custom_emb' (different from collection name 'prod')
        let config = faizdb_vector::HnswConfig {
            dimensions: 2,
            metric: faizdb_vector::DistanceMetric::Cosine,
            ..Default::default()
        };
        let idx = Arc::new(parking_lot::RwLock::new(faizdb_vector::HnswIndex::new(
            config,
        )));
        idx.write().insert("p1", vec![1.0, 0.0]).unwrap();
        ctx.vector_indexes().insert("custom_emb".to_string(), idx);

        // Query with USING INDEX 'custom_emb'
        let stmt =
            parse_query("FIND prod VECTOR NEAR [1.0, 0.0] TOP 2 USING INDEX 'custom_emb'").unwrap();
        let res = ctx.execute(stmt).unwrap();
        match res {
            QueryResult::Documents(docs) => {
                assert_eq!(docs.len(), 1);
                assert_eq!(docs[0].id.as_str(), "p1");
            }
            _ => panic!("Expected QueryResult::Documents"),
        }

        // Query with missing index returns clear error
        let stmt =
            parse_query("FIND prod VECTOR NEAR [1.0, 0.0] TOP 2 USING INDEX 'missing_index'")
                .unwrap();
        let err = ctx.execute(stmt).unwrap_err();
        assert!(err.contains("Vector index 'missing_index' not found"));
    }

    #[test]
    fn test_executor_sql_update_and_order_by() {
        let ctx = DatabaseContext::new();
        let col = ctx.get_or_create_collection("leaderboards");

        let mut d1 = Document::new();
        d1.set("player_id", "p1");
        d1.set("score", 100);
        col.insert(d1).unwrap();

        let mut d2 = Document::new();
        d2.set("player_id", "p2");
        d2.set("score", 300);
        col.insert(d2).unwrap();

        let mut d3 = Document::new();
        d3.set("player_id", "p3");
        d3.set("score", 200);
        col.insert(d3).unwrap();

        // 1. Test UPDATE with arithmetic increment
        let update_stmt =
            parse_query("UPDATE leaderboards SET score = score + 500 WHERE player_id = 'p1'")
                .unwrap();
        let update_res = ctx.execute(update_stmt).unwrap();
        match update_res {
            QueryResult::Updated(count) => assert_eq!(count, 1),
            _ => panic!("Expected QueryResult::Updated"),
        }

        // 2. Test ORDER BY score DESC
        let query_stmt = parse_query("SELECT * FROM leaderboards ORDER BY score DESC").unwrap();
        let query_res = ctx.execute(query_stmt).unwrap();
        match query_res {
            QueryResult::Documents(docs) => {
                assert_eq!(docs.len(), 3);
                // p1 now has 600, p2 has 300, p3 has 200
                assert_eq!(
                    docs[0].get("player_id"),
                    Some(&Value::String("p1".to_string()))
                );
                assert_eq!(docs[0].get("score"), Some(&Value::Integer(600)));
                assert_eq!(
                    docs[1].get("player_id"),
                    Some(&Value::String("p2".to_string()))
                );
                assert_eq!(
                    docs[2].get("player_id"),
                    Some(&Value::String("p3".to_string()))
                );
            }
            _ => panic!("Expected QueryResult::Documents"),
        }
    }

    #[test]
    fn test_executor_sql_inner_and_left_join() {
        let ctx = DatabaseContext::new();
        let orders = ctx.get_or_create_collection("orders");
        let users = ctx.get_or_create_collection("users");

        // Insert users
        let mut u1 = Document::new();
        u1.id = "u1".into();
        u1.set("name", "Faiz");
        users.insert(u1).unwrap();

        let mut u2 = Document::new();
        u2.id = "u2".into();
        u2.set("name", "Sara");
        users.insert(u2).unwrap();

        // Insert orders (order 1 belongs to u1, order 2 belongs to u3 [no user])
        let mut o1 = Document::new();
        o1.id = "o1".into();
        o1.set("user_id", "u1");
        o1.set("amount", 250);
        orders.insert(o1).unwrap();

        let mut o2 = Document::new();
        o2.id = "o2".into();
        o2.set("user_id", "u3");
        o2.set("amount", 100);
        orders.insert(o2).unwrap();

        // 1. INNER JOIN: only o1 has matching user
        let inner_sql =
            parse_query("SELECT * FROM orders JOIN users ON orders.user_id = users.id").unwrap();
        let inner_res = ctx.execute(inner_sql).unwrap();
        match inner_res {
            QueryResult::Documents(docs) => {
                assert_eq!(docs.len(), 1, "INNER JOIN must only return matched records");
                assert_eq!(
                    docs[0].get("user_id"),
                    Some(&Value::String("u1".to_string()))
                );
                assert_eq!(
                    docs[0].get("name"),
                    Some(&Value::String("Faiz".to_string()))
                );
                assert_eq!(
                    docs[0].get("users_name"),
                    Some(&Value::String("Faiz".to_string()))
                );
                assert_eq!(docs[0].get("amount"), Some(&Value::Integer(250)));
            }
            _ => panic!("Expected QueryResult::Documents"),
        }

        // 2. LEFT JOIN: o1 (matched) and o2 (unmatched) should both be returned
        let left_sql =
            parse_query("SELECT * FROM orders LEFT JOIN users ON orders.user_id = users.id")
                .unwrap();
        let left_res = ctx.execute(left_sql).unwrap();
        match left_res {
            QueryResult::Documents(docs) => {
                assert_eq!(docs.len(), 2, "LEFT JOIN must return all left records");
                let ids: Vec<String> = docs.iter().map(|d| d.id.as_str().to_string()).collect();
                assert!(ids.contains(&"o1".to_string()));
                assert!(ids.contains(&"o2".to_string()));
            }
            _ => panic!("Expected QueryResult::Documents"),
        }
    }

    #[test]
    fn test_executor_cypher_graphrag_and_semantic_cache_e2e() {
        let ctx = DatabaseContext::new();

        // 1. Insert documents using openCypher CREATE
        let s1 = parse_query("CREATE (n:prod {id: 'p1', cat: 'ai', title: 'FaizDB Core'})").unwrap();
        let s2 = parse_query("CREATE (n:prod {id: 'p2', cat: 'ai', title: 'Graph Engine'})").unwrap();
        let s3 = parse_query("CREATE (n:prod {id: 'p3', cat: 'ai', title: 'Isolated Component'})").unwrap();
        ctx.execute(s1).unwrap();
        ctx.execute(s2).unwrap();
        ctx.execute(s3).unwrap();

        // 2. Connect p1 -> p2 via openCypher CREATE edge
        let edge_q = parse_query("CREATE (a {id: 'p1'})-[:USES]->(b {id: 'p2'})").unwrap();
        let edge_res = ctx.execute(edge_q).unwrap();
        match edge_res {
            QueryResult::Success(msg) => assert!(msg.contains("created successfully")),
            _ => panic!("Expected QueryResult::Success for edge creation"),
        }

        // 3. Execute openCypher traversal query
        let match_q = parse_query("MATCH (a:prod)-[:USES]->(b:prod) WHERE a.id = 'p1' RETURN b").unwrap();
        let match_res = ctx.execute(match_q).unwrap();
        match match_res {
            QueryResult::Documents(docs) => {
                let ids: Vec<String> = docs.iter().map(|d| d.id.as_str().to_string()).collect();
                assert!(ids.contains(&"p1".to_string()));
                assert!(ids.contains(&"p2".to_string()));
                assert!(!ids.contains(&"p3".to_string()), "Isolated p3 must not be reached");
                assert_eq!(docs.len(), 2);
            }
            _ => panic!("Expected QueryResult::Documents from Cypher MATCH traversal"),
        }


        // 4. Extract LLM GraphRAG context
        let rag = ctx.graph_store.read().extract_rag_context("p1", 2, None);
        assert_eq!(rag.root_id, "p1");
        assert!(rag.formatted_markdown.contains("# Knowledge Graph Context for: `p1`"));
        assert!(rag.formatted_markdown.contains("- (`p1`) -[:USES]-> (`p2`)"));

        // 5. Store in Semantic Cache
        ctx.semantic_cache.put(
            "Which component does FaizDB Core use?",
            vec![1.0, 0.0, 0.0],
            &rag.formatted_markdown,
            vec!["p1".to_string(), "p2".to_string()],
        );

        // Near-identical query embedding (cosine similarity ~ 0.99 > 0.90)
        let cache_hit = ctx.semantic_cache.get(&[0.99, 0.02, 0.0]);
        assert!(cache_hit.is_some());
        let hit = cache_hit.unwrap();
        assert!(hit.similarity >= 0.90);
        assert_eq!(hit.prompt, "Which component does FaizDB Core use?");
        assert!(hit.context.contains("Knowledge Graph Context"));
        assert_eq!(hit.document_ids, vec!["p1", "p2"]);

        // Distinct query embedding (cosine similarity ~ 0.0 < 0.90) -> Cache Miss
        let cache_miss = ctx.semantic_cache.get(&[0.0, 1.0, 0.0]);
        assert!(cache_miss.is_none());
    }
}

