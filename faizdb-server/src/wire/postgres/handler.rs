//! PostgreSQL Wire Protocol Query Handler.
//!
//! Processes PostgreSQL Frontend 'Q' (Simple Query) messages,
//! executes them against `faizdb-query` and `faizdb-core`,
//! and generates the appropriate binary response packets.

use faizdb_core::document::model::Value;
use faizdb_query::{parse_query, DatabaseContext, QueryResult};
use std::sync::Arc;

use super::codec::{
    encode_command_complete, encode_data_row, encode_empty_query_response, encode_error_response,
    encode_ready_for_query, encode_row_description, PgField, PG_TYPE_BOOL, PG_TYPE_FLOAT8,
    PG_TYPE_INT8, PG_TYPE_JSONB, PG_TYPE_TEXT,
};

/// Handles a raw SQL string received via PostgreSQL protocol 'Q' message.
/// Returns a byte stream containing the complete response (RowDescription, DataRows, CommandComplete, ReadyForQuery).
pub fn handle_postgres_query(
    db: &Arc<DatabaseContext>,
    query_str: &str,
    in_transaction: &mut bool,
) -> Vec<u8> {
    let trimmed = query_str.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        let mut out = encode_empty_query_response();
        out.extend_from_slice(&encode_ready_for_query(if *in_transaction {
            b'T'
        } else {
            b'I'
        }));
        return out;
    }

    let upper = trimmed.to_uppercase();

    // 1. Handle SET statements (e.g. SET client_encoding = 'UTF8', SET extra_float_digits = 3, SET NAMES 'utf8')
    if upper.starts_with("SET ") || upper == "SET" {
        let mut out = encode_command_complete("SET");
        out.extend_from_slice(&encode_ready_for_query(if *in_transaction {
            b'T'
        } else {
            b'I'
        }));
        return out;
    }

    // 2. Handle RESET / DISCARD statements
    if upper.starts_with("RESET ") || upper.starts_with("DISCARD ") {
        let mut out = encode_command_complete("RESET");
        out.extend_from_slice(&encode_ready_for_query(if *in_transaction {
            b'T'
        } else {
            b'I'
        }));
        return out;
    }

    // 3. Handle Transactions
    if upper == "BEGIN" || upper == "BEGIN TRANSACTION" || upper.starts_with("START TRANSACTION") {
        *in_transaction = true;
        let mut out = encode_command_complete("BEGIN");
        out.extend_from_slice(&encode_ready_for_query(b'T'));
        return out;
    }
    if upper == "COMMIT" || upper == "COMMIT TRANSACTION" || upper == "END" {
        *in_transaction = false;
        let mut out = encode_command_complete("COMMIT");
        out.extend_from_slice(&encode_ready_for_query(b'I'));
        return out;
    }
    if upper == "ROLLBACK" || upper == "ROLLBACK TRANSACTION" {
        *in_transaction = false;
        let mut out = encode_command_complete("ROLLBACK");
        out.extend_from_slice(&encode_ready_for_query(b'I'));
        return out;
    }

    // 4. Handle SHOW statements
    if upper.starts_with("SHOW ") {
        let param = trimmed[5..].trim().to_lowercase();
        return handle_show_variable(db, &param, *in_transaction);
    }

    // 5. Handle PostgreSQL Introspection & System Information Queries
    if upper.starts_with("SELECT") {
        let has_from = upper.contains(" FROM ");

        if !has_from {
            // SELECT version()
            if upper.contains("VERSION()") {
                return single_value_result(
                    "version",
                    "PostgreSQL 16.0 (FaizDB Universal Safe-Rust Engine v0.1.0)",
                    *in_transaction,
                );
            }

            // SELECT current_schema(), current_database(), current_user
            if upper.contains("CURRENT_SCHEMA") {
                return single_value_result("current_schema", "public", *in_transaction);
            }
            if upper.contains("CURRENT_DATABASE") {
                return single_value_result("current_database", "faizdb", *in_transaction);
            }
            if upper == "SELECT CURRENT_USER"
                || upper == "SELECT CURRENT_USER()"
                || upper == "SELECT USER"
                || upper == "SELECT USER()"
                || upper.starts_with("SELECT CURRENT_USER AS")
                || upper.starts_with("SELECT USER AS")
                || upper.starts_with("SELECT USER()")
                || upper.starts_with("SELECT CURRENT_USER()")
            {
                return single_value_result("current_user", "postgres", *in_transaction);
            }

            // SELECT 1 or SELECT 1 AS one
            if upper == "SELECT 1"
                || upper.starts_with("SELECT 1 AS")
                || upper.starts_with("SELECT 1 ")
            {
                let col_name = if upper.contains(" AS ") {
                    trimmed.split_whitespace().last().unwrap_or("?column?")
                } else {
                    "?column?"
                };
                return single_value_result(col_name, "1", *in_transaction);
            }
        }

        // Introspection: Table listing from information_schema or pg_catalog
        if upper.contains("INFORMATION_SCHEMA.TABLES")
            || upper.contains("PG_TABLES")
            || upper.contains("PG_CATALOG.PG_TABLES")
            || upper.contains("PG_CLASS")
        {
            return handle_list_tables(db, *in_transaction);
        }
    }

    // 6. Execute general SQL query through `faizdb-query`
    match parse_query(trimmed) {
        Ok(stmt) => match db.execute(stmt) {
            Ok(result) => format_query_result(result, *in_transaction),
            Err(exec_err) => {
                let mut out = encode_error_response("ERROR", "XX000", &exec_err);
                out.extend_from_slice(&encode_ready_for_query(if *in_transaction {
                    b'T'
                } else {
                    b'I'
                }));
                out
            }
        },
        Err(parse_err) => {
            let mut out =
                encode_error_response("ERROR", "42601", &format!("SQL syntax error: {parse_err}"));
            out.extend_from_slice(&encode_ready_for_query(if *in_transaction {
                b'T'
            } else {
                b'I'
            }));
            out
        }
    }
}

/// Handle SHOW queries (e.g. SHOW client_encoding, SHOW TABLES)
fn handle_show_variable(db: &Arc<DatabaseContext>, var: &str, in_txn: bool) -> Vec<u8> {
    if var == "tables" || var == "collections" {
        return handle_list_tables(db, in_txn);
    }

    let val = match var {
        "client_encoding" | "server_encoding" => "UTF8",
        "server_version" => "16.0 (FaizDB)",
        "transaction_isolation" => "read committed",
        "standard_conforming_strings" => "on",
        "datestyle" => "ISO, MDY",
        "timezone" => "UTC",
        "integer_datetimes" => "on",
        "max_connections" => "10000",
        _ => "off",
    };

    single_value_result(var, val, in_txn)
}

/// Handle table / collection listing for GUI tools (DBeaver, TablePlus, psql \dt)
fn handle_list_tables(db: &Arc<DatabaseContext>, in_txn: bool) -> Vec<u8> {
    let collections = db.list_collections();
    let fields = vec![
        PgField::text("table_name"),
        PgField::text("table_schema"),
        PgField::text("table_type"),
    ];

    let mut out = encode_row_description(&fields);

    for col in &collections {
        let row = vec![
            Some(col.clone()),
            Some("public".to_string()),
            Some("BASE TABLE".to_string()),
        ];
        out.extend_from_slice(&encode_data_row(&row));
    }

    let tag = format!("SELECT {}", collections.len());
    out.extend_from_slice(&encode_command_complete(&tag));
    out.extend_from_slice(&encode_ready_for_query(if in_txn { b'T' } else { b'I' }));
    out
}

/// Helper to return a single-column, single-row result
fn single_value_result(col_name: &str, value: &str, in_txn: bool) -> Vec<u8> {
    let fields = vec![PgField::text(col_name)];
    let mut out = encode_row_description(&fields);

    let row = vec![Some(value.to_string())];
    out.extend_from_slice(&encode_data_row(&row));

    out.extend_from_slice(&encode_command_complete("SELECT 1"));
    out.extend_from_slice(&encode_ready_for_query(if in_txn { b'T' } else { b'I' }));
    out
}

/// Formats a FaizDB QueryResult into PostgreSQL DataRow / CommandComplete packets
fn format_query_result(result: QueryResult, in_txn: bool) -> Vec<u8> {
    let mut out = Vec::new();

    match result {
        QueryResult::Documents(docs) => {
            if docs.is_empty() {
                // Return empty result with default column
                let fields = vec![PgField::text("id")];
                out.extend_from_slice(&encode_row_description(&fields));
                out.extend_from_slice(&encode_command_complete("SELECT 0"));
            } else {
                // Infer columns and types from the first document
                let mut col_names = Vec::new();
                let mut col_types = Vec::new();

                // Guarantee '_id' is first
                col_names.push("_id".to_string());
                col_types.push(PG_TYPE_TEXT);

                // Add other fields
                for (k, v) in docs[0].fields.iter() {
                    if k == "_id" || k == "id" {
                        continue;
                    }
                    col_names.push(k.clone());
                    let type_oid = match v {
                        Value::Boolean(_) => PG_TYPE_BOOL,
                        Value::Integer(_) => PG_TYPE_INT8,
                        Value::Float(_) => PG_TYPE_FLOAT8,
                        Value::String(_) => PG_TYPE_TEXT,
                        Value::Array(_) | Value::Object(_) => PG_TYPE_JSONB,
                        Value::Binary(_)
                        | Value::DateTime(_)
                        | Value::Uuid(_)
                        | Value::Vector(_)
                        | Value::Null => PG_TYPE_TEXT,
                    };
                    col_types.push(type_oid);
                }

                let fields: Vec<PgField> = col_names
                    .iter()
                    .zip(col_types.iter())
                    .map(|(name, &oid)| PgField::new(name.clone(), oid))
                    .collect();

                out.extend_from_slice(&encode_row_description(&fields));

                for doc in &docs {
                    let mut row_vals = Vec::with_capacity(col_names.len());
                    for name in &col_names {
                        let val_str = if name == "_id" || name == "id" {
                            Some(doc.id.as_str().to_string())
                        } else {
                            match doc.get(name) {
                                Some(Value::Null) | None => None,
                                Some(Value::String(s)) => Some(s.clone()),
                                Some(Value::Integer(i)) => Some(i.to_string()),
                                Some(Value::Float(f)) => Some(f.to_string()),
                                Some(Value::Boolean(b)) => Some(b.to_string()),
                                Some(Value::Array(arr)) => {
                                    Some(serde_json::to_string(arr).unwrap_or_default())
                                }
                                Some(Value::Object(obj)) => {
                                    Some(serde_json::to_string(obj).unwrap_or_default())
                                }
                                Some(Value::DateTime(dt)) => Some(dt.to_rfc3339()),
                                Some(Value::Uuid(u)) => Some(u.to_string()),
                                Some(Value::Vector(v)) => Some(format!("{v:?}")),
                                Some(Value::Binary(b)) => {
                                    Some(format!("<binary {} bytes>", b.len()))
                                }
                            }
                        };
                        row_vals.push(val_str);
                    }
                    out.extend_from_slice(&encode_data_row(&row_vals));
                }

                let tag = format!("SELECT {}", docs.len());
                out.extend_from_slice(&encode_command_complete(&tag));
            }
        }
        QueryResult::Inserted(ids) => {
            let tag = format!("INSERT 0 {}", ids.len());
            out.extend_from_slice(&encode_command_complete(&tag));
        }
        QueryResult::Updated(count) => {
            let tag = format!("UPDATE {}", count);
            out.extend_from_slice(&encode_command_complete(&tag));
        }
        QueryResult::Deleted(count) => {
            let tag = format!("DELETE {}", count);
            out.extend_from_slice(&encode_command_complete(&tag));
        }
        QueryResult::Count(c) => {
            let fields = vec![PgField::new("count", PG_TYPE_INT8)];
            out.extend_from_slice(&encode_row_description(&fields));
            let row = vec![Some(c.to_string())];
            out.extend_from_slice(&encode_data_row(&row));
            out.extend_from_slice(&encode_command_complete("SELECT 1"));
        }
        QueryResult::Success(msg) => {
            out.extend_from_slice(&encode_command_complete(&msg));
        }
        QueryResult::Explain(plan) => {
            let fields = vec![PgField::text("QUERY PLAN")];
            out.extend_from_slice(&encode_row_description(&fields));

            let lines = vec![
                format!("Plan: {}", plan.plan_type),
                format!("Collection: {}", plan.collection),
                format!(
                    "Index Used: {}",
                    plan.index_used
                        .unwrap_or_else(|| "None (Full Scan)".to_string())
                ),
                format!("Documents Examined: {}", plan.documents_examined),
                format!("Documents Returned: {}", plan.documents_returned),
                format!("Execution Time: {} µs", plan.execution_time_us),
                format!("Cost Score: {:.2}", plan.estimated_cost_score),
            ];

            for line in lines {
                out.extend_from_slice(&encode_data_row(&[Some(line)]));
            }

            out.extend_from_slice(&encode_command_complete("EXPLAIN"));
        }
    }

    out.extend_from_slice(&encode_ready_for_query(if in_txn { b'T' } else { b'I' }));
    out
}

/// Executes a query for PostgreSQL Extended Query Protocol ('E' message) without trailing ReadyForQuery ('Z')
pub fn handle_postgres_execute_query(
    db: &Arc<DatabaseContext>,
    query_str: &str,
    in_transaction: &mut bool,
) -> Vec<u8> {
    let mut out = handle_postgres_query(db, query_str, in_transaction);
    if out.len() >= 6 && out[out.len() - 6] == b'Z' {
        out.truncate(out.len() - 6);
    }
    out
}
