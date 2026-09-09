//! # FaizDB Graph Engine — AI-Ready Knowledge Graph & GraphRAG
//!
//! Provides native relationship graph storage, traversal, and GraphRAG context
//! extraction integrated directly with FaizDB documents.

pub mod algorithms;
pub mod cache;
pub mod graph;
pub mod index;
pub mod persistence;
pub mod query;
pub mod vector_graph;

pub use algorithms::{
    degree_centrality, dijkstra_shortest_path, k_hop_neighbors, pagerank,
    weakly_connected_components, DegreeCentrality, ShortestPathResult,
};
pub use cache::{cosine_similarity, SemanticCache, SemanticCacheEntry, SemanticCacheHit};
pub use graph::{Direction, Edge, GraphRagContext, GraphStore, PathStep, Vertex};
pub use index::GraphIndex;
pub use persistence::{load_snapshot, save_snapshot, GRAPH_SNAPSHOT_MAGIC};
pub use query::{GraphQuery, PatternMatch, PropertyFilter};
pub use vector_graph::{FusedRagContext, FusedRagNode, VectorGraph};

/// Crate version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

