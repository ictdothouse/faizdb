//! Graph Inverted Indexing — Label, Relation, and Property lookups.
//!
//! Provides sub-microsecond O(1) index scans for vertices by category label,
//! relationship types, and specific document properties.

use crate::graph::{Edge, Vertex};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Fast Inverted Graph Index for O(1) Vertex & Edge Lookups
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphIndex {
    /// Label -> Set of Vertex IDs (e.g. "Person" -> {"u1", "u2"})
    label_index: HashMap<String, HashSet<String>>,
    /// Relation type -> Set of (from_id, to_id)
    relation_index: HashMap<String, HashSet<(String, String)>>,
    /// (property_key, property_val) -> Set of Vertex IDs
    property_index: HashMap<(String, String), HashSet<String>>,
}

impl GraphIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Index a vertex by label and stringifiable properties
    pub fn index_vertex(&mut self, vertex: &Vertex) {
        let vid = vertex.id.clone();
        self.label_index
            .entry(vertex.label.clone())
            .or_default()
            .insert(vid.clone());

        for (k, v) in &vertex.properties.fields {
            let val_str = v.to_string();
            self.property_index
                .entry((k.clone(), val_str))
                .or_default()
                .insert(vid.clone());
        }
    }

    /// Remove a vertex from all inverted indices
    pub fn unindex_vertex(&mut self, vertex: &Vertex) {
        let vid = &vertex.id;
        if let Some(set) = self.label_index.get_mut(&vertex.label) {
            set.remove(vid);
        }

        for (k, v) in &vertex.properties.fields {
            let val_str = v.to_string();
            if let Some(set) = self.property_index.get_mut(&(k.clone(), val_str)) {
                set.remove(vid);
            }
        }
    }

    /// Index an edge by relation type
    pub fn index_edge(&mut self, edge: &Edge) {
        self.relation_index
            .entry(edge.relation.clone())
            .or_default()
            .insert((edge.from.clone(), edge.to.clone()));
    }

    /// Remove an edge from relation index
    pub fn unindex_edge(&mut self, edge: &Edge) {
        if let Some(set) = self.relation_index.get_mut(&edge.relation) {
            set.remove(&(edge.from.clone(), edge.to.clone()));
        }
    }

    /// Lookup all vertex IDs with a specific label
    pub fn find_by_label(&self, label: &str) -> Vec<String> {
        self.label_index
            .get(label)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Lookup all vertex IDs matching a specific property key & string value
    pub fn find_by_property(&self, key: &str, value: &str) -> Vec<String> {
        self.property_index
            .get(&(key.to_string(), value.to_string()))
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Lookup all edges (from, to) with a given relationship type
    pub fn find_edges_by_relation(&self, relation: &str) -> Vec<(String, String)> {
        self.relation_index
            .get(relation)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Count indexed labels
    pub fn label_count(&self) -> usize {
        self.label_index.len()
    }

    /// Clear all indices
    pub fn clear(&mut self) {
        self.label_index.clear();
        self.relation_index.clear();
        self.property_index.clear();
    }
}
