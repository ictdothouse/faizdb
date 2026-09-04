//! Integration tests for Cost-Based Optimizer (CBO), Histograms & Adaptive Plan Selection.

use faizdb_core::document::model::{Document, Value};
use faizdb_query::ast::{FilterExpr, Operator, Statement};
use faizdb_query::executor::{DatabaseContext, QueryResult};

#[test]
fn test_cbo_analyze_and_explain_plan() {
    let db = DatabaseContext::new();

    // 1. Populate 'metrics' collection with 500 documents
    let docs: Vec<Document> = (1..=500)
        .map(|i| {
            let mut d = Document::new();
            d.set("cpu_usage", (i % 100) as f64);
            d.set("node_id", format!("server_{}", i % 10));
            d
        })
        .collect();

    db.execute(Statement::Insert {
        collection: "metrics".to_string(),
        documents: docs,
    })
    .unwrap();

    // 2. Create secondary index on cpu_usage
    db.execute(Statement::CreateIndex {
        collection: "metrics".to_string(),
        field: "cpu_usage".to_string(),
        unique: false,
    })
    .unwrap();

    // 3. Run ANALYZE to collect table statistics and build histograms
    let analyze_res = db
        .execute(Statement::Analyze {
            collection: "metrics".to_string(),
        })
        .unwrap();

    match analyze_res {
        QueryResult::Success(msg) => {
            assert!(
                msg.contains("metrics"),
                "Analyze message should mention collection name"
            );
            assert!(
                msg.contains("500 documents"),
                "Analyze message should mention doc count"
            );
        }
        _ => panic!("Expected QueryResult::Success from ANALYZE"),
    }

    // 4. Run EXPLAIN on equality query (low selectivity -> should pick IndexScan)
    let explain_res = db
        .execute(Statement::Explain(Box::new(Statement::Find {
            collection: "metrics".to_string(),
            filter: Some(FilterExpr::Field {
                field: "cpu_usage".to_string(),
                op: Operator::Eq,
                value: Value::Float(50.0),
            }),
            sort_by: None,
            limit: None,
            skip: None,
            vector_search: None,
            traverse: None,
            joins: Vec::new(),
        })))
        .unwrap();

    match explain_res {
        QueryResult::Explain(plan) => {
            assert!(
                plan.plan_type.starts_with("IndexScan"),
                "Low selectivity query should use IndexScan, got: {}",
                plan.plan_type
            );
            assert!(plan.index_used.is_some());
            assert!(plan.estimated_cost_score > 0.0);
            assert!(plan.estimated_selectivity_pct.is_some());
            assert!(plan.optimization_rationale.is_some());
        }
        _ => panic!("Expected QueryResult::Explain"),
    }
}
