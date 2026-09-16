//! Integration tests for Phase B:
//! - Tokenizer-driven Pratt/Recursive-Descent ExprParser
//! - Multi-column ORDER BY execution
//! - CBO Cost Models (SSTable sparse index scan, HNSW vector search vs flat scan)
//! - Complex nested WHERE logic with BETWEEN, LIKE, IS NULL, AND, OR, NOT

use faizdb_core::document::model::{Document, Value};
use faizdb_query::ast::{FilterExpr, Operator, Statement};
use faizdb_query::executor::DatabaseContext;
use faizdb_query::optimizer::{CostModel, QueryOptimizer, TableStatistics};
use faizdb_query::parser::parse_query;
use faizdb_query::tokenizer::ExprParser;

#[test]
fn test_phase_b_expr_parser_deeply_nested() {
    let where_clause = "((status = 'active' OR status = 'pending') AND (age >= 18 OR score BETWEEN 80 AND 100)) AND NOT (is_banned = true)";
    let filter =
        ExprParser::parse_from_str(where_clause).expect("Should parse nested boolean expression");

    // Matching document: active, 20, banned=false
    let mut doc_match = Document::new();
    doc_match.set("status", "active");
    doc_match.set("age", 20);
    doc_match.set("score", 70);
    doc_match.set("is_banned", false);
    assert!(filter.matches(&doc_match));

    // Matching document: pending, 16, score=95, banned=false
    let mut doc_match2 = Document::new();
    doc_match2.set("status", "pending");
    doc_match2.set("age", 16);
    doc_match2.set("score", 95);
    doc_match2.set("is_banned", false);
    assert!(filter.matches(&doc_match2));

    // Rejected document: status=inactive
    let mut doc_reject1 = Document::new();
    doc_reject1.set("status", "inactive");
    doc_reject1.set("age", 25);
    doc_reject1.set("score", 95);
    doc_reject1.set("is_banned", false);
    assert!(!filter.matches(&doc_reject1));

    // Rejected document: is_banned=true
    let mut doc_reject2 = Document::new();
    doc_reject2.set("status", "active");
    doc_reject2.set("age", 25);
    doc_reject2.set("score", 95);
    doc_reject2.set("is_banned", true);
    assert!(!filter.matches(&doc_reject2));
}

#[test]
fn test_phase_b_multi_column_order_by_execution() {
    let ctx = DatabaseContext::new();

    // Create employees table
    let create_stmt = parse_query("CREATE TABLE employees").unwrap();
    ctx.execute(create_stmt).unwrap();

    // Insert test employees
    let employees = vec![
        ("e1", "Engineering", 120_000),
        ("e2", "Engineering", 150_000),
        ("e3", "Engineering", 90_000),
        ("e4", "Design", 110_000),
        ("e5", "Design", 130_000),
        ("e6", "Marketing", 100_000),
    ];

    for (id, dept, salary) in employees {
        let mut doc = Document::new();
        doc.id = faizdb_core::document::model::DocumentId::from_string(id);
        doc.set("department", dept);
        doc.set("salary", salary);
        ctx.execute(Statement::Insert {
            collection: "employees".to_string(),
            documents: vec![doc],
        })
        .unwrap();
    }

    // Query with Multi-Column ORDER BY: department ASC, salary DESC
    let select_sql = "SELECT * FROM employees ORDER BY department ASC, salary DESC";
    let stmt = parse_query(select_sql).expect("Should parse multi-column order by query");

    let result = ctx.execute(stmt).expect("Query should succeed");
    let docs = match result {
        faizdb_query::QueryResult::Documents(d) => d,
        _ => panic!("Expected documents result"),
    };

    assert_eq!(docs.len(), 6);

    // Expected order:
    // 1. Design, 130000 (e5)
    // 2. Design, 110000 (e4)
    // 3. Engineering, 150000 (e2)
    // 4. Engineering, 120000 (e1)
    // 5. Engineering, 90000 (e3)
    // 6. Marketing, 100000 (e6)

    assert_eq!(docs[0].get("department").unwrap().as_str(), Some("Design"));
    assert_eq!(docs[0].get("salary").unwrap().as_i64(), Some(130_000));

    assert_eq!(docs[1].get("department").unwrap().as_str(), Some("Design"));
    assert_eq!(docs[1].get("salary").unwrap().as_i64(), Some(110_000));

    assert_eq!(
        docs[2].get("department").unwrap().as_str(),
        Some("Engineering")
    );
    assert_eq!(docs[2].get("salary").unwrap().as_i64(), Some(150_000));

    assert_eq!(
        docs[3].get("department").unwrap().as_str(),
        Some("Engineering")
    );
    assert_eq!(docs[3].get("salary").unwrap().as_i64(), Some(120_000));

    assert_eq!(
        docs[4].get("department").unwrap().as_str(),
        Some("Engineering")
    );
    assert_eq!(docs[4].get("salary").unwrap().as_i64(), Some(90_000));

    assert_eq!(
        docs[5].get("department").unwrap().as_str(),
        Some("Marketing")
    );
    assert_eq!(docs[5].get("salary").unwrap().as_i64(), Some(100_000));
}

#[test]
fn test_phase_b_cbo_advanced_cost_modeling() {
    // 1. SSTable Sparse Index Cost Model
    let sparse_cost = CostModel::sstable_sparse_scan_cost(50_000, 4, 100);
    assert!(sparse_cost > 0.0);

    // 2. HNSW vs Flat Vector Scan
    let hnsw_decision_large = QueryOptimizer::choose_vector_plan(100_000, 384, 64, true);
    assert_eq!(hnsw_decision_large.chosen_plan, "HnswIndexScan");
    assert!(hnsw_decision_large.estimated_cost < hnsw_decision_large.seq_scan_cost);

    let hnsw_decision_tiny = QueryOptimizer::choose_vector_plan(8, 128, 64, true);
    assert_eq!(hnsw_decision_tiny.chosen_plan, "FlatVectorScan");

    // 3. Multi-Predicate Damped Selectivity
    let mut docs = Vec::new();
    for i in 1..=200 {
        let mut d = Document::new();
        d.set("age", i as f64);
        d.set("score", (i * 2) as f64);
        docs.push(d);
    }
    let stats = TableStatistics::analyze("students", &docs);

    let query_and = FilterExpr::And(vec![
        FilterExpr::Field {
            field: "age".to_string(),
            op: Operator::Gte,
            value: Value::Float(50.0),
        },
        FilterExpr::Field {
            field: "score".to_string(),
            op: Operator::Lte,
            value: Value::Float(300.0),
        },
    ]);
    let sel = QueryOptimizer::estimate_selectivity(&stats, &query_and);
    assert!(sel > 0.0 && sel <= 1.0);
}
