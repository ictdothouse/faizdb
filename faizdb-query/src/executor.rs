//! Query Executor for evaluating parsed statements against collections and indexes.

use std::sync::Arc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use faizdb_core::document::collection::Collection;
use faizdb_core::document::model::Document;
use crate::ast::Statement;

/// Query execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QueryResult {
    Documents(Vec<Document>),
    Count(u64),
    Inserted(Vec<String>),
    Updated(u64),
    Deleted(u64),
    Success(String),
}

/// Execution environment holding database collections
pub struct DatabaseContext {
    collections: DashMap<String, Arc<Collection>>,
}

impl Default for DatabaseContext {
    fn default() -> Self {
        Self {
            collections: DashMap::new(),
        }
    }
}

impl DatabaseContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create a collection
    pub fn get_or_create_collection(&self, name: &str) -> Arc<Collection> {
        self.collections
            .entry(name.to_string())
            .or_insert_with(|| Arc::new(Collection::new(name)))
            .clone()
    }

    /// Execute an AST statement
    pub fn execute(&self, stmt: Statement) -> Result<QueryResult, String> {
        match stmt {
            Statement::Find {
                collection,
                filter,
                limit,
                skip,
                ..
            } => {
                let col = self.get_or_create_collection(&collection);
                let all_docs = col.find_all(None);

                let filtered: Vec<Document> = all_docs
                    .into_iter()
                    .filter(|doc| filter.as_ref().map_or(true, |f| f.matches(doc)))
                    .skip(skip.unwrap_or(0))
                    .take(limit.unwrap_or(usize::MAX))
                    .collect();

                Ok(QueryResult::Documents(filtered))
            }
            Statement::Insert { collection, documents } => {
                let col = self.get_or_create_collection(&collection);
                let mut ids = Vec::with_capacity(documents.len());
                for doc in documents {
                    let id = col.insert(doc).map_err(|e| e.to_string())?;
                    ids.push(id.as_str().to_string());
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
                    let _ = col.delete_by_id(&id);
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
                    let res = col.update_by_id(&id, |doc| {
                        for (k, v) in &updates {
                            doc.set(k.clone(), v.clone());
                        }
                    });
                    if res.is_ok() {
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
                Ok(QueryResult::Success(format!("Collection '{name}' dropped")))
            }
            Statement::CreateIndex { collection, field, unique } => {
                let col = self.get_or_create_collection(&collection);
                col.create_index(faizdb_core::document::collection::IndexDef {
                    name: format!("idx_{field}"),
                    fields: vec![(field, 1)],
                    index_type: faizdb_core::document::collection::IndexType::BTree,
                    unique,
                    sparse: false,
                })
                .map_err(|e| e.to_string())?;
                Ok(QueryResult::Success("Index created".into()))
            }
        }
    }
}
