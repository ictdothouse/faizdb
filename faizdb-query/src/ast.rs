//! Abstract Syntax Tree (AST) for FaizQL query language.

use serde::{Deserialize, Serialize};
use faizdb_core::document::model::{Document, Value};

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
                let actual = match doc.get_nested(field) {
                    Some(v) => v,
                    None => return false,
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
}

/// Graph Traversal Clause within a Query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraverseClause {
    pub start_id: String,
    pub max_depth: usize,
    pub relation: Option<String>,
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
    Explain(Box<Statement>),
    BeginTransaction,
    CommitTransaction,
    RollbackTransaction,
}
