//! # FaizDB Graph Engine — AI-Ready Knowledge Graph & GraphRAG
//!
//! Provides native relationship graph storage, traversal, and GraphRAG context
//! extraction integrated directly with FaizDB documents.

pub mod graph;

pub use graph::{Direction, Edge, GraphStore, PathStep, Vertex};

/// Crate version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
