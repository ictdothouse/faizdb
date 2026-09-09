//! Enterprise Integration Tests: SQL Keyword Boundary Isolation, Compound Operators, and Vector Bounds
//!
//! Validates:
//! 1. Keyword boundary isolation: `UPDATE settings`, `UPDATE assets`, `CREATE INDEX idx_location ON ...`, `DELETE FROM warehouse`.
//! 2. SQL `WHERE col BETWEEN val1 AND val2` & `NOT BETWEEN`.
//! 3. SQL `WHERE col IN (...)` & `NOT IN (...)`.
//! 4. Compound `OR` & nested parenthesized conditions: `(status = 'active' OR role = 'admin') AND age >= 18`.
//! 5. `HnswIndex::try_search` boundary consistency when all vectors are tombstoned.
//! 6. MVCC `committed_writes` automated pruning on commit when inactive.

use std::sync::Arc;

use faizdb_core::document::model::{Document, Value};
use faizdb_core::transaction::mvcc::TransactionManager;
use faizdb_query::ast::{FilterExpr, Operator, Statement};
use faizdb_query::executor::QueryResult;
use faizdb_query::parser::parse_query;
use faizdb_query::DatabaseContext;
use faizdb_vector::distance::DistanceMetric;
use faizdb_vector::hnsw::{HnswConfig, HnswIndex};

#[test]
fn test_keyword_boundary_isolation_settings_and_assets() {
    // 1. UPDATE settings (contains 'SET')
    let stmt = parse_query("UPDATE settings SET active = true WHERE id = 'cfg_1'").unwrap();
    match stmt {
        Statement::Update {
            collection,
            filter,
            updates,
        } => {
            assert_eq!(collection, "settings");
            assert_eq!(updates.len(), 1);
            assert_eq!(updates[0].0, "active");
            assert_eq!(updates[0].1, Value::Boolean(true));
            assert_eq!(
                filter,
                FilterExpr::Field {
                    field: "id".to_string(),
                    op: Operator::Eq,
                    value: Value::String("cfg_1".to_string()),
                }
            );
        }
        _ => panic!("Expected Statement::Update"),
    }

    // 2. UPDATE assets (contains 'SET')
    let stmt = parse_query("UPDATE assets SET value = 500 WHERE id = 'ast_1'").unwrap();
    match stmt {
        Statement::Update {
            collection,
            updates,
            ..
        } => {
            assert_eq!(collection, "assets");
            assert_eq!(updates[0].0, "value");
            assert_eq!(updates[0].1, Value::Integer(500));
        }
        _ => panic!("Expected Statement::Update"),
    }

    // 3. CREATE INDEX with name containing 'ON' (e.g. idx_location, idx_person)
    let stmt = parse_query("CREATE INDEX idx_location ON cities(name)").unwrap();
    match stmt {
        Statement::CreateIndex {
            collection,
            field,
            unique,
        } => {
            assert_eq!(collection, "cities");
            assert_eq!(field, "name");
            assert!(!unique);
        }
        _ => panic!("Expected Statement::CreateIndex"),
    }

    // 4. DROP INDEX with name containing 'ON'
    let stmt = parse_query("DROP INDEX idx_location ON cities").unwrap();
    match stmt {
        Statement::DropIndex { collection, field } => {
            assert_eq!(collection, "cities");
            assert_eq!(field, "location");
        }
        _ => panic!("Expected Statement::DropIndex"),
    }

    // 5. DELETE FROM warehouse (contains 'WHERE')
    let stmt = parse_query("DELETE FROM warehouse WHERE id = 'wh_1'").unwrap();
    match stmt {
        Statement::Delete { collection, filter } => {
            assert_eq!(collection, "warehouse");
            assert_eq!(
                filter,
                FilterExpr::Field {
                    field: "id".to_string(),
                    op: Operator::Eq,
                    value: Value::String("wh_1".to_string()),
                }
            );
        }
        _ => panic!("Expected Statement::Delete"),
    }

    // 6. COUNT FROM warehouse WHERE id = 'wh_1'
    let stmt = parse_query("COUNT FROM warehouse WHERE id = 'wh_1'").unwrap();
    match stmt {
        Statement::Count { collection, filter } => {
            assert_eq!(collection, "warehouse");
            assert!(filter.is_some());
        }
        _ => panic!("Expected Statement::Count"),
    }
}

#[test]
fn test_sql_where_between_and_not_between() {
    let db = Arc::new(DatabaseContext::new());
    let col = db.get_or_create_collection("users_between");

    for (id, age) in [("u1", 15), ("u2", 18), ("u3", 24), ("u4", 30), ("u5", 35)] {
        col.insert(Document::with_id(id).field("age", age)).unwrap();
    }

    // Query BETWEEN 18 AND 30 (inclusive)
    let res = db
        .execute(parse_query("SELECT * FROM users_between WHERE age BETWEEN 18 AND 30").unwrap())
        .unwrap();
    if let QueryResult::Documents(docs) = res {
        assert_eq!(docs.len(), 3);
        let mut ages: Vec<i64> = docs
            .iter()
            .map(|d| match d.get("age").unwrap() {
                Value::Integer(i) => *i,
                _ => 0,
            })
            .collect();
        ages.sort();
        assert_eq!(ages, vec![18, 24, 30]);
    } else {
        panic!("Expected QueryResult::Documents");
    }

    // Query NOT BETWEEN 18 AND 30
    let res_not = db
        .execute(parse_query("SELECT * FROM users_between WHERE age NOT BETWEEN 18 AND 30").unwrap())
        .unwrap();
    if let QueryResult::Documents(docs_not) = res_not {
        assert_eq!(docs_not.len(), 2);
        let mut ages_not: Vec<i64> = docs_not
            .iter()
            .map(|d| match d.get("age").unwrap() {
                Value::Integer(i) => *i,
                _ => 0,
            })
            .collect();
        ages_not.sort();
        assert_eq!(ages_not, vec![15, 35]);
    } else {
        panic!("Expected QueryResult::Documents");
    }
}

#[test]
fn test_sql_where_in_and_not_in() {
    let db = Arc::new(DatabaseContext::new());
    let col = db.get_or_create_collection("users_in");

    for (id, status) in [
        ("s1", "active"),
        ("s2", "pending"),
        ("s3", "suspended"),
        ("s4", "deleted"),
    ] {
        col.insert(Document::with_id(id).field("status", status))
            .unwrap();
    }

    // Query WHERE status IN ('active', 'pending')
    let res = db
        .execute(parse_query("SELECT * FROM users_in WHERE status IN ('active', 'pending')").unwrap())
        .unwrap();
    if let QueryResult::Documents(docs) = res {
        assert_eq!(docs.len(), 2);
    } else {
        panic!("Expected QueryResult::Documents");
    }

    // Query WHERE status NOT IN ('suspended', 'deleted')
    let res_not = db
        .execute(
            parse_query("SELECT * FROM users_in WHERE status NOT IN ('suspended', 'deleted')")
                .unwrap(),
        )
        .unwrap();
    if let QueryResult::Documents(docs_not) = res_not {
        assert_eq!(docs_not.len(), 2);
        for doc in docs_not {
            let s = doc.get("status").unwrap().as_str().unwrap();
            assert!(s == "active" || s == "pending");
        }
    } else {
        panic!("Expected QueryResult::Documents");
    }
}

#[test]
fn test_sql_where_compound_or_and_nested_parens() {
    let db = Arc::new(DatabaseContext::new());
    let col = db.get_or_create_collection("users_compound");

    // Seed test users
    // u1: active, user, 20  -> Match: status == active && age >= 18
    // u2: inactive, admin, 25 -> Match: role == admin && age >= 18
    // u3: active, user, 16  -> Fail: age < 18
    // u4: inactive, user, 30 -> Fail: neither active nor admin
    let dataset = vec![
        ("u1", "active", "user", 20),
        ("u2", "inactive", "admin", 25),
        ("u3", "active", "user", 16),
        ("u4", "inactive", "user", 30),
    ];

    for (id, status, role, age) in dataset {
        col.insert(
            Document::with_id(id)
                .field("status", status)
                .field("role", role)
                .field("age", age),
        )
        .unwrap();
    }

    let q = "SELECT * FROM users_compound WHERE (status = 'active' OR role = 'admin') AND age >= 18";
    let res = db.execute(parse_query(q).unwrap()).unwrap();
    if let QueryResult::Documents(docs) = res {
        assert_eq!(docs.len(), 2);
        let mut ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, vec!["u1", "u2"]);
    } else {
        panic!("Expected QueryResult::Documents");
    }
}

#[test]
fn test_hnsw_try_search_on_all_tombstoned_nodes() {
    let config = HnswConfig::new(3, DistanceMetric::Euclidean);

    let mut index = HnswIndex::new(config);
    index.insert("v1", vec![1.0, 2.0, 3.0]).unwrap();
    index.insert("v2", vec![4.0, 5.0, 6.0]).unwrap();

    assert_eq!(index.len(), 2);
    assert!(!index.is_empty());

    // Tombstone delete all nodes
    assert!(index.delete("v1"));
    assert!(index.delete("v2"));

    // Index is empty of active vectors, but nodes vector still contains tombstones
    assert_eq!(index.len(), 0);
    assert!(index.is_empty());

    // try_search with any query vector should return Ok(empty) without panicking
    let search_res = index.try_search(&[0.0, 0.0, 0.0], 10).unwrap();
    assert!(search_res.is_empty());

    // search() should also return empty
    let regular_res = index.search(&[0.0, 0.0, 0.0], 10);
    assert!(regular_res.is_empty());
}

#[test]
fn test_mvcc_committed_writes_auto_prune() {
    let mgr = TransactionManager::new();

    // Run a series of transactions and commit them
    for i in 0..10 {
        let mut txn = mgr.begin();
        txn.put(format!("key_{i}").into_bytes(), b"val".to_vec())
            .unwrap();
        mgr.commit(&mut txn).unwrap();
    }

    // Since no transactions are active after each commit, committed_writes should be cleared automatically
    assert_eq!(mgr.active_count(), 0);
    // Begin a new transaction and verify it can write without conflicts
    let mut txn_new = mgr.begin();
    txn_new
        .put(b"key_0".to_vec(), b"val_updated".to_vec())
        .unwrap();
    assert!(mgr.commit(&mut txn_new).is_ok());
}
