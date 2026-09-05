//! MySQL Wire Protocol Query Handler & Dispatcher.
//!
//! Processes MySQL COM_QUERY (0x03), COM_INIT_DB (0x02), COM_PING (0x0E),
//! executes queries against `faizdb-query` and `DatabaseContext`,
//! and formats responses as MySQL ResultSets or OK/ERR packets.

use bytes::{Bytes, BytesMut};
use faizdb_core::document::model::Value;
use faizdb_query::{parse_query, DatabaseContext, QueryResult};
use std::sync::Arc;

use super::codec::{
    build_column_def, build_eof_packet, build_err_packet, build_ok_packet, build_row_packet,
    encode_packet, put_lenenc_int, MYSQL_TYPE_LONGLONG, MYSQL_TYPE_VAR_STRING,
};

/// Handles a single MySQL COM_QUERY packet (0x03)
pub fn handle_mysql_query(
    db: &Arc<DatabaseContext>,
    current_db: &str,
    query_str: &str,
    start_seq_id: u8,
) -> Vec<Bytes> {
    let mut seq = start_seq_id;
    let trimmed = query_str.trim().trim_end_matches(';').trim();

    if trimmed.is_empty() {
        return vec![build_ok_packet(seq, 0, 0, "")];
    }

    let upper = trimmed.to_uppercase();

    // 1. Handle SET commands (SET NAMES ..., SET autocommit=1, SET time_zone=...)
    if upper.starts_with("SET ") || upper == "SET" {
        return vec![build_ok_packet(seq, 0, 0, "")];
    }

    // 2. Handle Transaction control
    if upper == "BEGIN" || upper == "START TRANSACTION" || upper.starts_with("BEGIN ") {
        return vec![build_ok_packet(seq, 0, 0, "")];
    }
    if upper == "COMMIT" || upper.starts_with("COMMIT ") {
        return vec![build_ok_packet(seq, 0, 0, "")];
    }
    if upper == "ROLLBACK" || upper.starts_with("ROLLBACK ") {
        return vec![build_ok_packet(seq, 0, 0, "")];
    }

    // 3. MySQL Driver Bootstrap Queries (Laravel / PHP PDO / MySQL CLI probe)
    if upper.contains("@@VERSION_COMMENT") {
        return make_single_value_result(
            &mut seq,
            current_db,
            "@@version_comment",
            "FaizDB Universal Multi-Model Engine (Safe Rust)",
        );
    }
    if upper.contains("@@VERSION") {
        return make_single_value_result(&mut seq, current_db, "@@version", "8.0.35-FaizDB");
    }
    if upper.contains("@@COLLATION_CONNECTION") {
        return make_single_value_result(
            &mut seq,
            current_db,
            "@@collation_connection",
            "utf8mb4_general_ci",
        );
    }
    if upper.contains("@@CHARACTER_SET_CLIENT") {
        return make_single_value_result(
            &mut seq,
            current_db,
            "@@character_set_client",
            "utf8mb4",
        );
    }
    if upper.contains("@@MAX_ALLOWED_PACKET") {
        return make_single_value_result(
            &mut seq,
            current_db,
            "@@max_allowed_packet",
            "67108864",
        );
    }
    if upper.contains("@@SQL_MODE") {
        return make_single_value_result(
            &mut seq,
            current_db,
            "@@sql_mode",
            "ONLY_FULL_GROUP_BY,STRICT_TRANS_TABLES,NO_ZERO_IN_DATE,NO_ZERO_DATE,ERROR_FOR_DIVISION_BY_ZERO,NO_ENGINE_SUBSTITUTION",
        );
    }
    if upper.contains("@@TIME_ZONE") || upper.contains("@@SYSTEM_TIME_ZONE") {
        return make_single_value_result(&mut seq, current_db, "@@time_zone", "SYSTEM");
    }
    if upper.contains("@@TX_ISOLATION") || upper.contains("@@TRANSACTION_ISOLATION") {
        return make_single_value_result(
            &mut seq,
            current_db,
            "@@transaction_isolation",
            "READ-COMMITTED",
        );
    }
    if upper == "SELECT DATABASE()" || upper == "SELECT DATABASE();" {
        return make_single_value_result(&mut seq, current_db, "DATABASE()", current_db);
    }
    if upper == "SELECT USER()" || upper == "SELECT CURRENT_USER()" {
        return make_single_value_result(&mut seq, current_db, "USER()", "root@localhost");
    }
    if upper == "SELECT 1" || upper == "SELECT 1;" || upper.starts_with("SELECT 1 AS") {
        return make_single_value_result(&mut seq, current_db, "1", "1");
    }

    // 4. SHOW DATABASES
    if upper == "SHOW DATABASES" || upper == "SHOW SCHEMAS" {
        let cols = vec!["Database".to_string()];
        let rows = vec![
            vec![Some(current_db.to_string())],
            vec![Some("information_schema".to_string())],
            vec![Some("performance_schema".to_string())],
        ];
        return format_result_set(&mut seq, current_db, "", &cols, &rows);
    }

    // 5. SHOW TABLES
    if upper.starts_with("SHOW TABLES") || upper.starts_with("SHOW FULL TABLES") {
        let collections = db.list_collections();
        let col_name = format!("Tables_in_{current_db}");
        let cols = vec![col_name];
        let rows: Vec<Vec<Option<String>>> = collections
            .into_iter()
            .map(|c| vec![Some(c)])
            .collect();
        return format_result_set(&mut seq, current_db, "", &cols, &rows);
    }

    // 6. General DDL / DQL / DML via `faizdb-query`
    let parsed = match parse_query(trimmed) {
        Ok(stmt) => stmt,
        Err(e) => {
            // Check if it's CREATE TABLE which we can handle gracefully
            if upper.starts_with("CREATE TABLE") {
                if let Some(table_name) = extract_table_name_from_create(&upper) {
                    let _ = db.get_or_create_collection(&table_name);
                    return vec![build_ok_packet(seq, 0, 0, "Table created")];
                }
            }
            return vec![build_err_packet(
                seq,
                1064,
                "42000",
                &format!("FaizDB MySQL syntax error: {e}"),
            )];
        }
    };

    match db.execute(parsed) {
        Ok(res) => match res {
            QueryResult::Documents(docs) => {
                if docs.is_empty() {
                    // Empty result set
                    let cols = vec!["result".to_string()];
                    let rows: Vec<Vec<Option<String>>> = Vec::new();
                    format_result_set(&mut seq, current_db, "", &cols, &rows)
                } else {
                    // Collect column names from first doc
                    let mut cols: Vec<String> = docs[0].fields.keys().cloned().collect();
                    if !cols.contains(&"_id".to_string()) {
                        cols.insert(0, "_id".to_string());
                    }

                    let mut rows: Vec<Vec<Option<String>>> = Vec::with_capacity(docs.len());
                    for doc in &docs {
                        let mut row = Vec::with_capacity(cols.len());
                        for col in &cols {
                            let val_opt = if col == "_id" || col == "id" {
                                Some(doc.id.as_str().to_string())
                            } else {
                                doc.fields.get(col).map(format_value_for_mysql)
                            };
                            row.push(val_opt);
                        }
                        rows.push(row);
                    }
                    format_result_set(&mut seq, current_db, "", &cols, &rows)
                }
            }
            QueryResult::Count(n) => {
                let cols = vec!["COUNT(*)".to_string()];
                let rows = vec![vec![Some(n.to_string())]];
                format_result_set(&mut seq, current_db, "", &cols, &rows)
            }
            QueryResult::Inserted(ids) => vec![build_ok_packet(seq, ids.len() as u64, 0, "")],
            QueryResult::Updated(n) => vec![build_ok_packet(seq, n as u64, 0, "")],
            QueryResult::Deleted(n) => vec![build_ok_packet(seq, n as u64, 0, "")],
            QueryResult::Success(msg) => vec![build_ok_packet(seq, 0, 0, &msg)],
            QueryResult::Explain(plan) => {
                let cols = vec!["EXPLAIN".to_string()];
                let plan_str = if let Some(ref pg_tree) = plan.formatted_pg_tree {
                    pg_tree.clone()
                } else {
                    format!("Plan: {} on collection: {}", plan.plan_type, plan.collection)
                };
                let rows = vec![vec![Some(plan_str)]];
                format_result_set(&mut seq, current_db, "", &cols, &rows)
            }
        },
        Err(e) => vec![build_err_packet(
            seq,
            1146,
            "42S02",
            &format!("FaizDB execution error: {e}"),
        )],
    }
}

/// Helper to create a single-row, single-column result set
fn make_single_value_result(
    seq: &mut u8,
    db: &str,
    col_name: &str,
    value: &str,
) -> Vec<Bytes> {
    let cols = vec![col_name.to_string()];
    let rows = vec![vec![Some(value.to_string())]];
    format_result_set(seq, db, "", &cols, &rows)
}

/// Formats a tabular result into a sequence of MySQL wire packets
pub fn format_result_set(
    seq: &mut u8,
    db: &str,
    table: &str,
    columns: &[String],
    rows: &[Vec<Option<String>>],
) -> Vec<Bytes> {
    let mut packets = Vec::with_capacity(3 + columns.len() + rows.len());

    // 1. Column Count packet
    let mut count_payload = BytesMut::with_capacity(8);
    put_lenenc_int(&mut count_payload, columns.len() as u64);
    packets.push(encode_packet(*seq, &count_payload));
    *seq = seq.wrapping_add(1);

    // 2. Column Definitions
    for col in columns {
        let col_type = if col.to_uppercase().contains("ID") || col.to_uppercase().contains("COUNT") {
            MYSQL_TYPE_LONGLONG
        } else {
            MYSQL_TYPE_VAR_STRING
        };
        packets.push(build_column_def(*seq, db, table, col, col_type, 255));
        *seq = seq.wrapping_add(1);
    }

    // 3. EOF Packet
    packets.push(build_eof_packet(*seq));
    *seq = seq.wrapping_add(1);

    // 4. Data Rows
    for row in rows {
        packets.push(build_row_packet(*seq, row));
        *seq = seq.wrapping_add(1);
    }

    // 5. Final EOF Packet
    packets.push(build_eof_packet(*seq));
    *seq = seq.wrapping_add(1);

    packets
}

/// Format document field value for MySQL text row
fn format_value_for_mysql(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Boolean(b) => if *b { "1".to_string() } else { "0".to_string() },
        Value::Integer(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => serde_json::to_string(arr).unwrap_or_default(),
        Value::Object(map) => serde_json::to_string(map).unwrap_or_default(),
        Value::DateTime(dt) => dt.to_rfc3339(),
        Value::Uuid(u) => u.to_string(),
        Value::Vector(vec) => format!("{vec:?}"),
        Value::Binary(b) => format!("<binary {} bytes>", b.len()),
    }
}

/// Extract table name from `CREATE TABLE [IF NOT EXISTS] <name> (...)`
fn extract_table_name_from_create(query: &str) -> Option<String> {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.len() < 3 {
        return None;
    }
    let mut idx = 2;
    if tokens[idx] == "IF" && tokens.len() > 5 && tokens[idx + 1] == "NOT" && tokens[idx + 2] == "EXISTS" {
        idx += 3;
    }
    if idx < tokens.len() {
        let raw = tokens[idx].trim_matches('`').trim_matches('"');
        let clean = raw.split('(').next()?.trim();
        return Some(clean.to_lowercase());
    }
    None
}
