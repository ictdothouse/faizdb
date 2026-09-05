//! # FaizDB Graph Engine — AI-Ready Knowledge Graph & GraphRAG
//!
//! Provides native relationship graph storage, traversal, and GraphRAG context
//! extraction integrated directly with FaizDB documents.

pub mod cache;
pub mod graph;

pub use cache::{cosine_similarity, SemanticCache, SemanticCacheEntry, SemanticCacheHit};
pub use graph::{Direction, Edge, GraphRagContext, GraphStore, PathStep, Vertex};

/// Crate version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

