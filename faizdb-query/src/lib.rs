//! # FaizDB Query Engine
//!
//! Provides the AST, multi-dialect parser (SQL, MongoDB JSON, FaizQL),
//! and execution engine for querying FaizDB collections.

pub mod aggregation;
pub mod ast;
pub mod distributed;
pub mod executor;
pub mod optimizer;
pub mod parser;

pub use aggregation::{
    execute_pipeline, execute_pipeline_with_collections, parse_pipeline, Accumulator, PipelineStage,
};
pub use ast::{FilterExpr, Operator, Statement, TraverseClause, VectorSearchClause};
pub use distributed::{
    DistributedQueryCoordinator, DistributedQueryResult, ScatterGatherPlan, ShardTarget,
};
pub use executor::{DatabaseContext, QueryResult};
pub use optimizer::{
    ColumnHistogram, CostModel, OptimizerDecision, QueryOptimizer, TableStatistics,
};
pub use parser::parse_query;

/// Query engine version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
