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
    PG_TYPE_INT2, PG_TYPE_INT8, PG_TYPE_JSONB, PG_TYPE_TEXT,
};

/// Handles a raw SQL string received via PostgreSQL protocol 'Q' message with explicit transaction state:
/// - b'I': Idle (outside transaction)
/// - b'T': In active transaction block
/// - b'E': In aborted/failed transaction block (all commands rejected until ROLLBACK)
pub fn handle_postgres_query_state(
    db: &Arc<DatabaseContext>,
    query_str: &str,
    txn_state: &mut u8,
) -> Vec<u8> {
    let trimmed = query_str.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        let mut out = encode_empty_query_response();
        out.extend_from_slice(&encode_ready_for_query(*txn_state));
        return out;
    }

    let upper = trimmed.to_uppercase();

    // In failed transaction state (b'E'), reject any command except ROLLBACK or COMMIT
    if *txn_state == b'E' {
        if upper == "ROLLBACK"
            || upper == "ROLLBACK TRANSACTION"
            || upper == "ROLLBACK WORK"
            || upper.starts_with("ROLLBACK TO")
        {
            *txn_state = b'I';
            let mut out = encode_command_complete("ROLLBACK");
            out.extend_from_slice(&encode_ready_for_query(b'I'));
            return out;
        }
        if upper == "COMMIT"
            || upper == "COMMIT TRANSACTION"
            || upper == "COMMIT WORK"
            || upper == "END"
        {
            *txn_state = b'I';
            let mut out = encode_command_complete("ROLLBACK");
            out.extend_from_slice(&encode_ready_for_query(b'I'));
            return out;
        }
        let mut out = encode_error_response(
            "ERROR",
            "25P02",
            "current transaction is aborted, commands ignored until end of transaction block",
        );
        out.extend_from_slice(&encode_ready_for_query(b'E'));
        return out;
    }

    // 1. Handle SET statements (e.g. SET client_encoding = 'UTF8', SET extra_float_digits = 3, SET NAMES 'utf8')
    if upper.starts_with("SET ") || upper == "SET" {
        let mut out = encode_command_complete("SET");
        out.extend_from_slice(&encode_ready_for_query(*txn_state));
        return out;
    }

    // 2. Handle RESET / DISCARD statements
    if upper.starts_with("RESET ") || upper.starts_with("DISCARD ") {
        let mut out = encode_command_complete("RESET");
        out.extend_from_slice(&encode_ready_for_query(*txn_state));
        return out;
    }

    // 3. Handle Transactions
    if upper == "BEGIN" || upper == "BEGIN TRANSACTION" || upper.starts_with("START TRANSACTION") {
        *txn_state = b'T';
        let mut out = encode_command_complete("BEGIN");
        out.extend_from_slice(&encode_ready_for_query(b'T'));
        return out;
    }
    if upper == "COMMIT"
        || upper == "COMMIT TRANSACTION"
        || upper == "COMMIT WORK"
        || upper == "END"
    {
        *txn_state = b'I';
        let mut out = encode_command_complete("COMMIT");
        out.extend_from_slice(&encode_ready_for_query(b'I'));
        return out;
    }
    if upper == "ROLLBACK" || upper == "ROLLBACK TRANSACTION" || upper == "ROLLBACK WORK" {
        *txn_state = b'I';
        let mut out = encode_command_complete("ROLLBACK");
        out.extend_from_slice(&encode_ready_for_query(b'I'));
        return out;
    }

    // 4. Handle SHOW statements
    if upper.starts_with("SHOW ") {
        let param = trimmed[5..].trim().to_lowercase();
        return handle_show_variable(db, &param, *txn_state);
    }

    // 5. Handle PostgreSQL Introspection & System Information Queries
    if upper.starts_with("SELECT") {
        let has_from = upper.contains(" FROM ");

        if !has_from {
            // SELECT current_setting(...)
            if upper.contains("CURRENT_SETTING") {
                let setting_val = if upper.contains("SERVER_VERSION_NUM") {
                    "160000"
                } else if upper.contains("SERVER_VERSION") {
                    "16.0"
                } else if upper.contains("CLIENT_ENCODING") {
                    "UTF8"
                } else if upper.contains("STANDARD_CONFORMING_STRINGS") {
                    "on"
                } else if upper.contains("TIMEZONE") {
                    "UTC"
                } else {
                    "on"
                };
                return single_value_result("current_setting", setting_val, *txn_state);
            }

            // SELECT version()
            if upper.contains("VERSION()") {
                return single_value_result(
                    "version",
                    "PostgreSQL 16.0 (FaizDB Universal Safe-Rust Engine v0.1.0)",
                    *txn_state,
                );
            }

            // SELECT current_schema(), current_database(), current_user
            if upper.contains("CURRENT_SCHEMA") {
                return single_value_result("current_schema", "public", *txn_state);
            }
            if upper.contains("CURRENT_DATABASE") {
                return single_value_result("current_database", "faizdb", *txn_state);
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
                return single_value_result("current_user", "postgres", *txn_state);
            }

            // Standalone format_type
            if upper.contains("FORMAT_TYPE") {
                return single_value_result("format_type", "text", *txn_state);
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
                return single_value_result(col_name, "1", *txn_state);
            }
        }

        // Introspection: Database listing
        if upper.contains("PG_DATABASE") || upper.contains("PG_CATALOG.PG_DATABASE") {
            return handle_pg_database(*txn_state);
        }

        // Introspection: Namespace / schema listing
        if upper.contains("PG_NAMESPACE") || upper.contains("PG_CATALOG.PG_NAMESPACE") {
            return handle_pg_namespace(*txn_state);
        }

        // Introspection: Type catalog listing (Prisma / Drizzle handshake)
        if upper.contains("PG_TYPE") || upper.contains("PG_CATALOG.PG_TYPE") {
            return handle_pg_type(*txn_state);
        }

        // Introspection: Constraints (primary keys, foreign keys)
        if upper.contains("PG_CONSTRAINT") || upper.contains("TABLE_CONSTRAINTS") {
            return handle_pg_constraint(db, *txn_state);
        }

        // Introspection: Relations & tables (pg_class)
        if upper.contains("PG_CLASS") {
            return handle_pg_class(db, *txn_state);
        }

        // Introspection: Table attributes & columns (pg_attribute)
        if upper.contains("PG_ATTRIBUTE") {
            return handle_pg_attribute(db, *txn_state);
        }

        // Introspection: Access methods (pg_am)
        if upper.contains("PG_AM") {
            return handle_pg_am(*txn_state);
        }

        // Introspection: Comments, descriptions, enums, ranges (empty catalogs)
        if upper.contains("PG_DESCRIPTION")
            || upper.contains("PG_ENUM")
            || upper.contains("PG_RANGE")
        {
            return handle_empty_catalog("pg_description", *txn_state);
        }

        // Introspection: Column listing
        if upper.contains("INFORMATION_SCHEMA.COLUMNS") {
            return handle_list_columns(db, *txn_state);
        }

        // Introspection: Table listing from information_schema or pg_catalog
        if upper.contains("INFORMATION_SCHEMA.TABLES")
            || upper.contains("PG_TABLES")
            || upper.contains("PG_CATALOG.PG_TABLES")
        {
            return handle_list_tables(db, *txn_state);
        }

        // Introspection: Statistics for user tables
        if upper.contains("PG_STAT_USER_TABLES") || upper.contains("PG_STAT_ALL_TABLES") {
            return handle_pg_stat_user_tables(db, *txn_state);
        }

        // Introspection: Indexes listing
        if upper.contains("PG_INDEXES") {
            return handle_pg_indexes(db, *txn_state);
        }

        // Introspection: Configuration / settings
        if upper.contains("PG_SETTINGS") {
            return handle_pg_settings(*txn_state);
        }
    }

    // 6. Execute general SQL query through `faizdb-query`
    match parse_query(trimmed) {
        Ok(stmt) => match db.execute(stmt) {
            Ok(result) => format_query_result(result, *txn_state),
            Err(exec_err) => {
                if *txn_state == b'T' {
                    *txn_state = b'E'; // Transition to aborted transaction state
                }
                let mut out = encode_error_response("ERROR", "XX000", &exec_err);
                out.extend_from_slice(&encode_ready_for_query(*txn_state));
                out
            }
        },
        Err(parse_err) => {
            if *txn_state == b'T' {
                *txn_state = b'E'; // Transition to aborted transaction state
            }
            let mut out =
                encode_error_response("ERROR", "42601", &format!("SQL syntax error: {parse_err}"));
            out.extend_from_slice(&encode_ready_for_query(*txn_state));
            out
        }
    }
}

/// Backward-compatible adapter for handle_postgres_query taking &mut bool
pub fn handle_postgres_query(
    db: &Arc<DatabaseContext>,
    query_str: &str,
    in_transaction: &mut bool,
) -> Vec<u8> {
    let mut state = if *in_transaction { b'T' } else { b'I' };
    let resp = handle_postgres_query_state(db, query_str, &mut state);
    *in_transaction = state == b'T';
    resp
}

/// Handle SHOW queries (e.g. SHOW client_encoding, SHOW TABLES)
fn handle_show_variable(db: &Arc<DatabaseContext>, var: &str, txn_state: u8) -> Vec<u8> {
    if var == "tables" || var == "collections" {
        return handle_list_tables(db, txn_state);
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

    single_value_result(var, val, txn_state)
}

/// Handle table / collection listing for GUI tools (DBeaver, TablePlus, psql \dt)
fn handle_list_tables(db: &Arc<DatabaseContext>, txn_state: u8) -> Vec<u8> {
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
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_database listing
fn handle_pg_database(txn_state: u8) -> Vec<u8> {
    let fields = vec![
        PgField::text("datname"),
        PgField {
            name: "oid".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
    ];
    let mut out = encode_row_description(&fields);
    let row = vec![Some("faizdb".to_string()), Some("16384".to_string())];
    out.extend_from_slice(&encode_data_row(&row));
    out.extend_from_slice(&encode_command_complete("SELECT 1"));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_namespace listing
fn handle_pg_namespace(txn_state: u8) -> Vec<u8> {
    let fields = vec![
        PgField::text("nspname"),
        PgField {
            name: "oid".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
    ];
    let mut out = encode_row_description(&fields);
    let namespaces = [
        ("public", "2200"),
        ("pg_catalog", "11"),
        ("information_schema", "12345"),
    ];
    for (nsp, oid) in namespaces {
        let row = vec![Some(nsp.to_string()), Some(oid.to_string())];
        out.extend_from_slice(&encode_data_row(&row));
    }
    out.extend_from_slice(&encode_command_complete(&format!(
        "SELECT {}",
        namespaces.len()
    )));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_type listing for driver and ORM type handshakes (Prisma, Drizzle, SQLAlchemy)
fn handle_pg_type(txn_state: u8) -> Vec<u8> {
    let fields = vec![
        PgField::text("typname"),
        PgField {
            name: "oid".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
        PgField {
            name: "typarray".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
    ];
    let mut out = encode_row_description(&fields);
    let types = [
        ("bool", "16", "1000"),
        ("int2", "21", "1005"),
        ("int4", "23", "1007"),
        ("int8", "20", "1016"),
        ("float4", "700", "1021"),
        ("float8", "701", "1022"),
        ("text", "25", "1009"),
        ("varchar", "1043", "1015"),
        ("jsonb", "3802", "3807"),
        ("vector", "16390", "16391"),
    ];
    for (name, oid, arr) in types {
        let row = vec![
            Some(name.to_string()),
            Some(oid.to_string()),
            Some(arr.to_string()),
        ];
        out.extend_from_slice(&encode_data_row(&row));
    }
    out.extend_from_slice(&encode_command_complete(&format!("SELECT {}", types.len())));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle constraint listing for Prisma and Drizzle ORM schema reflection
fn handle_pg_constraint(db: &Arc<DatabaseContext>, txn_state: u8) -> Vec<u8> {
    let collections = db.list_collections();
    let fields = vec![
        PgField::text("conname"),
        PgField::text("contype"),
        PgField {
            name: "conrelid".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
    ];
    let mut out = encode_row_description(&fields);
    let mut count = 0;
    for col in &collections {
        let row = vec![
            Some(format!("{col}_pkey")),
            Some("p".to_string()), // 'p' = primary key constraint
            Some("16384".to_string()),
        ];
        out.extend_from_slice(&encode_data_row(&row));
        count += 1;
    }
    out.extend_from_slice(&encode_command_complete(&format!("SELECT {count}")));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_class listing for table discovery (relkind = 'r')
fn handle_pg_class(db: &Arc<DatabaseContext>, txn_state: u8) -> Vec<u8> {
    let collections = db.list_collections();
    let fields = vec![
        PgField::text("relname"),
        PgField {
            name: "relnamespace".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
        PgField::text("relkind"),
        PgField {
            name: "reltuples".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_FLOAT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
    ];
    let mut out = encode_row_description(&fields);
    for col_name in &collections {
        let col = db.get_or_create_collection(col_name);
        let count = col.count(&[]);
        let row = vec![
            Some(col_name.clone()),
            Some("2200".to_string()), // public namespace OID
            Some("r".to_string()),    // ordinary table
            Some(count.to_string()),
        ];
        out.extend_from_slice(&encode_data_row(&row));
    }
    out.extend_from_slice(&encode_command_complete(&format!(
        "SELECT {}",
        collections.len()
    )));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_attribute listing for column attributes and metadata
fn handle_pg_attribute(db: &Arc<DatabaseContext>, txn_state: u8) -> Vec<u8> {
    let collections = db.list_collections();
    let fields = vec![
        PgField {
            name: "attrelid".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
        PgField::text("attname"),
        PgField {
            name: "atttypid".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
        PgField {
            name: "attnum".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT2,
            type_size: 2,
            type_modifier: -1,
            format_code: 0,
        },
        PgField {
            name: "attnotnull".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_BOOL,
            type_size: 1,
            type_modifier: -1,
            format_code: 0,
        },
    ];
    let mut out = encode_row_description(&fields);
    let mut count = 0;
    for col_name in &collections {
        // Primary key _id
        let row_id = vec![
            Some("16384".to_string()),
            Some("_id".to_string()),
            Some(PG_TYPE_TEXT.to_string()),
            Some("1".to_string()),
            Some("t".to_string()),
        ];
        out.extend_from_slice(&encode_data_row(&row_id));
        count += 1;

        let col = db.get_or_create_collection(col_name);
        let sample_docs = col.find_paginated(0, 10);
        let mut attnum = 2;
        let mut seen = std::collections::HashSet::new();
        seen.insert("_id".to_string());
        seen.insert("id".to_string());
        for doc in sample_docs {
            for (k, v) in doc.fields {
                if seen.insert(k.clone()) {
                    let type_oid = match v {
                        Value::Boolean(_) => PG_TYPE_BOOL,
                        Value::Integer(_) => PG_TYPE_INT8,
                        Value::Float(_) => PG_TYPE_FLOAT8,
                        Value::String(_) => PG_TYPE_TEXT,
                        Value::Array(_) | Value::Object(_) => PG_TYPE_JSONB,
                        _ => PG_TYPE_TEXT,
                    };
                    let row = vec![
                        Some("16384".to_string()),
                        Some(k),
                        Some(type_oid.to_string()),
                        Some(attnum.to_string()),
                        Some("f".to_string()),
                    ];
                    out.extend_from_slice(&encode_data_row(&row));
                    count += 1;
                    attnum += 1;
                }
            }
        }
    }
    out.extend_from_slice(&encode_command_complete(&format!("SELECT {count}")));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_am listing (Access methods: btree, hash, hnsw)
fn handle_pg_am(txn_state: u8) -> Vec<u8> {
    let fields = vec![
        PgField::text("amname"),
        PgField {
            name: "oid".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
    ];
    let mut out = encode_row_description(&fields);
    let ams = [("btree", "403"), ("hash", "405"), ("hnsw", "16395")];
    for (name, oid) in ams {
        let row = vec![Some(name.to_string()), Some(oid.to_string())];
        out.extend_from_slice(&encode_data_row(&row));
    }
    out.extend_from_slice(&encode_command_complete(&format!("SELECT {}", ams.len())));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle empty catalogs (pg_description, pg_enum, pg_range)
fn handle_empty_catalog(_name: &str, txn_state: u8) -> Vec<u8> {
    let fields = vec![
        PgField {
            name: "objoid".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
        PgField::text("description"),
    ];
    let mut out = encode_row_description(&fields);
    out.extend_from_slice(&encode_command_complete("SELECT 0"));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle column listing for GUI tools and ORM schema reflection
fn handle_list_columns(db: &Arc<DatabaseContext>, txn_state: u8) -> Vec<u8> {
    let collections = db.list_collections();
    let fields = vec![
        PgField::text("table_name"),
        PgField::text("column_name"),
        PgField::text("data_type"),
        PgField::text("is_nullable"),
    ];
    let mut out = encode_row_description(&fields);
    let mut count = 0;
    for col_name in &collections {
        let col = db.get_or_create_collection(col_name);
        let row_id = vec![
            Some(col_name.clone()),
            Some("_id".to_string()),
            Some("text".to_string()),
            Some("NO".to_string()),
        ];
        out.extend_from_slice(&encode_data_row(&row_id));
        count += 1;

        let sample_docs = col.find_paginated(0, 10);
        let mut discovered_fields = std::collections::BTreeMap::new();
        for doc in &sample_docs {
            for (k, v) in &doc.fields {
                if k != "_id" && k != "id" {
                    discovered_fields
                        .entry(k.clone())
                        .or_insert_with(|| match v {
                            Value::Boolean(_) => "boolean",
                            Value::Integer(_) => "bigint",
                            Value::Float(_) => "double precision",
                            Value::String(_) => "text",
                            Value::Array(_) | Value::Object(_) => "jsonb",
                            _ => "text",
                        });
                }
            }
        }

        for (col_k, col_type) in discovered_fields {
            let row = vec![
                Some(col_name.clone()),
                Some(col_k),
                Some(col_type.to_string()),
                Some("YES".to_string()),
            ];
            out.extend_from_slice(&encode_data_row(&row));
            count += 1;
        }
    }
    out.extend_from_slice(&encode_command_complete(&format!("SELECT {count}")));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_stat_user_tables for monitoring tools
fn handle_pg_stat_user_tables(db: &Arc<DatabaseContext>, txn_state: u8) -> Vec<u8> {
    let collections = db.list_collections();
    let fields = vec![
        PgField::text("relname"),
        PgField {
            name: "n_live_tup".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
        PgField {
            name: "n_dead_tup".to_string(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid: PG_TYPE_INT8,
            type_size: 8,
            type_modifier: -1,
            format_code: 0,
        },
    ];
    let mut out = encode_row_description(&fields);
    for col_name in &collections {
        let col = db.get_or_create_collection(col_name);
        let count = col.count(&[]);
        let row = vec![
            Some(col_name.clone()),
            Some(count.to_string()),
            Some("0".to_string()),
        ];
        out.extend_from_slice(&encode_data_row(&row));
    }
    out.extend_from_slice(&encode_command_complete(&format!(
        "SELECT {}",
        collections.len()
    )));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_indexes listing
fn handle_pg_indexes(db: &Arc<DatabaseContext>, txn_state: u8) -> Vec<u8> {
    let collections = db.list_collections();
    let fields = vec![
        PgField::text("schemaname"),
        PgField::text("tablename"),
        PgField::text("indexname"),
        PgField::text("indexdef"),
    ];
    let mut out = encode_row_description(&fields);
    let mut count = 0;
    for col_name in &collections {
        let row = vec![
            Some("public".to_string()),
            Some(col_name.clone()),
            Some(format!("{col_name}_pkey")),
            Some(format!(
                "CREATE UNIQUE INDEX {col_name}_pkey ON public.{col_name} USING btree (_id)"
            )),
        ];
        out.extend_from_slice(&encode_data_row(&row));
        count += 1;
    }
    out.extend_from_slice(&encode_command_complete(&format!("SELECT {count}")));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Handle pg_settings listing
fn handle_pg_settings(txn_state: u8) -> Vec<u8> {
    let fields = vec![
        PgField::text("name"),
        PgField::text("setting"),
        PgField::text("unit"),
        PgField::text("category"),
    ];
    let mut out = encode_row_description(&fields);
    let settings = [
        ("server_version", "16.0", "", "Version"),
        ("server_version_num", "160000", "", "Version"),
        ("client_encoding", "UTF8", "", "Client Connection Defaults"),
        ("server_encoding", "UTF8", "", "Client Connection Defaults"),
        (
            "standard_conforming_strings",
            "on",
            "",
            "Client Connection Defaults",
        ),
        (
            "max_connections",
            "10000",
            "",
            "Connections and Authentication",
        ),
        ("shared_buffers", "128MB", "MB", "Resource Usage"),
    ];
    for (name, setting, unit, cat) in settings {
        let row = vec![
            Some(name.to_string()),
            Some(setting.to_string()),
            Some(unit.to_string()),
            Some(cat.to_string()),
        ];
        out.extend_from_slice(&encode_data_row(&row));
    }
    out.extend_from_slice(&encode_command_complete(&format!(
        "SELECT {}",
        settings.len()
    )));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Helper to return a single-column, single-row result
fn single_value_result(col_name: &str, value: &str, txn_state: u8) -> Vec<u8> {
    let fields = vec![PgField::text(col_name)];
    let mut out = encode_row_description(&fields);

    let row = vec![Some(value.to_string())];
    out.extend_from_slice(&encode_data_row(&row));

    out.extend_from_slice(&encode_command_complete("SELECT 1"));
    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Formats a FaizDB QueryResult into PostgreSQL DataRow / CommandComplete packets
fn format_query_result(result: QueryResult, txn_state: u8) -> Vec<u8> {
    let mut out = Vec::new();

    match result {
        QueryResult::Documents(docs) => {
            if docs.is_empty() {
                let fields = vec![PgField::text("id")];
                out.extend_from_slice(&encode_row_description(&fields));
                out.extend_from_slice(&encode_command_complete("SELECT 0"));
            } else {
                let mut col_names = Vec::new();
                let mut col_types = Vec::new();

                col_names.push("_id".to_string());
                col_types.push(PG_TYPE_TEXT);

                let mut seen = std::collections::HashSet::new();
                seen.insert("_id".to_string());
                seen.insert("id".to_string());

                for doc in &docs {
                    for (k, v) in doc.fields.iter() {
                        if seen.insert(k.clone()) {
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
                    }
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

            let lines: Vec<String> = if let Some(ref pg_tree) = plan.formatted_pg_tree {
                pg_tree.lines().map(|s| s.to_string()).collect()
            } else {
                vec![
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
                ]
            };

            for line in lines {
                out.extend_from_slice(&encode_data_row(&[Some(line)]));
            }

            out.extend_from_slice(&encode_command_complete("EXPLAIN"));
        }
    }

    out.extend_from_slice(&encode_ready_for_query(txn_state));
    out
}

/// Executes a query for PostgreSQL Extended Query Protocol ('E' message) without trailing ReadyForQuery ('Z')
pub fn handle_postgres_execute_query_state(
    db: &Arc<DatabaseContext>,
    query_str: &str,
    txn_state: &mut u8,
) -> Vec<u8> {
    let mut out = handle_postgres_query_state(db, query_str, txn_state);
    if out.len() >= 6 && out[out.len() - 6] == b'Z' {
        out.truncate(out.len() - 6);
    }
    out
}

/// Backward-compatible adapter for handle_postgres_execute_query taking &mut bool
pub fn handle_postgres_execute_query(
    db: &Arc<DatabaseContext>,
    query_str: &str,
    in_transaction: &mut bool,
) -> Vec<u8> {
    let mut state = if *in_transaction { b'T' } else { b'I' };
    let resp = handle_postgres_execute_query_state(db, query_str, &mut state);
    *in_transaction = state == b'T';
    resp
}

/// Infers RowDescription for a query without executing it, used by PostgreSQL Describe ('D') messages.
/// Returns Some(fields) for queries that return rows (e.g. SELECT, SHOW, EXPLAIN),
/// or None for queries that return no rows (INSERT, UPDATE, DELETE, DDL, TRANSACTIONS).
pub fn infer_query_row_description(
    db: &Arc<DatabaseContext>,
    query_str: &str,
) -> Option<Vec<PgField>> {
    let trimmed = query_str.trim().trim_end_matches(';').trim();
    if trimmed.is_empty() {
        return None;
    }
    let upper = trimmed.to_uppercase();

    if upper.starts_with("SHOW ") {
        let var = trimmed[5..].trim().to_lowercase();
        if var == "tables" || var == "collections" {
            return Some(vec![
                PgField::text("table_name"),
                PgField::text("table_schema"),
                PgField::text("table_type"),
            ]);
        }
        return Some(vec![PgField::text(var)]);
    }

    if upper.starts_with("EXPLAIN") {
        return Some(vec![PgField::text("QUERY PLAN")]);
    }

    if !upper.starts_with("SELECT") {
        return None;
    }

    let has_from = upper.contains(" FROM ");

    if !has_from {
        if upper.contains("VERSION()") {
            return Some(vec![PgField::text("version")]);
        }
        if upper.contains("CURRENT_SCHEMA") {
            return Some(vec![PgField::text("current_schema")]);
        }
        if upper.contains("CURRENT_DATABASE") {
            return Some(vec![PgField::text("current_database")]);
        }
        if upper.contains("CURRENT_USER") || upper.contains("USER") {
            return Some(vec![PgField::text("current_user")]);
        }
        if upper.contains("CURRENT_SETTING") {
            return Some(vec![PgField::text("current_setting")]);
        }
        let col_name = if upper.contains(" AS ") {
            trimmed.split_whitespace().last().unwrap_or("?column?")
        } else {
            "?column?"
        };
        return Some(vec![PgField::text(col_name)]);
    }

    if upper.contains("PG_DATABASE") {
        return Some(vec![
            PgField::text("datname"),
            PgField::new("oid", PG_TYPE_INT8),
        ]);
    }
    if upper.contains("PG_NAMESPACE") {
        return Some(vec![
            PgField::text("nspname"),
            PgField::new("oid", PG_TYPE_INT8),
        ]);
    }
    if upper.contains("PG_TYPE") {
        return Some(vec![
            PgField::text("typname"),
            PgField::new("oid", PG_TYPE_INT8),
            PgField::new("typarray", PG_TYPE_INT8),
        ]);
    }
    if upper.contains("PG_CONSTRAINT") || upper.contains("TABLE_CONSTRAINTS") {
        return Some(vec![
            PgField::text("conname"),
            PgField::text("contype"),
            PgField::new("conrelid", PG_TYPE_INT8),
        ]);
    }
    if upper.contains("PG_CLASS") {
        return Some(vec![
            PgField::text("relname"),
            PgField::new("relnamespace", PG_TYPE_INT8),
            PgField::text("relkind"),
            PgField::new("reltuples", PG_TYPE_FLOAT8),
        ]);
    }
    if upper.contains("PG_ATTRIBUTE") {
        return Some(vec![
            PgField::new("attrelid", PG_TYPE_INT8),
            PgField::text("attname"),
            PgField::new("atttypid", PG_TYPE_INT8),
            PgField::new("attnum", PG_TYPE_INT2),
            PgField::new("attnotnull", PG_TYPE_BOOL),
        ]);
    }
    if upper.contains("PG_AM") {
        return Some(vec![
            PgField::text("amname"),
            PgField::new("oid", PG_TYPE_INT8),
        ]);
    }
    if upper.contains("PG_INDEXES") {
        return Some(vec![
            PgField::text("schemaname"),
            PgField::text("tablename"),
            PgField::text("indexname"),
            PgField::text("indexdef"),
        ]);
    }
    if upper.contains("PG_SETTINGS") {
        return Some(vec![
            PgField::text("name"),
            PgField::text("setting"),
            PgField::text("unit"),
            PgField::text("category"),
        ]);
    }
    if upper.contains("PG_STAT_USER_TABLES") || upper.contains("PG_STAT_ALL_TABLES") {
        return Some(vec![
            PgField::text("relname"),
            PgField::new("n_live_tup", PG_TYPE_INT8),
            PgField::new("n_dead_tup", PG_TYPE_INT8),
        ]);
    }
    if upper.contains("COLUMNS") {
        return Some(vec![
            PgField::text("table_name"),
            PgField::text("column_name"),
            PgField::text("data_type"),
            PgField::text("is_nullable"),
        ]);
    }
    if upper.contains("TABLES") {
        return Some(vec![
            PgField::text("table_name"),
            PgField::text("table_schema"),
            PgField::text("table_type"),
        ]);
    }

    // General collection query: extract collection name
    if let Some(from_idx) = upper.find(" FROM ") {
        let after_from = trimmed[from_idx + 6..].trim();
        let col_name = after_from
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(|c| c == ';' || c == '"' || c == '`');
        if !col_name.is_empty() {
            let col = db.get_or_create_collection(col_name);
            let mut fields = vec![PgField::text("_id")];
            let sample = col.find_paginated(0, 5);
            let mut seen = std::collections::HashSet::new();
            seen.insert("_id".to_string());
            seen.insert("id".to_string());
            for doc in sample {
                for (k, v) in doc.fields {
                    if seen.insert(k.clone()) {
                        let type_oid = match v {
                            Value::Boolean(_) => PG_TYPE_BOOL,
                            Value::Integer(_) => PG_TYPE_INT8,
                            Value::Float(_) => PG_TYPE_FLOAT8,
                            Value::String(_) => PG_TYPE_TEXT,
                            Value::Array(_) | Value::Object(_) => PG_TYPE_JSONB,
                            _ => PG_TYPE_TEXT,
                        };
                        fields.push(PgField::new(k, type_oid));
                    }
                }
            }
            return Some(fields);
        }
    }

    Some(vec![PgField::text("id")])
}
