//! Graph Engine — Vertices, Edges, Traversal, and GraphRAG for FaizDB.
//!
//! Enables rich relationship traversal, knowledge graphs, and hybrid AI GraphRAG
//! without needing a separate graph database like Neo4j.

use faizdb_core::document::model::Document;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

/// Direction for traversing relationships
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    Outgoing,
    Incoming,
    Both,
}

/// A graph vertex (node)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vertex {
    /// Unique vertex ID
    pub id: String,
    /// Vertex label / category (e.g. "Person", "Article", "Topic")
    pub label: String,
    /// Properties stored as a standard FaizDB Document
    pub properties: Document,
}

impl Vertex {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            properties: Document::new(),
        }
    }

    pub fn with_properties(id: impl Into<String>, label: impl Into<String>, doc: Document) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            properties: doc,
        }
    }
}

/// A directed edge between two vertices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    /// Source vertex ID
    pub from: String,
    /// Target vertex ID
    pub to: String,
    /// Relationship type (e.g. "KNOWS", "AUTHORED", "REFERENCES")
    pub relation: String,
    /// Optional edge weight (for pathfinding / importance)
    pub weight: f32,
    /// Extra properties
    pub properties: Document,
}

impl Edge {
    pub fn new(
        from: impl Into<String>,
        to: impl Into<String>,
        relation: impl Into<String>,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            relation: relation.into(),
            weight: 1.0,
            properties: Document::new(),
        }
    }

    pub fn with_weight(
        from: impl Into<String>,
        to: impl Into<String>,
        relation: impl Into<String>,
        weight: f32,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            relation: relation.into(),
            weight,
            properties: Document::new(),
        }
    }
}

/// Traversal step in a path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PathStep {
    pub vertex_id: String,
    pub relation: Option<String>,
    pub depth: usize,
}

/// Graph Store with adjacency lists
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GraphStore {
    vertices: HashMap<String, Vertex>,
    /// outgoing: from -> Vec<Edge>
    outgoing: HashMap<String, Vec<Edge>>,
    /// incoming: to -> Vec<Edge>
    incoming: HashMap<String, Vec<Edge>>,
}

impl GraphStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add or update a vertex
    pub fn add_vertex(&mut self, vertex: Vertex) {
        let id = vertex.id.clone();
        self.vertices.insert(id.clone(), vertex);
        self.outgoing.entry(id.clone()).or_default();
        self.incoming.entry(id).or_default();
    }

    /// Add an edge (deduplicating existing edges with identical from, to, and relation)
    pub fn add_edge(&mut self, edge: Edge) {
        let from = edge.from.clone();
        let to = edge.to.clone();

        // Ensure vertices exist
        self.vertices
            .entry(from.clone())
            .or_insert_with(|| Vertex::new(from.clone(), "Generic"));
        self.vertices
            .entry(to.clone())
            .or_insert_with(|| Vertex::new(to.clone(), "Generic"));

        let out_list = self.outgoing.entry(from).or_default();
        if let Some(existing) = out_list
            .iter_mut()
            .find(|e| e.to == edge.to && e.relation == edge.relation)
        {
            existing.weight = edge.weight;
            existing.properties = edge.properties.clone();
        } else {
            out_list.push(edge.clone());
        }

        let in_list = self.incoming.entry(to).or_default();
        if let Some(existing) = in_list
            .iter_mut()
            .find(|e| e.from == edge.from && e.relation == edge.relation)
        {
            existing.weight = edge.weight;
            existing.properties = edge.properties;
        } else {
            in_list.push(edge);
        }
    }

    /// Remove an edge between vertices, optionally matching a relation type
    pub fn remove_edge(&mut self, from: &str, to: &str, relation: Option<&str>) -> bool {
        let mut removed = false;
        if let Some(out_list) = self.outgoing.get_mut(from) {
            let initial_len = out_list.len();
            out_list.retain(|e| !(e.to == to && relation.is_none_or(|r| e.relation == r)));
            if out_list.len() < initial_len {
                removed = true;
            }
        }
        if let Some(in_list) = self.incoming.get_mut(to) {
            in_list.retain(|e| !(e.from == from && relation.is_none_or(|r| e.relation == r)));
        }
        removed
    }

    /// Remove a vertex and all incident edges (preventing dangling references)
    pub fn remove_vertex(&mut self, id: &str) -> bool {
        if self.vertices.remove(id).is_none() {
            return false;
        }

        // 1. Remove outgoing edges and clean corresponding incoming in peers
        if let Some(out_edges) = self.outgoing.remove(id) {
            for edge in out_edges {
                if let Some(in_list) = self.incoming.get_mut(&edge.to) {
                    in_list.retain(|e| e.from != id);
                }
            }
        }

        // 2. Remove incoming edges and clean corresponding outgoing in peers
        if let Some(in_edges) = self.incoming.remove(id) {
            for edge in in_edges {
                if let Some(out_list) = self.outgoing.get_mut(&edge.from) {
                    out_list.retain(|e| e.to != id);
                }
            }
        }

        true
    }

    /// Get vertex by ID
    pub fn get_vertex(&self, id: &str) -> Option<&Vertex> {
        self.vertices.get(id)
    }

    /// Get outgoing or incoming edges for a vertex
    pub fn edges(&self, vertex_id: &str, dir: Direction, relation: Option<&str>) -> Vec<&Edge> {
        let mut res = Vec::new();

        if dir == Direction::Outgoing || dir == Direction::Both {
            if let Some(list) = self.outgoing.get(vertex_id) {
                for e in list {
                    if relation.is_none_or(|r| e.relation == r) {
                        res.push(e);
                    }
                }
            }
        }

        if dir == Direction::Incoming || dir == Direction::Both {
            if let Some(list) = self.incoming.get(vertex_id) {
                for e in list {
                    if relation.is_none_or(|r| e.relation == r) {
                        res.push(e);
                    }
                }
            }
        }

        res
    }

    /// Traverse graph with Breadth-First Search (BFS) up to max_depth
    /// Ideal for GraphRAG context gathering.
    pub fn traverse_bfs(
        &self,
        start_id: &str,
        max_depth: usize,
        relation_filter: Option<&str>,
    ) -> Vec<PathStep> {
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        let mut results = Vec::new();

        if !self.vertices.contains_key(start_id) {
            return results;
        }

        visited.insert(start_id.to_string());
        queue.push_back((start_id.to_string(), None, 0));

        while let Some((curr_id, rel, depth)) = queue.pop_front() {
            results.push(PathStep {
                vertex_id: curr_id.clone(),
                relation: rel,
                depth,
            });

            if depth < max_depth {
                if let Some(edges) = self.outgoing.get(&curr_id) {
                    for edge in edges {
                        if relation_filter.is_none_or(|r| edge.relation == r)
                            && visited.insert(edge.to.clone())
                        {
                            queue.push_back((
                                edge.to.clone(),
                                Some(edge.relation.clone()),
                                depth + 1,
                            ));
                        }
                    }
                }
            }
        }

        results
    }

    /// Shortest path between two vertices (BFS unweighted)
    pub fn shortest_path(&self, from_id: &str, to_id: &str) -> Option<Vec<String>> {
        if from_id == to_id {
            return Some(vec![from_id.to_string()]);
        }

        let mut visited = HashSet::new();
        let mut parent: HashMap<String, String> = HashMap::new();
        let mut queue = VecDeque::new();

        visited.insert(from_id.to_string());
        queue.push_back(from_id.to_string());

        let mut found = false;

        while let Some(curr) = queue.pop_front() {
            if curr == to_id {
                found = true;
                break;
            }

            if let Some(edges) = self.outgoing.get(&curr) {
                for edge in edges {
                    if visited.insert(edge.to.clone()) {
                        parent.insert(edge.to.clone(), curr.clone());
                        queue.push_back(edge.to.clone());
                    }
                }
            }
        }

        if !found {
            return None;
        }

        // Reconstruct path
        let mut path = vec![to_id.to_string()];
        let mut curr = to_id;
        while let Some(p) = parent.get(curr) {
            path.push(p.clone());
            curr = p;
        }
        path.reverse();
        Some(path)
    }

    /// Total vertices count
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Total edges count
    pub fn edge_count(&self) -> usize {
        self.outgoing.values().map(|v| v.len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_construction_and_bfs() {
        let mut graph = GraphStore::new();

        graph.add_vertex(Vertex::new("faiz", "Person"));
        graph.add_vertex(Vertex::new("ai_db", "Project"));
        graph.add_vertex(Vertex::new("rust", "Technology"));

        graph.add_edge(Edge::new("faiz", "ai_db", "CREATED"));
        graph.add_edge(Edge::new("ai_db", "rust", "USES"));

        assert_eq!(graph.vertex_count(), 3);
        assert_eq!(graph.edge_count(), 2);

        // BFS from "faiz" with depth 2
        let path = graph.traverse_bfs("faiz", 2, None);
        assert_eq!(path.len(), 3);
        assert_eq!(path[0].vertex_id, "faiz");
        assert_eq!(path[1].vertex_id, "ai_db");
        assert_eq!(path[2].vertex_id, "rust");
    }

    #[test]
    fn test_shortest_path() {
        let mut graph = GraphStore::new();

        graph.add_edge(Edge::new("A", "B", "CONNECTS"));
        graph.add_edge(Edge::new("B", "C", "CONNECTS"));
        graph.add_edge(Edge::new("A", "C", "SHORTCUT"));

        let path = graph.shortest_path("A", "C").unwrap();
        // Should find direct shortcut A -> C
        assert_eq!(path, vec!["A", "C"]);
    }

    #[test]
    fn test_graph_edge_deduplication_and_vertex_deletion() {
        let mut graph = GraphStore::new();

        // 1. Edge deduplication: adding identical edge 3 times should only result in 1 edge
        graph.add_edge(Edge::new("User1", "User2", "FOLLOWS"));
        graph.add_edge(Edge::new("User1", "User2", "FOLLOWS"));
        graph.add_edge(Edge::new("User1", "User2", "FOLLOWS"));

        assert_eq!(graph.edge_count(), 1, "Duplicate edges must be merged");
        assert_eq!(graph.vertex_count(), 2);

        // 2. Remove edge
        assert!(graph.remove_edge("User1", "User2", Some("FOLLOWS")));
        assert_eq!(graph.edge_count(), 0);

        // 3. Multi-node dangling reference prevention
        let mut graph2 = GraphStore::new();
        graph2.add_edge(Edge::new("A", "B", "CONNECTS"));
        graph2.add_edge(Edge::new("B", "C", "CONNECTS"));
        assert_eq!(graph2.vertex_count(), 3);
        assert_eq!(graph2.edge_count(), 2);

        // Remove node B: all edges (A->B and B->C) must be cleanly pruned
        assert!(graph2.remove_vertex("B"));
        assert_eq!(graph2.vertex_count(), 2);
        assert_eq!(
            graph2.edge_count(),
            0,
            "All edges incident to B must be removed"
        );
        assert!(graph2.edges("A", Direction::Outgoing, None).is_empty());
        assert!(graph2.edges("C", Direction::Incoming, None).is_empty());
    }
}
