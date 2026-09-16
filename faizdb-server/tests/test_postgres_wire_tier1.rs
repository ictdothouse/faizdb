use faizdb_core::document::model::{Document, Value as FaizValue};
use faizdb_query::DatabaseContext;
use faizdb_server::wire::postgres::codec::decode_pg_param;
use faizdb_server::wire::postgres::handler::{
    handle_postgres_query_state, infer_query_row_description,
};
use std::sync::Arc;

#[test]
fn test_extended_query_describe_inference() {
    let db = Arc::new(DatabaseContext::new());

    // 1. SELECT queries should return Some(fields)
    let desc_users = infer_query_row_description(&db, "SELECT * FROM users");
    assert!(
        desc_users.is_some(),
        "SELECT queries must produce RowDescription on Describe"
    );
    let fields = desc_users.unwrap();
    assert_eq!(fields[0].name, "_id");

    // 2. System catalog queries should return accurate metadata fields
    let desc_types = infer_query_row_description(&db, "SELECT typname, oid FROM pg_type");
    assert!(desc_types.is_some());
    let type_fields = desc_types.unwrap();
    assert_eq!(type_fields[0].name, "typname");
    assert_eq!(type_fields[1].name, "oid");

    let desc_settings =
        infer_query_row_description(&db, "SELECT current_setting('server_version_num')");
    assert!(desc_settings.is_some());

    // 3. Mutation and DDL queries should return None (mapping to NoData 'n' packet)
    assert!(infer_query_row_description(&db, "INSERT INTO users (_id) VALUES ('u1')").is_none());
    assert!(infer_query_row_description(&db, "UPDATE users SET name = 'Faiz'").is_none());
    assert!(infer_query_row_description(&db, "DELETE FROM users WHERE id = 'u1'").is_none());
    assert!(infer_query_row_description(&db, "BEGIN").is_none());
    assert!(infer_query_row_description(&db, "COMMIT").is_none());
}

#[test]
fn test_binary_parameter_decoding() {
    // Text format (0)
    let text_val = decode_pg_param(0, b"FaizDB", None);
    assert_eq!(text_val, "FaizDB");

    // Binary format (1) - 4-byte integer
    let int4_bytes = 42i32.to_be_bytes();
    let int4_val = decode_pg_param(1, &int4_bytes, None);
    assert_eq!(int4_val, "42");

    // Binary format (1) - 8-byte integer
    let int8_bytes = 1234567890i64.to_be_bytes();
    let int8_val = decode_pg_param(1, &int8_bytes, None);
    assert_eq!(int8_val, "1234567890");

    // Binary format (1) - 2-byte integer
    let int2_bytes = 15i16.to_be_bytes();
    let int2_val = decode_pg_param(1, &int2_bytes, None);
    assert_eq!(int2_val, "15");

    // Binary format (1) - boolean
    let bool_true = decode_pg_param(1, &[1u8], None);
    assert_eq!(bool_true, "true");
    let bool_false = decode_pg_param(1, &[0u8], None);
    assert_eq!(bool_false, "false");
}

#[test]
fn test_session_transaction_state_lifecycle() {
    let db = Arc::new(DatabaseContext::new());
    let mut txn_state: u8 = b'I'; // Idle

    // 1. BEGIN transitions to 'T'
    let begin_resp = handle_postgres_query_state(&db, "BEGIN", &mut txn_state);
    assert_eq!(txn_state, b'T');
    let begin_str = String::from_utf8_lossy(&begin_resp);
    assert!(begin_str.contains("BEGIN"));

    // 2. Successful query within transaction preserves 'T'
    let col = db.get_or_create_collection("accounts");
    let mut d = Document::new();
    d.set("balance", FaizValue::Integer(1000));
    col.insert(d).unwrap();

    let query_resp = handle_postgres_query_state(&db, "SELECT * FROM accounts", &mut txn_state);
    assert_eq!(txn_state, b'T');
    assert!(String::from_utf8_lossy(&query_resp).contains("1000"));

    // 3. Syntax error within transaction transitions to 'E' (Aborted transaction)
    let err_resp = handle_postgres_query_state(&db, "MALFORMED SQL QUERY !!!", &mut txn_state);
    assert_eq!(txn_state, b'E');
    assert!(String::from_utf8_lossy(&err_resp).contains("42601"));

    // 4. Subsequent queries while in 'E' state are rejected with standard 25P02
    let rejected_resp = handle_postgres_query_state(&db, "SELECT * FROM accounts", &mut txn_state);
    assert_eq!(txn_state, b'E');
    let rej_str = String::from_utf8_lossy(&rejected_resp);
    assert!(rej_str.contains("25P02"));
    assert!(rej_str.contains("current transaction is aborted"));

    // 5. ROLLBACK resets transaction state back to 'I'
    let rollback_resp = handle_postgres_query_state(&db, "ROLLBACK", &mut txn_state);
    assert_eq!(txn_state, b'I');
    assert!(String::from_utf8_lossy(&rollback_resp).contains("ROLLBACK"));

    // 6. Normal queries can now execute again in 'I' state
    let ok_resp = handle_postgres_query_state(&db, "SELECT 1", &mut txn_state);
    assert_eq!(txn_state, b'I');
    assert!(String::from_utf8_lossy(&ok_resp).contains("1"));
}

#[test]
fn test_postgres_orm_introspection_catalogs() {
    let db = Arc::new(DatabaseContext::new());
    let mut txn_state: u8 = b'I';

    // Populate collections
    let col = db.get_or_create_collection("products");
    let mut d = Document::new();
    d.set("title", FaizValue::String("FaizDB Enterprise".to_string()));
    d.set("price", FaizValue::Float(499.0));
    col.insert(d).unwrap();

    // 1. current_setting
    let v_num_resp = handle_postgres_query_state(
        &db,
        "SELECT current_setting('server_version_num')",
        &mut txn_state,
    );
    assert!(String::from_utf8_lossy(&v_num_resp).contains("160000"));

    let v_resp = handle_postgres_query_state(
        &db,
        "SELECT current_setting('server_version')",
        &mut txn_state,
    );
    assert!(String::from_utf8_lossy(&v_resp).contains("16.0"));

    // 2. pg_constraint (Primary key)
    let pkey_resp = handle_postgres_query_state(
        &db,
        "SELECT conname, contype, conrelid FROM pg_catalog.pg_constraint",
        &mut txn_state,
    );
    let pkey_str = String::from_utf8_lossy(&pkey_resp);
    assert!(pkey_str.contains("products_pkey"));
    assert!(pkey_str.contains("p"));

    // 3. pg_class (Ordinary table 'r')
    let class_resp = handle_postgres_query_state(
        &db,
        "SELECT relname, relkind, reltuples FROM pg_class",
        &mut txn_state,
    );
    let class_str = String::from_utf8_lossy(&class_resp);
    assert!(class_str.contains("products"));
    assert!(class_str.contains("r"));

    // 4. pg_attribute (Columns & OIDs)
    let attr_resp = handle_postgres_query_state(
        &db,
        "SELECT attname, atttypid, attnum FROM pg_attribute",
        &mut txn_state,
    );
    let attr_str = String::from_utf8_lossy(&attr_resp);
    assert!(attr_str.contains("_id"));
    assert!(attr_str.contains("title"));
    assert!(attr_str.contains("price"));

    // 5. pg_am (Access methods)
    let am_resp = handle_postgres_query_state(&db, "SELECT amname, oid FROM pg_am", &mut txn_state);
    let am_str = String::from_utf8_lossy(&am_resp);
    assert!(am_str.contains("btree"));
    assert!(am_str.contains("hnsw"));
}
