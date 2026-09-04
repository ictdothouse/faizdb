//! # FaizDB Vector Engine — AI-Native High-Dimensional Vector Search
//!
//! Provides ultra-fast vector search directly embedded in FaizDB without needing
//! external plugins (unlike pgvector or MongoDB Atlas Search).
//!
//! ## Key Capabilities
//! - **HNSW (Hierarchical Navigable Small World)** indexing for sub-millisecond similarity search
//! - Multiple metrics: **Cosine Distance**, **Euclidean (L2)**, **Dot Product**, **Manhattan**
//! - High dimensional support (up to 4096 dimensions for standard text & multimodal embeddings)
//! - Thread-safe vector index management

pub mod distance;
pub mod hnsw;
pub mod quantization;

pub use distance::{
    cosine_distance, dot_product_distance, euclidean_distance, manhattan_distance, normalize,
    normalize_in_place, squared_euclidean_distance, DistanceMetric,
};
pub use hnsw::{HnswConfig, HnswIndex, VectorSearchResult};
pub use quantization::{
    BinaryQuantizedVector, BinaryQuantizer, QuantizationType, QuantizedVector, ScalarQuantizer,
};

/// Database version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
