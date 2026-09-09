//! Graph Pattern Matching & Fluent Query Builder.
//!
//! Provides expressive declarative pattern queries for Knowledge Graphs
//! and GraphRAG pipelines, inspired by openCypher.

use crate::graph::{Direction, Edge, GraphStore, Vertex};
use serde::{Deserialize, Serialize};

/// A matched path element from a graph pattern query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternMatch {
    pub from: Vertex,
    pub edge: Edge,
    pub to: Vertex,
}

/// Filter condition for node properties
#[derive(Debug, Clone)]
pub struct PropertyFilter {
    pub key: String,
    pub value: String,
}

/// Declarative Graph Query Builder
#[derive(Debug, Clone, Default)]
pub struct GraphQuery {
    source_id: Option<String>,
    source_label: Option<String>,
    edge_relation: Option<String>,
    target_label: Option<String>,
    direction: Option<Direction>,
    source_filters: Vec<PropertyFilter>,
    target_filters: Vec<PropertyFilter>,
    limit: Option<usize>,
}

impl GraphQuery {
    pub fn new() -> Self {
        Self::default()
    }

    /// Match source vertex by specific ID
    pub fn match_source_id(mut self, id: impl Into<String>) -> Self {
        self.source_id = Some(id.into());
        self
    }

    /// Match source vertex by category label (e.g. "Person", "Article")
    pub fn match_source_label(mut self, label: impl Into<String>) -> Self {
        self.source_label = Some(label.into());
        self
    }

    /// Match relationship type (e.g. "KNOWS", "AUTHORED", "SIMILAR_TO")
    pub fn via_relation(mut self, relation: impl Into<String>) -> Self {
        self.edge_relation = Some(relation.into());
        self
    }

    /// Traversal direction (defaults to Outgoing)
    pub fn direction(mut self, dir: Direction) -> Self {
        self.direction = Some(dir);
        self
    }

    /// Match target vertex by category label
    pub fn match_target_label(mut self, label: impl Into<String>) -> Self {
        self.target_label = Some(label.into());
        self
    }

    /// Add property filter on source vertex
    pub fn where_source_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.source_filters.push(PropertyFilter {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    /// Add property filter on target vertex
    pub fn where_target_property(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.target_filters.push(PropertyFilter {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    /// Limit number of matched pattern results
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Execute the pattern query against a GraphStore
    pub fn execute(&self, graph: &GraphStore) -> Vec<PatternMatch> {
        let mut results = Vec::new();
        let dir = self.direction.unwrap_or(Direction::Outgoing);

        // 1. Identify candidate source IDs
        let candidate_sources: Vec<String> = if let Some(ref id) = self.source_id {
            if graph.get_vertex(id).is_some() {
                vec![id.clone()]
            } else {
                return Vec::new();
            }
        } else if let Some(ref label) = self.source_label {
            graph.find_vertices_by_label(label)
        } else {
            graph.all_vertex_ids()
        };

        let max_results = self.limit.unwrap_or(usize::MAX);

        for src_id in &candidate_sources {
            if results.len() >= max_results {
                break;
            }

            let Some(source_vertex) = graph.get_vertex(src_id) else {
                continue;
            };

            // Check source property filters
            if !self.matches_filters(source_vertex, &self.source_filters) {
                continue;
            }

            // Fetch edges according to direction and relation
            let edges = graph.edges(src_id, dir, self.edge_relation.as_deref());

            for edge in edges {
                if results.len() >= max_results {
                    break;
                }

                let target_id = if edge.from == *src_id {
                    &edge.to
                } else {
                    &edge.from
                };

                let Some(target_vertex) = graph.get_vertex(target_id) else {
                    continue;
                };

                // Check target label
                if let Some(ref tgt_label) = self.target_label {
                    if target_vertex.label != *tgt_label {
                        continue;
                    }
                }

                // Check target property filters
                if !self.matches_filters(target_vertex, &self.target_filters) {
                    continue;
                }

                results.push(PatternMatch {
                    from: source_vertex.clone(),
                    edge: edge.clone(),
                    to: target_vertex.clone(),
                });
            }
        }

        results
    }

    fn matches_filters(&self, vertex: &Vertex, filters: &[PropertyFilter]) -> bool {
        for f in filters {
            let Some(val) = vertex.properties.get(&f.key) else {
                return false;
            };
            if val.to_string() != f.value {
                return false;
            }
        }
        true
    }
}
