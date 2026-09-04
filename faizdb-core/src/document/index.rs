//! High-Performance B-Tree Secondary Index Engine with Unique Constraint Enforcement.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

use super::model::{Document, Value};
use crate::error::{FaizError, FaizResult};

/// Secondary index definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecondaryIndexDef {
    pub name: String,
    pub collection: String,
    pub field: String,
    pub unique: bool,
}

/// An active secondary B-Tree index instance
pub struct SecondaryIndex {
    pub def: SecondaryIndexDef,
    /// B-Tree mapping string representation of Value -> Set of Document IDs
    tree: RwLock<BTreeMap<String, HashSet<String>>>,
}

impl SecondaryIndex {
    pub fn new(def: SecondaryIndexDef) -> Self {
        Self {
            def,
            tree: RwLock::new(BTreeMap::new()),
        }
    }

    /// Check if inserting this document would violate unique constraint
    pub fn check_unique(&self, doc: &Document) -> FaizResult<()> {
        if !self.def.unique {
            return Ok(());
        }

        if let Some(val) = doc.get_nested(&self.def.field) {
            let key = format!("{val}");
            let tree = self.tree.read();
            if let Some(existing) = tree.get(&key) {
                let doc_id = doc.id.as_str();
                if existing.iter().any(|id| id != doc_id) {
                    return Err(FaizError::DuplicateKey {
                        collection: self.def.collection.clone(),
                        field: self.def.field.clone(),
                        value: key,
                    });
                }
            }
        }
        Ok(())
    }

    /// Insert document into index
    pub fn insert(&self, doc: &Document) {
        if let Some(val) = doc.get_nested(&self.def.field) {
            let key = format!("{val}");
            let mut tree = self.tree.write();
            tree.entry(key)
                .or_default()
                .insert(doc.id.as_str().to_string());
        }
    }

    /// Remove document from index
    pub fn remove(&self, doc: &Document) {
        if let Some(val) = doc.get_nested(&self.def.field) {
            let key = format!("{val}");
            let mut tree = self.tree.write();
            if let Some(set) = tree.get_mut(&key) {
                set.remove(doc.id.as_str());
                if set.is_empty() {
                    tree.remove(&key);
                }
            }
        }
    }

    /// Lookup document IDs matching an exact value: O(log N)
    pub fn lookup(&self, value: &Value) -> Vec<String> {
        let key = format!("{value}");
        let tree = self.tree.read();
        tree.get(&key)
            .cloned()
            .map(|s| s.into_iter().collect())
            .unwrap_or_default()
    }

    /// Range lookup between min and max values: O(log N + K)
    pub fn range_lookup(&self, min: &Value, max: &Value) -> Vec<String> {
        let min_key = format!("{min}");
        let max_key = format!("{max}");
        let tree = self.tree.read();
        let mut results = Vec::new();
        for (_, ids) in tree.range(min_key..=max_key) {
            results.extend(ids.iter().cloned());
        }
        results
    }

    /// Total distinct keys in this index
    pub fn distinct_keys_count(&self) -> usize {
        self.tree.read().len()
    }

    /// Total indexed entries
    pub fn count_entries(&self) -> usize {
        let tree = self.tree.read();
        tree.values().map(|s| s.len()).sum()
    }
}
