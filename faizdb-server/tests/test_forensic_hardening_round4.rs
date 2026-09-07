//! Integration tests for Forensic Hardening Round 4
//! Verifies fixes for:
//! 1. MVCC atomic commit under race conditions
//! 2. WAL sequence continuity across empty rotated segments
//! 3. PostgreSQL extended query parameter substitution ($1 vs $10) and string safety
//! 4. Collection _id / id find and query sorting
//! 5. Hash Join NULL/missing key isolation and canonical stringification
//! 6. ANSI SQL WHERE operators (<>, IS NULL, IS NOT NULL, LIKE)

use std::fs::File;
use std::io::Write;
use std::sync::Arc;

use faizdb_core::document::model::{Document, Value};
use faizdb_core::storage::wal::Wal;
use faizdb_core::transaction::mvcc::TransactionManager;
use faizdb_query::executor::QueryResult;
use faizdb_query::parser::parse_query;
use faizdb_query::DatabaseContext;
use faizdb_server::wire::postgres::listener::substitute_postgres_params;

#[test]
fn test_mvcc_atomic_conflict_validation() {
    let mgr = TransactionManager::new();

    let mut txn1 = mgr.begin();
    let mut txn2 = mgr.begin();

    // Both write to the same key
    txn1.put(b"shared_account".to_vec(), b"100".to_vec()).unwrap();
    txn2.put(b"shared_account".to_vec(), b"200".to_vec()).unwrap();

    // First commit succeeds
    assert!(mgr.commit(&mut txn1).is_ok());

    // Second commit must fail atomically with TransactionConflict
    let err = mgr.commit(&mut txn2).unwrap_err();
    assert!(err.to_string().contains("modified by transaction"));
}

#[test]
fn test_wal_sequence_continuity_on_empty_rotated_segment() {
    let dir = tempfile::tempdir().unwrap();

    // 1. Open WAL and write some records in wal_000001.log
    {
        let wal = Wal::open(dir.path()).unwrap();
        let s0 = wal.log_put(b"k0", b"v0").unwrap();
        let s1 = wal.log_put(b"k1", b"v1").unwrap();
        let s2 = wal.log_put(b"k2", b"v2").unwrap();
        assert_eq!(s0, 0);
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
    }

    // 2. Simulate rotation right before crash: create empty wal_000002.log with only magic header
    let wal2_path = dir.path().join("wal_000002.log");
    {
        let mut f = File::create(&wal2_path).unwrap();
        f.write_all(b"FAIZWAL1").unwrap();
        f.sync_all().unwrap();
    }

    // 3. Re-open WAL: it should discover that wal_000002.log has 0 records,
    // and scan wal_000001.log to resume sequence from 3 (not reset to 0!)
    {
        let wal = Wal::open(dir.path()).unwrap();
        let s3 = wal.log_put(b"k3", b"v3").unwrap();
        assert_eq!(s3, 3, "Sequence number should continue from 3, not reset to 0");
    }
}

#[test]
fn test_postgres_parameter_substitution_and_bounds() {
    // 1. Test parameter substitution with >9 parameters and string literal protection
    let sql = "SELECT * FROM items WHERE id = $1 AND name = $10 AND category = $11 AND note = 'Price is $1 only' AND cost = $2";
    let mut params = Vec::new();
    for i in 1..=12 {
        params.push(format!("val_{}", i));
    }
    // Specific numeric value for cost ($2)
    params[1] = "49.99".to_string();

    let resolved = substitute_postgres_params(sql, &params);

    // Verify $10 is replaced with 'val_10', NOT mangled by $1 into 'val_1'0
    assert!(
        resolved.contains("name = 'val_10'"),
        "Expected name = 'val_10', got: {resolved}"
    );
    // Verify $11 is replaced with 'val_11'
    assert!(
        resolved.contains("category = 'val_11'"),
        "Expected category = 'val_11', got: {resolved}"
    );
    // Verify $1 is replaced with 'val_1'
    assert!(
        resolved.contains("id = 'val_1'"),
        "Expected id = 'val_1', got: {resolved}"
    );
    // Verify numeric $2 is parsed without quotes
    assert!(
        resolved.contains("cost = 49.99"),
        "Expected cost = 49.99, got: {resolved}"
    );
    // Verify $1 inside string literal is preserved!
    assert!(
        resolved.contains("note = 'Price is $1 only'"),
        "String literal should not have $1 replaced, got: {resolved}"
    );
}

#[test]
fn test_collection_id_find_and_query_sorting() {
    let db = Arc::new(DatabaseContext::new());
    let items_col = db.get_or_create_collection("items");

    let doc1 = Document::with_id("item_3").field("val", 30);
    let doc2 = Document::with_id("item_1").field("val", 10);
    let doc3 = Document::with_id("item_2").field("val", 20);

    items_col.insert(doc1).unwrap();
    items_col.insert(doc2).unwrap();
    items_col.insert(doc3).unwrap();

    // 1. Verify finding by _id and id on Collection
    let res1 = items_col
        .find(&[("_id".to_string(), Value::String("item_1".into()))], None, None)
        .unwrap();
    assert_eq!(res1.len(), 1);
    assert_eq!(res1[0].id.as_str(), "item_1");

    let res2 = items_col
        .find(&[("id".to_string(), Value::String("item_2".into()))], None, None)
        .unwrap();
    assert_eq!(res2.len(), 1);
    assert_eq!(res2[0].id.as_str(), "item_2");

    // 2. Verify sorting by id ASC and DESC in DatabaseContext execute
    let asc_query = parse_query("SELECT * FROM items ORDER BY id ASC").unwrap();
    if let QueryResult::Documents(docs) = db.execute(asc_query).unwrap() {
        let ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["item_1", "item_2", "item_3"]);
    } else {
        panic!("Expected documents");
    }

    // Sort DESC by id
    let desc_query = parse_query("SELECT * FROM items ORDER BY id DESC").unwrap();
    if let QueryResult::Documents(docs) = db.execute(desc_query).unwrap() {
        let ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["item_3", "item_2", "item_1"]);
    } else {
        panic!("Expected documents");
    }
}

#[test]
fn test_hash_join_null_key_isolation() {
    let db = Arc::new(DatabaseContext::new());
    let users_col = db.get_or_create_collection("users");
    let orders_col = db.get_or_create_collection("orders");

    users_col.insert(Document::with_id("u1").field("name", "Alice")).unwrap();
    users_col.insert(Document::with_id("u2").field("name", "Bob")).unwrap();

    orders_col.insert(Document::with_id("o1").field("user_id", "u1").field("amount", 100)).unwrap();
    orders_col.insert(Document::with_id("o2").field("amount", 200)).unwrap(); // no user_id

    // 1. INNER JOIN: order o2 has no user_id, so it MUST NOT match any user!
    let join_stmt = parse_query("SELECT * FROM orders INNER JOIN users ON orders.user_id = users.id").unwrap();

    if let QueryResult::Documents(docs) = db.execute(join_stmt).unwrap() {
        assert_eq!(docs.len(), 1, "Only order o1 should match user u1 in INNER JOIN");
        assert_eq!(docs[0].id.as_str(), "o1");
    } else {
        panic!("Expected documents");
    }

    // 2. LEFT JOIN: order o2 has no user_id, but MUST still be returned with its fields
    let left_join_stmt = parse_query("SELECT * FROM orders LEFT JOIN users ON orders.user_id = users.id").unwrap();

    if let QueryResult::Documents(docs) = db.execute(left_join_stmt).unwrap() {
        assert_eq!(docs.len(), 2, "Both o1 and o2 must be returned in LEFT JOIN");
    } else {
        panic!("Expected documents");
    }
}

#[test]
fn test_sql_ansi_where_operators() {
    let db = Arc::new(DatabaseContext::new());
    let col = db.get_or_create_collection("articles");

    col.insert(Document::with_id("a1").field("title", "FaizDB Architecture").field("status", "published")).unwrap(); // deleted_at absent (NULL)
    col.insert(Document::with_id("a2").field("title", "Rust Performance Guide").field("status", "draft")).unwrap(); // deleted_at absent (NULL)
    col.insert(Document::with_id("a3").field("title", "Deprecated Post").field("status", "archived").field("deleted_at", "2026-01-01")).unwrap();

    // 1. Test <> (ANSI SQL not equal)
    let q1 = parse_query("SELECT * FROM articles WHERE status <> 'published'").unwrap();
    if let QueryResult::Documents(docs) = db.execute(q1).unwrap() {
        assert_eq!(docs.len(), 2);
        assert!(docs.iter().all(|d| d.id.as_str() != "a1"));
    }

    // 2. Test IS NULL
    let q2 = parse_query("SELECT * FROM articles WHERE deleted_at IS NULL").unwrap();
    if let QueryResult::Documents(docs) = db.execute(q2).unwrap() {
        assert_eq!(docs.len(), 2);
        let ids: Vec<&str> = docs.iter().map(|d| d.id.as_str()).collect();
        assert!(ids.contains(&"a1"));
        assert!(ids.contains(&"a2"));
    }

    // 3. Test IS NOT NULL
    let q3 = parse_query("SELECT * FROM articles WHERE deleted_at IS NOT NULL").unwrap();
    if let QueryResult::Documents(docs) = db.execute(q3).unwrap() {
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id.as_str(), "a3");
    }

    // 4. Test LIKE '%pattern%'
    let q4 = parse_query("SELECT * FROM articles WHERE title LIKE '%FaizDB%'").unwrap();
    if let QueryResult::Documents(docs) = db.execute(q4).unwrap() {
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id.as_str(), "a1");
    }

    // 5. Test LIKE '%Guide'
    let q5 = parse_query("SELECT * FROM articles WHERE title LIKE '%Guide'").unwrap();
    if let QueryResult::Documents(docs) = db.execute(q5).unwrap() {
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].id.as_str(), "a2");
    }
}
