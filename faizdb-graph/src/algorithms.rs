//! High-Performance Graph Algorithms — Dijkstra, PageRank, WCC, and Centrality.
//!
//! Provides industrial-grade graph intelligence comparable to Neo4j GDS & Memgraph MAGE.

use crate::graph::{Direction, GraphStore};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet, VecDeque};

/// Min-heap element for Dijkstra shortest path search
#[derive(Debug, Clone, PartialEq)]
struct DijkstraState {
    node_id: String,
    cost: f32,
}

impl Eq for DijkstraState {}

impl Ord for DijkstraState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse order for min-heap
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for DijkstraState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Dijkstra weighted shortest path result
#[derive(Debug, Clone, PartialEq)]
pub struct ShortestPathResult {
    /// Ordered list of vertex IDs from source to destination
    pub path: Vec<String>,
    /// Total path weight
    pub total_cost: f32,
}

/// Compute weighted shortest path between two vertices using Dijkstra's algorithm
pub fn dijkstra_shortest_path(
    graph: &GraphStore,
    from: &str,
    to: &str,
) -> Option<ShortestPathResult> {
    if from == to {
        return Some(ShortestPathResult {
            path: vec![from.to_string()],
            total_cost: 0.0,
        });
    }

    if graph.get_vertex(from).is_none() || graph.get_vertex(to).is_none() {
        return None;
    }

    let mut distances: HashMap<String, f32> = HashMap::new();
    let mut predecessors: HashMap<String, String> = HashMap::new();
    let mut heap = BinaryHeap::new();

    distances.insert(from.to_string(), 0.0);
    heap.push(DijkstraState {
        node_id: from.to_string(),
        cost: 0.0,
    });

    while let Some(DijkstraState { node_id, cost }) = heap.pop() {
        if node_id == to {
            // Reconstruct path
            let mut path = vec![to.to_string()];
            let mut curr = to;
            while let Some(prev) = predecessors.get(curr) {
                path.push(prev.clone());
                curr = prev;
            }
            path.reverse();
            return Some(ShortestPathResult {
                path,
                total_cost: cost,
            });
        }

        if let Some(&best) = distances.get(&node_id) {
            if cost > best {
                continue;
            }
        }

        for edge in graph.edges(&node_id, Direction::Outgoing, None) {
            let next_cost = cost + edge.weight.max(0.0);
            let is_shorter = distances
                .get(&edge.to)
                .map(|&d| next_cost < d)
                .unwrap_or(true);

            if is_shorter {
                distances.insert(edge.to.clone(), next_cost);
                predecessors.insert(edge.to.clone(), node_id.clone());
                heap.push(DijkstraState {
                    node_id: edge.to.clone(),
                    cost: next_cost,
                });
            }
        }
    }

    None
}

/// Compute PageRank centrality scores for all vertices in the graph
///
/// Implements power iteration with damping factor (standard: 0.85) and handles dangling nodes.
pub fn pagerank(
    graph: &GraphStore,
    damping_factor: f32,
    max_iterations: usize,
    tolerance: f32,
) -> HashMap<String, f32> {
    let n = graph.vertex_count();
    if n == 0 {
        return HashMap::new();
    }

    let initial_score = 1.0 / n as f32;
    let mut scores: HashMap<String, f32> = HashMap::with_capacity(n);
    let mut out_degree: HashMap<String, usize> = HashMap::with_capacity(n);

    // Initialize scores
    for v_id in graph.all_vertex_ids() {
        scores.insert(v_id.clone(), initial_score);
        let out = graph.edges(&v_id, Direction::Outgoing, None).len();
        out_degree.insert(v_id, out);
    }

    let vertices = graph.all_vertex_ids();

    for _ in 0..max_iterations {
        let mut new_scores: HashMap<String, f32> = HashMap::with_capacity(n);
        let mut dangling_sum = 0.0f32;

        // Sum contributions from dangling nodes (no outgoing edges)
        for v_id in &vertices {
            if out_degree.get(v_id).copied().unwrap_or(0) == 0 {
                dangling_sum += scores.get(v_id).copied().unwrap_or(0.0);
            }
        }

        let base_score = ((1.0 - damping_factor) + damping_factor * dangling_sum) / n as f32;

        let mut max_diff = 0.0f32;

        for v_id in &vertices {
            let mut inbound_sum = 0.0f32;
            for edge in graph.edges(v_id, Direction::Incoming, None) {
                let sender_out = out_degree.get(&edge.from).copied().unwrap_or(1);
                if sender_out > 0 {
                    let sender_score = scores.get(&edge.from).copied().unwrap_or(0.0);
                    inbound_sum += sender_score / sender_out as f32;
                }
            }

            let new_score = base_score + damping_factor * inbound_sum;
            let old_score = scores.get(v_id).copied().unwrap_or(0.0);
            max_diff = max_diff.max((new_score - old_score).abs());
            new_scores.insert(v_id.clone(), new_score);
        }

        scores = new_scores;
        if max_diff < tolerance {
            break;
        }
    }

    scores
}

/// Compute Weakly Connected Components (WCC) for community & cluster detection
///
/// Treats directed edges as undirected links to identify disconnected partitions.
pub fn weakly_connected_components(graph: &GraphStore) -> Vec<Vec<String>> {
    let mut visited = HashSet::new();
    let mut components = Vec::new();

    for start_id in graph.all_vertex_ids() {
        if visited.contains(&start_id) {
            continue;
        }

        let mut component = Vec::new();
        let mut queue = VecDeque::new();

        visited.insert(start_id.clone());
        queue.push_back(start_id);

        while let Some(curr) = queue.pop_front() {
            component.push(curr.clone());

            for edge in graph.edges(&curr, Direction::Both, None) {
                let neighbor = if edge.from == curr {
                    &edge.to
                } else {
                    &edge.from
                };

                if visited.insert(neighbor.clone()) {
                    queue.push_back(neighbor.clone());
                }
            }
        }

        component.sort();
        components.push(component);
    }

    components.sort_by(|a, b| b.len().cmp(&a.len()));
    components
}

/// Degree Centrality metrics for a node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DegreeCentrality {
    pub in_degree: usize,
    pub out_degree: usize,
    pub total_degree: usize,
}

/// Compute Degree Centrality for a vertex
pub fn degree_centrality(graph: &GraphStore, vertex_id: &str) -> Option<DegreeCentrality> {
    if graph.get_vertex(vertex_id).is_none() {
        return None;
    }

    let in_degree = graph.edges(vertex_id, Direction::Incoming, None).len();
    let out_degree = graph.edges(vertex_id, Direction::Outgoing, None).len();
    Some(DegreeCentrality {
        in_degree,
        out_degree,
        total_degree: in_degree + out_degree,
    })
}

/// Extract all nodes within K-hops from a start vertex
pub fn k_hop_neighbors(graph: &GraphStore, start_id: &str, k: usize) -> HashSet<String> {
    let mut visited = HashSet::new();
    let mut current_level = HashSet::new();

    if graph.get_vertex(start_id).is_none() {
        return visited;
    }

    visited.insert(start_id.to_string());
    current_level.insert(start_id.to_string());

    for _ in 0..k {
        let mut next_level = HashSet::new();
        for node in &current_level {
            for edge in graph.edges(node, Direction::Outgoing, None) {
                if visited.insert(edge.to.clone()) {
                    next_level.insert(edge.to.clone());
                }
            }
        }
        if next_level.is_empty() {
            break;
        }
        current_level = next_level;
    }

    visited
}
