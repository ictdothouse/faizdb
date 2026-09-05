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
        .execute(Statement::Explain {
            statement: Box::new(Statement::Find {
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
            }),
            analyze: false,
            verbose: false,
        })
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
            assert!(!plan.is_analyze);
            assert!(plan.formatted_pg_tree.is_some());
        }
        _ => panic!("Expected QueryResult::Explain"),
    }
}

#[test]
fn test_distributed_explain_analyze_parser_and_strategies() {
    let db = DatabaseContext::new();

    // Insert test data for orders and customers with colocated hash tag {tenant_1}
    for i in 1..=20 {
        let mut ord = Document::new();
        ord.set("customer_id", Value::Integer(i as i64));
        ord.set("amount", Value::Float(100.0 * i as f64));
        db.execute(Statement::Insert {
            collection: "{tenant_1}:orders".to_string(),
            documents: vec![ord],
        })
        .unwrap();

        let mut cust = Document::new();
        cust.set("id", Value::Integer(i as i64));
        cust.set("name", Value::String(format!("Customer {i}")));
        db.execute(Statement::Insert {
            collection: "{tenant_1}:customers".to_string(),
            documents: vec![cust],
        })
        .unwrap();
    }

    // 1. Test parsing of EXPLAIN ANALYZE
    let parsed = faizdb_query::parse_query(
        "EXPLAIN ANALYZE SELECT * FROM {tenant_1}:orders JOIN {tenant_1}:customers ON {tenant_1}:orders.customer_id = {tenant_1}:customers.id",
    )
    .expect("Should parse EXPLAIN ANALYZE SQL query");

    if let Statement::Explain { analyze, statement, .. } = &parsed {
        assert!(*analyze, "Analyze flag should be true");
        if let Statement::Find { joins, collection, .. } = &**statement {
            assert_eq!(collection, "{tenant_1}:orders");
            assert_eq!(joins.len(), 1);
        } else {
            panic!("Expected Find statement inside Explain");
        }
    } else {
        panic!("Expected Statement::Explain");
    }

    // 2. Execute EXPLAIN ANALYZE on colocated join -> should detect ColocatedHashJoin & 0 Network I/O
    let res = db.execute(parsed).unwrap();
    match res {
        QueryResult::Explain(plan) => {
            assert!(plan.is_analyze);
            assert_eq!(plan.join_strategy.as_deref(), Some("ColocatedHashJoin"));
            assert_eq!(plan.estimated_network_io_bytes, 0);
            assert_eq!(plan.actual_network_io_bytes, 0);
            assert!(plan.warning.is_none());
            assert_eq!(plan.shard_metrics.len(), 4);
            assert_eq!(plan.shard_metrics[0].cache_hit_pct, 100.0);
            assert_eq!(plan.shard_metrics[0].network_transfer_bytes, 0);
            assert!(plan.node_tree.is_some());
            let pg_tree = plan.formatted_pg_tree.expect("formatted_pg_tree must exist");
            assert!(pg_tree.contains("ColocatedHashJoin"));
            assert!(pg_tree.contains("Estimated Network I/O: 0 bytes"));
        }
        _ => panic!("Expected QueryResult::Explain"),
    }

    // 3. Test parsing of EXPLAIN (ANALYZE, VERBOSE) syntax
    let parsed_opt = faizdb_query::parse_query(
        "EXPLAIN (ANALYZE, VERBOSE) SELECT * FROM {tenant_1}:orders",
    )
    .expect("Should parse EXPLAIN (ANALYZE, VERBOSE)");

    if let Statement::Explain { analyze, verbose, .. } = parsed_opt {
        assert!(analyze);
        assert!(verbose);
    } else {
        panic!("Expected Explain with analyze and verbose");
    }
}
