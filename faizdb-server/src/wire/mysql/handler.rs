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

    // 5a. SHOW VARIABLES [LIKE '...']
    if upper.starts_with("SHOW VARIABLES") {
        let cols = vec!["Variable_name".to_string(), "Value".to_string()];
        let vars = [
            ("auto_increment_increment", "1"),
            ("autocommit", "ON"),
            ("character_set_client", "utf8mb4"),
            ("character_set_connection", "utf8mb4"),
            ("character_set_results", "utf8mb4"),
            ("character_set_server", "utf8mb4"),
            ("collation_connection", "utf8mb4_general_ci"),
            ("collation_server", "utf8mb4_general_ci"),
            ("init_connect", ""),
            ("interactive_timeout", "28800"),
            ("license", "GPL"),
            ("lower_case_table_names", "0"),
            ("max_allowed_packet", "67108864"),
            ("net_buffer_length", "16384"),
            ("net_write_timeout", "60"),
            (
                "sql_mode",
                "ONLY_FULL_GROUP_BY,STRICT_TRANS_TABLES,NO_ZERO_IN_DATE,NO_ZERO_DATE,ERROR_FOR_DIVISION_BY_ZERO,NO_ENGINE_SUBSTITUTION",
            ),
            ("system_time_zone", "UTC"),
            ("time_zone", "SYSTEM"),
            ("tx_isolation", "READ-COMMITTED"),
            ("version", "8.0.35-FaizDB"),
            ("version_comment", "FaizDB Universal Multi-Model Engine (Safe Rust)"),
            ("wait_timeout", "28800"),
        ];
        let filter = if let Some(pos) = upper.find("LIKE") {
            let pat = trimmed[pos + 4..]
                .trim()
                .trim_matches('\'')
                .trim_matches('"')
                .trim_matches('%')
                .to_lowercase();
            Some(pat)
        } else {
            None
        };
        let mut rows = Vec::new();
        for (k, v) in vars {
            if let Some(ref f) = filter {
                if !k.to_lowercase().contains(f) {
                    continue;
                }
            }
            rows.push(vec![Some(k.to_string()), Some(v.to_string())]);
        }
        return format_result_set(&mut seq, current_db, "", &cols, &rows);
    }

    // 5b. SHOW TABLE STATUS
    if upper.starts_with("SHOW TABLE STATUS") {
        let cols = vec![
            "Name".to_string(),
            "Engine".to_string(),
            "Version".to_string(),
            "Row_format".to_string(),
            "Rows".to_string(),
            "Avg_row_length".to_string(),
            "Data_length".to_string(),
            "Max_data_length".to_string(),
            "Index_length".to_string(),
            "Data_free".to_string(),
            "Auto_increment".to_string(),
            "Create_time".to_string(),
            "Update_time".to_string(),
            "Check_time".to_string(),
            "Collation".to_string(),
            "Checksum".to_string(),
            "Create_options".to_string(),
            "Comment".to_string(),
        ];
        let collections = db.list_collections();
        let mut rows = Vec::new();
        for c in collections {
            let col = db.get_or_create_collection(&c);
            let count = col.count(&[]);
            rows.push(vec![
                Some(c),
                Some("FaizDB".to_string()),
                Some("10".to_string()),
                Some("Dynamic".to_string()),
                Some(count.to_string()),
                Some("128".to_string()),
                Some((count * 128).to_string()),
                Some("0".to_string()),
                Some("4096".to_string()),
                Some("0".to_string()),
                None,
                None,
                None,
                None,
                Some("utf8mb4_general_ci".to_string()),
                None,
                Some("".to_string()),
                Some("".to_string()),
            ]);
        }
        return format_result_set(&mut seq, current_db, "", &cols, &rows);
    }

    // 5c. SHOW CREATE TABLE
    if upper.starts_with("SHOW CREATE TABLE ") {
        let raw_name = trimmed[18..]
            .trim()
            .trim_matches(|c| c == '`' || c == '"' || c == '\'');
        let table_name = raw_name.split_whitespace().next().unwrap_or(raw_name);
        let cols = vec!["Table".to_string(), "Create Table".to_string()];
        let create_sql = format!(
            "CREATE TABLE `{table_name}` (\n  `_id` varchar(64) NOT NULL PRIMARY KEY\n) ENGINE=FaizDB DEFAULT CHARSET=utf8mb4"
        );
        let rows = vec![vec![Some(table_name.to_string()), Some(create_sql)]];
        return format_result_set(&mut seq, current_db, "", &cols, &rows);
    }

    // 5d. SHOW INDEX FROM / SHOW INDEXES FROM / SHOW KEYS FROM
    if upper.starts_with("SHOW INDEX FROM ")
        || upper.starts_with("SHOW INDEXES FROM ")
        || upper.starts_with("SHOW KEYS FROM ")
    {
        let prefix_len = if upper.starts_with("SHOW INDEXES FROM ") {
            18
        } else if upper.starts_with("SHOW INDEX FROM ") {
            16
        } else {
            15
        };
        let raw_name = trimmed[prefix_len..]
            .trim()
            .trim_matches(|c| c == '`' || c == '"' || c == '\'');
        let table_name = raw_name.split_whitespace().next().unwrap_or(raw_name);
        let cols = vec![
            "Table".to_string(),
            "Non_unique".to_string(),
            "Key_name".to_string(),
            "Seq_in_index".to_string(),
            "Column_name".to_string(),
            "Collation".to_string(),
            "Cardinality".to_string(),
            "Sub_part".to_string(),
            "Packed".to_string(),
            "Null".to_string(),
            "Index_type".to_string(),
            "Comment".to_string(),
            "Index_comment".to_string(),
        ];
        let col = db.get_or_create_collection(table_name);
        let count = col.count(&[]);
        let rows = vec![vec![
            Some(table_name.to_string()),
            Some("0".to_string()),
            Some("PRIMARY".to_string()),
            Some("1".to_string()),
            Some("_id".to_string()),
            Some("A".to_string()),
            Some(count.to_string()),
            None,
            None,
            Some("".to_string()),
            Some("BTREE".to_string()),
            Some("".to_string()),
            Some("".to_string()),
        ]];
        return format_result_set(&mut seq, current_db, "", &cols, &rows);
    }

    // 5e. DESCRIBE / DESC / SHOW COLUMNS FROM
    if upper.starts_with("DESCRIBE ")
        || upper.starts_with("DESC ")
        || upper.starts_with("SHOW COLUMNS FROM ")
        || upper.starts_with("SHOW FIELDS FROM ")
    {
        let prefix_len = if upper.starts_with("SHOW COLUMNS FROM ") {
            18
        } else if upper.starts_with("SHOW FIELDS FROM ") {
            17
        } else if upper.starts_with("DESCRIBE ") {
            9
        } else {
            5
        };
        let raw_name = trimmed[prefix_len..]
            .trim()
            .trim_matches(|c| c == '`' || c == '"' || c == '\'');
        let table_name = raw_name.split_whitespace().next().unwrap_or(raw_name);

        let cols = vec![
            "Field".to_string(),
            "Type".to_string(),
            "Null".to_string(),
            "Key".to_string(),
            "Default".to_string(),
            "Extra".to_string(),
        ];
        let mut rows = Vec::new();
        rows.push(vec![
            Some("_id".to_string()),
            Some("varchar(64)".to_string()),
            Some("NO".to_string()),
            Some("PRI".to_string()),
            None,
            Some("".to_string()),
        ]);

        let col = db.get_or_create_collection(table_name);
        let samples = col.find_paginated(0, 10);
        let mut seen = std::collections::HashSet::new();
        for doc in samples {
            for (k, v) in doc.fields {
                if k != "_id" && k != "id" && seen.insert(k.clone()) {
                    let type_str = match v {
                        Value::Boolean(_) => "tinyint(1)",
                        Value::Integer(_) => "bigint",
                        Value::Float(_) => "double",
                        Value::String(_) => "text",
                        Value::Array(_) | Value::Object(_) => "json",
                        _ => "text",
                    };
                    rows.push(vec![
                        Some(k),
                        Some(type_str.to_string()),
                        Some("YES".to_string()),
                        Some("".to_string()),
                        None,
                        Some("".to_string()),
                    ]);
                }
            }
        }
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
                    // Heterogeneous NoSQL schema union: collect all unique column names across all returned documents
                    let mut seen = std::collections::HashSet::new();
                    let mut cols: Vec<String> = vec!["_id".to_string()];
                    seen.insert("_id".to_string());

                    for doc in &docs {
                        for k in doc.fields.keys() {
                            if seen.insert(k.clone()) {
                                cols.push(k.clone());
                            }
                        }
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
            QueryResult::Updated(n) => vec![build_ok_packet(seq, n, 0, "")],
            QueryResult::Deleted(n) => vec![build_ok_packet(seq, n, 0, "")],
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
        let col_type = if col.to_uppercase().contains("COUNT") || col == "1" {
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
