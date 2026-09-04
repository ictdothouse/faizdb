//! Aggregation and Analytics Pipeline Module.

pub mod parser;
pub mod pipeline;

pub use parser::parse_pipeline;
pub use pipeline::{
    execute_pipeline, execute_pipeline_with_collections, Accumulator, PipelineStage,
};
