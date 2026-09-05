//! Abstract Syntax Tree (AST) for FaizQL query language.

use faizdb_core::document::model::{Document, Value};
use serde::{Deserialize, Serialize};

/// Comparison operator
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Operator {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    In,
    Contains,
    StartsWith,
    EndsWith,
}

/// Filter Expression (supports nesting with AND/OR/NOT)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterExpr {
    Field {
        field: String,
        op: Operator,
        value: Value,
    },
    And(Vec<FilterExpr>),
    Or(Vec<FilterExpr>),
    Not(Box<FilterExpr>),
    AlwaysTrue,
}

impl FilterExpr {
    /// Evaluate filter against a document
    pub fn matches(&self, doc: &Document) -> bool {
        match self {
            FilterExpr::AlwaysTrue => true,
            FilterExpr::Field { field, op, value } => {
                let id_val;
                let actual = match doc.get_nested(field) {
                    Some(v) => v,
                    None => {
                        if field == "id" || field == "_id" {
                            id_val = Value::String(doc.id.to_string());
                            &id_val
                        } else {
                            return false;
                        }
                    }
                };
                Self::eval_op(actual, op, value)
            }
            FilterExpr::And(exprs) => exprs.iter().all(|e| e.matches(doc)),
            FilterExpr::Or(exprs) => exprs.iter().any(|e| e.matches(doc)),
            FilterExpr::Not(expr) => !expr.matches(doc),
        }
    }

    fn eval_op(actual: &Value, op: &Operator, target: &Value) -> bool {
        match op {
            Operator::Eq => actual == target,
            Operator::Neq => actual != target,
            Operator::Gt => match (actual, target) {
                (Value::Integer(a), Value::Integer(b)) => a > b,
                (Value::Float(a), Value::Float(b)) => a > b,
                (Value::Integer(a), Value::Float(b)) => (*a as f64) > *b,
                (Value::Float(a), Value::Integer(b)) => *a > (*b as f64),
                (Value::String(a), Value::String(b)) => a > b,
                _ => false,
            },
            Operator::Gte => match (actual, target) {
                (Value::Integer(a), Value::Integer(b)) => a >= b,
                (Value::Float(a), Value::Float(b)) => a >= b,
                (Value::Integer(a), Value::Float(b)) => (*a as f64) >= *b,
                (Value::Float(a), Value::Integer(b)) => *a >= (*b as f64),
                (Value::String(a), Value::String(b)) => a >= b,
                _ => false,
            },
            Operator::Lt => match (actual, target) {
                (Value::Integer(a), Value::Integer(b)) => a < b,
                (Value::Float(a), Value::Float(b)) => a < b,
                (Value::Integer(a), Value::Float(b)) => (*a as f64) < *b,
                (Value::Float(a), Value::Integer(b)) => *a < (*b as f64),
                (Value::String(a), Value::String(b)) => a < b,
                _ => false,
            },
            Operator::Lte => match (actual, target) {
                (Value::Integer(a), Value::Integer(b)) => a <= b,
                (Value::Float(a), Value::Float(b)) => a <= b,
                (Value::Integer(a), Value::Float(b)) => (*a as f64) <= *b,
                (Value::Float(a), Value::Integer(b)) => *a <= (*b as f64),
                (Value::String(a), Value::String(b)) => a <= b,
                _ => false,
            },
            Operator::Contains => match (actual, target) {
                (Value::String(a), Value::String(b)) => a.contains(b.as_str()),
                (Value::Array(arr), target) => arr.contains(target),
                _ => false,
            },
            Operator::StartsWith => match (actual, target) {
                (Value::String(a), Value::String(b)) => a.starts_with(b.as_str()),
                _ => false,
            },
            Operator::EndsWith => match (actual, target) {
                (Value::String(a), Value::String(b)) => a.ends_with(b.as_str()),
                _ => false,
            },
            Operator::In => match (actual, target) {
                (val, Value::Array(arr)) => arr.contains(val),
                _ => false,
            },
        }
    }
}

/// Vector Search Clause within a Query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchClause {
    pub vector: Vec<f32>,
    pub top_k: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_name: Option<String>,
}

/// Graph Traversal Clause within a Query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraverseClause {
    pub start_id: String,
    pub max_depth: usize,
    pub relation: Option<String>,
}

/// Type of relational JOIN
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinType {
    Inner,
    Left,
}

/// A relational JOIN clause between collections/tables
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JoinClause {
    pub join_type: JoinType,
    pub collection: String,
    pub on_left: String,  // e.g. "orders.customer_id" or "customer_id"
    pub on_right: String, // e.g. "customers.id" or "id"
}

/// Detailed metric for a single shard participating in distributed execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardExecutionMetric {
    pub shard_id: u16,
    pub partition_name: String,
    pub execution_time_us: u64,
    pub rows_scanned: usize,
    pub rows_emitted: usize,
    pub cache_hit_pct: f64,
    pub network_transfer_bytes: u64,
    pub status: String,
}

/// Hierarchical plan node for interactive tree visualization and PostgreSQL wire output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanNode {
    pub node_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub estimated_cost_start: f64,
    pub estimated_cost_total: f64,
    pub estimated_rows: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_time_start_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_time_total_us: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_rows: Option<usize>,
    pub loops: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PlanNode>,
}

/// Execution plan details for EXPLAIN queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainPlan {
    pub plan_type: String,
    pub collection: String,
    pub index_used: Option<String>,
    pub execution_time_us: u64,
    pub documents_examined: usize,
    pub documents_returned: usize,
    pub is_unique: bool,
    pub estimated_cost_score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_selectivity_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq_scan_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_scan_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimization_rationale: Option<String>,
    #[serde(default)]
    pub is_analyze: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join_strategy: Option<String>,
    #[serde(default)]
    pub estimated_network_io_bytes: u64,
    #[serde(default)]
    pub actual_network_io_bytes: u64,
    #[serde(default)]
    pub cache_hits: usize,
    #[serde(default)]
    pub cache_misses: usize,
    #[serde(default)]
    pub shards_involved: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shard_metrics: Vec<ShardExecutionMetric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_tree: Option<PlanNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formatted_pg_tree: Option<String>,
}

/// Top-level AST statement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Statement {
    Find {
        collection: String,
        filter: Option<FilterExpr>,
        sort_by: Option<(String, i8)>, // (field, 1 = asc, -1 = desc)
        limit: Option<usize>,
        skip: Option<usize>,
        vector_search: Option<VectorSearchClause>,
        traverse: Option<TraverseClause>,
        #[serde(default)]
        joins: Vec<JoinClause>,
    },
    Insert {
        collection: String,
        documents: Vec<Document>,
    },
    Update {
        collection: String,
        filter: FilterExpr,
        updates: Vec<(String, Value)>,
    },
    Delete {
        collection: String,
        filter: FilterExpr,
    },
    Count {
        collection: String,
        filter: Option<FilterExpr>,
    },
    CreateCollection {
        name: String,
    },
    DropCollection {
        name: String,
    },
    CreateIndex {
        collection: String,
        field: String,
        unique: bool,
    },
    DropIndex {
        collection: String,
        field: String,
    },
    Analyze {
        collection: String,
    },
    Explain {
        statement: Box<Statement>,
        #[serde(default)]
        analyze: bool,
        #[serde(default)]
        verbose: bool,
    },
    CreateEdge {
        from: String,
        to: String,
        relation: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        weight: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        properties: Option<Document>,
    },
    DeleteEdge {
        from: String,
        to: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        relation: Option<String>,
    },
    BeginTransaction,
    CommitTransaction,
    RollbackTransaction,
}

