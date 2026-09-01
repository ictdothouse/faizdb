//! # FaizDB Query Engine
//!
//! Provides the AST, multi-dialect parser (SQL, MongoDB JSON, FaizQL),
//! and execution engine for querying FaizDB collections.

pub mod aggregation;
pub mod ast;
pub mod parser;
pub mod executor;

pub use aggregation::{execute_pipeline, parse_pipeline, PipelineStage, Accumulator};
pub use ast::{FilterExpr, Operator, Statement, VectorSearchClause, TraverseClause};
pub use parser::parse_query;
pub use executor::{DatabaseContext, QueryResult};

/// Query engine version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
