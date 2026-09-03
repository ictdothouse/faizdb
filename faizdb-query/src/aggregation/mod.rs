//! Aggregation and Analytics Pipeline Module.

pub mod pipeline;
pub mod parser;

pub use pipeline::{execute_pipeline, execute_pipeline_with_collections, PipelineStage, Accumulator};
pub use parser::parse_pipeline;
