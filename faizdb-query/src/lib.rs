//! # FaizDB Query Engine
//!
//! Provides the AST, multi-dialect parser (SQL, MongoDB JSON, FaizQL),
//! and execution engine for querying FaizDB collections.

pub mod aggregation;
pub mod ast;
pub mod parser;
pub mod executor;
pub mod distributed;
pub mod optimizer;

pub use aggregation::{execute_pipeline, execute_pipeline_with_collections, parse_pipeline, PipelineStage, Accumulator};
pub use ast::{FilterExpr, Operator, Statement, VectorSearchClause, TraverseClause};
pub use parser::parse_query;
pub use executor::{DatabaseContext, QueryResult};
pub use distributed::{DistributedQueryCoordinator, ScatterGatherPlan, ShardTarget, DistributedQueryResult};
pub use optimizer::{ColumnHistogram, CostModel, QueryOptimizer, TableStatistics, OptimizerDecision};

/// Query engine version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
