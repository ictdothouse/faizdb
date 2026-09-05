//! Parser for SQL, MongoDB, and FaizQL dialect statements into AST Statements.

use crate::ast::{FilterExpr, Operator, Statement, TraverseClause, VectorSearchClause};
use faizdb_core::document::model::{Document, Value};

/// Parse any supported query string into a [`Statement`]
pub fn parse_query(input: &str) -> Result<Statement, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Empty query".to_string());
    }

    let upper = trimmed.to_uppercase();

    // 0. EXPLAIN query execution plan
    if upper.starts_with("EXPLAIN") {
        let inner = trimmed[7..].trim();
        let inner_stmt = parse_query(inner)?;
        return Ok(Statement::Explain(Box::new(inner_stmt)));
    }

    // Transactions
    if upper == "BEGIN" || upper == "BEGIN TRANSACTION" {
        return Ok(Statement::BeginTransaction);
    }
    if upper == "COMMIT" || upper == "COMMIT TRANSACTION" {
        return Ok(Statement::CommitTransaction);
    }
    if upper == "ROLLBACK" || upper == "ROLLBACK TRANSACTION" {
        return Ok(Statement::RollbackTransaction);
    }

    // 1. MongoDB JS shell format: `db.collection.action(...)`
    if trimmed.starts_with("db.") {
        return parse_mongo_query(trimmed);
    }

    // 2. Index DDL: CREATE [UNIQUE] INDEX ... / DROP INDEX ...
    if upper.starts_with("CREATE") && upper.contains("INDEX") {
        return parse_create_index_query(trimmed);
    }

    if upper.starts_with("DROP") && upper.contains("INDEX") {
        return parse_drop_index_query(trimmed);
    }

    // 3. openCypher graph queries: MATCH ...
    if upper.starts_with("MATCH") {
        return parse_cypher_match(trimmed);
    }

    // 4. openCypher graph DDL/DML: CREATE (n:Person ...) or CREATE (a)-[:REL]->(b)
    if upper.starts_with("CREATE") && trimmed.contains('(') && !upper.contains("INDEX") {
        return parse_cypher_create(trimmed);
    }

    // 5. SQL / FaizQL format: SELECT / INSERT / DELETE / COUNT

    if upper.starts_with("SELECT") || upper.starts_with("FIND") {
        return parse_select_query(trimmed);
    }

    if upper.starts_with("INSERT") {
        return parse_insert_query(trimmed);
    }

    if upper.starts_with("UPDATE") {
        return parse_update_query(trimmed);
    }

    if upper.starts_with("DELETE") {
        return parse_delete_query(trimmed);
    }

    if upper.starts_with("COUNT") {
        return parse_count_query(trimmed);
    }

    Err(format!("Unrecognized query syntax: '{trimmed}'"))
}

/// Parse MongoDB syntax: `db.<collection>.<action>(<args>)`
fn parse_mongo_query(input: &str) -> Result<Statement, String> {
    let without_db = input.strip_prefix("db.").unwrap();
    let dot_pos = without_db
        .find('.')
        .ok_or("Expected collection name after 'db.'")?;
    let collection = without_db[..dot_pos].to_string();

    let remainder = &without_db[dot_pos + 1..];
    let (remainder_method, remainder_args, sort_args) =
        if let Some(sort_idx) = remainder.find(".sort(") {
            let first_call = remainder[..sort_idx].trim();
            let sort_part = remainder[sort_idx + 6..].trim();
            let sort_close = sort_part.rfind(')').unwrap_or(sort_part.len());
            let s_args = sort_part[..sort_close].trim().to_string();

            let p_open = first_call
                .find('(')
                .ok_or("Expected '(' after method name")?;
            let p_close = first_call
                .rfind(')')
                .ok_or("Expected ')' after method args")?;
            let m = first_call[..p_open].trim().to_string();
            let a = first_call[p_open + 1..p_close].trim().to_string();
            (m, a, Some(s_args))
        } else {
            let paren_open = remainder
                .find('(')
                .ok_or("Expected '(' after method name")?;
            let paren_close = remainder
                .rfind(')')
                .ok_or("Expected ')' at end of statement")?;
            let m = remainder[..paren_open].trim().to_string();
            let a = remainder[paren_open + 1..paren_close].trim().to_string();
            (m, a, None)
        };

    let method = remainder_method.as_str();
    let args_str = remainder_args.as_str();

    match method {
        "find" => {
            let (filter, traverse) = if args_str.is_empty() {
                (None, None)
            } else if let Ok(mut v) = serde_json::from_str::<serde_json::Value>(args_str) {
                let mut trav = None;
                if let Some(obj) = v.as_object_mut() {
                    if let Some(t_val) = obj.remove("$traverse") {
                        if let Some(from_id) = t_val.get("from").and_then(|x| x.as_str()) {
                            let depth =
                                t_val.get("depth").and_then(|x| x.as_u64()).unwrap_or(1) as usize;
                            let via = t_val
                                .get("via")
                                .and_then(|x| x.as_str())
                                .map(|s| s.to_string());
                            trav = Some(TraverseClause {
                                start_id: from_id.to_string(),
                                max_depth: depth,
                                relation: via,
                            });
                        }
                    }
                }
                let f = if v.as_object().is_some_and(|m| m.is_empty()) {
                    None
                } else {
                    Some(parse_json_filter(&v.to_string())?)
                };
                (f, trav)
            } else {
                (Some(parse_json_filter(args_str)?), None)
            };

            let sort_by = if let Some(ref s_str) = sort_args {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(s_str) {
                    if let Some(obj) = val.as_object() {
                        if let Some((field, dir_val)) = obj.iter().next() {
                            let dir = dir_val.as_i64().unwrap_or(1) as i8;
                            Some((field.clone(), dir))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };

            Ok(Statement::Find {
                collection,
                filter,
                sort_by,
                limit: None,
                skip: None,
                vector_search: None,
                traverse,
                joins: Vec::new(),
            })
        }
        "update" | "updateOne" | "updateMany" => {
            let wrapped = format!("[{args_str}]");
            let arr: serde_json::Value = serde_json::from_str(&wrapped)
                .map_err(|e| format!("Invalid JSON arguments in update: {e}"))?;
            let items = arr.as_array().ok_or("Expected JSON array of arguments")?;
            if items.is_empty() {
                return Err("update requires filter and update documents".to_string());
            }

            let filter = if let Some(f_val) = items.first() {
                if f_val.as_object().is_some_and(|m| m.is_empty()) {
                    FilterExpr::AlwaysTrue
                } else {
                    parse_json_filter(&f_val.to_string())?
                }
            } else {
                FilterExpr::AlwaysTrue
            };

            let mut updates = Vec::new();
            if let Some(u_val) = items.get(1) {
                if let Some(u_obj) = u_val.as_object() {
                    let fields_obj =
                        if let Some(set_val) = u_obj.get("$set").and_then(|s| s.as_object()) {
                            set_val
                        } else {
                            u_obj
                        };
                    for (k, v) in fields_obj {
                        updates.push((k.clone(), Value::from(v.clone())));
                    }
                }
            }

            Ok(Statement::Update {
                collection,
                filter,
                updates,
            })
        }
        "insert" | "insertOne" => {
            let doc_val: serde_json::Value = serde_json::from_str(args_str)
                .map_err(|e| format!("Invalid JSON document: {e}"))?;
            let doc = Document::from_json_value(doc_val).ok_or("Expected JSON object")?;
            Ok(Statement::Insert {
                collection,
                documents: vec![doc],
            })
        }
        "delete" | "deleteMany" | "deleteOne" => {
            let filter = parse_json_filter(args_str)?;
            Ok(Statement::Delete { collection, filter })
        }
        "count" | "countDocuments" => {
            let filter = if args_str.is_empty() {
                None
            } else {
                Some(parse_json_filter(args_str)?)
            };
            Ok(Statement::Count { collection, filter })
        }
        "createIndex" => {
            let val: serde_json::Value =
                serde_json::from_str(args_str).unwrap_or_else(|_| serde_json::json!({}));
            let field = val
                .as_object()
                .and_then(|o| o.keys().next().cloned())
                .unwrap_or_else(|| "id".to_string());
            let unique = args_str.to_lowercase().contains("unique")
                && args_str.to_lowercase().contains("true");
            Ok(Statement::CreateIndex {
                collection,
                field,
                unique,
            })
        }
        "dropIndex" => {
            let field = args_str
                .trim_matches(|c| c == '"' || c == '\'' || c == ' ')
                .to_string();
            Ok(Statement::DropIndex { collection, field })
        }
        _ => Err(format!("Unsupported MongoDB method: '{method}'")),
    }
}

/// Parse MongoDB JSON filter: e.g. `{"age": 30}` or `{"age": {"$gt": 25}}`
fn parse_json_filter(json_str: &str) -> Result<FilterExpr, String> {
    let val: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON filter: {e}"))?;

    let obj = match val {
        serde_json::Value::Object(map) => map,
        _ => return Err("Filter must be a JSON object".to_string()),
    };

    let mut and_clauses = Vec::new();

    for (k, v) in obj {
        if k == "$or" {
            if let serde_json::Value::Array(arr) = v {
                let mut or_clauses = Vec::new();
                for item in arr {
                    or_clauses.push(parse_json_filter(&item.to_string())?);
                }
                and_clauses.push(FilterExpr::Or(or_clauses));
            }
            continue;
        }

        if let serde_json::Value::Object(op_map) = v {
            for (op_str, op_val) in op_map {
                let op = match op_str.as_str() {
                    "$eq" => Operator::Eq,
                    "$ne" | "$neq" => Operator::Neq,
                    "$gt" => Operator::Gt,
                    "$gte" => Operator::Gte,
                    "$lt" => Operator::Lt,
                    "$lte" => Operator::Lte,
                    "$contains" => Operator::Contains,
                    "$startsWith" => Operator::StartsWith,
                    "$endsWith" => Operator::EndsWith,
                    "$in" => Operator::In,
                    _ => return Err(format!("Unsupported filter operator: '{op_str}'")),
                };
                and_clauses.push(FilterExpr::Field {
                    field: k.clone(),
                    op,
                    value: Value::from(op_val),
                });
            }
        } else {
            and_clauses.push(FilterExpr::Field {
                field: k,
                op: Operator::Eq,
                value: Value::from(v),
            });
        }
    }

    if and_clauses.is_empty() {
        Ok(FilterExpr::AlwaysTrue)
    } else if and_clauses.len() == 1 {
        Ok(and_clauses.pop().unwrap())
    } else {
        Ok(FilterExpr::And(and_clauses))
    }
}

/// Parse SQL SELECT or FaizQL FIND
fn parse_select_query(input: &str) -> Result<Statement, String> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 2 {
        return Err("Malformed SELECT / FIND statement".to_string());
    }

    let mut collection = String::new();
    let mut i = 0;

    if tokens[i].eq_ignore_ascii_case("SELECT") {
        i += 1;
        // Skip columns up to FROM
        while i < tokens.len() && !tokens[i].eq_ignore_ascii_case("FROM") {
            i += 1;
        }
        if i < tokens.len() && tokens[i].eq_ignore_ascii_case("FROM") {
            i += 1;
            if i < tokens.len() {
                collection = tokens[i].trim_matches(';').to_string();
                i += 1;
            }
        }
    } else if tokens[i].eq_ignore_ascii_case("FIND") {
        i += 1;
        if i < tokens.len() {
            collection = tokens[i].to_string();
            i += 1;
        }
    }

    if collection.is_empty() {
        return Err("Could not extract collection name".to_string());
    }

    let mut filter = None;
    let mut limit = None;
    let mut skip = None;
    let mut vector_search = None;
    let mut traverse = None;
    let mut sort_by = None;
    let mut joins = Vec::new();

    while i < tokens.len() {
        let token_upper = tokens[i].to_uppercase();

        if token_upper == "WHERE" {
            i += 1;
            let mut where_tokens = Vec::new();
            while i < tokens.len() {
                let next_upper = tokens[i].to_uppercase();
                if [
                    "LIMIT", "SKIP", "OFFSET", "ORDER", "VECTOR", "TRAVERSE", "JOIN", "INNER",
                    "LEFT",
                ]
                .contains(&next_upper.as_str())
                {
                    break;
                }
                where_tokens.push(tokens[i]);
                i += 1;
            }
            if !where_tokens.is_empty() {
                filter = Some(parse_sql_where(&where_tokens.join(" "))?);
            }
        } else if token_upper == "JOIN" || token_upper == "INNER" || token_upper == "LEFT" {
            let join_type = if token_upper == "LEFT" {
                i += 1;
                if i < tokens.len() && tokens[i].eq_ignore_ascii_case("OUTER") {
                    i += 1;
                }
                if i < tokens.len() && tokens[i].eq_ignore_ascii_case("JOIN") {
                    i += 1;
                }
                crate::ast::JoinType::Left
            } else {
                if token_upper == "INNER" {
                    i += 1;
                }
                if i < tokens.len() && tokens[i].eq_ignore_ascii_case("JOIN") {
                    i += 1;
                }
                crate::ast::JoinType::Inner
            };

            let joined_table = if i < tokens.len() {
                let name = tokens[i]
                    .trim_matches(|c| c == '\'' || c == '"' || c == ';')
                    .to_string();
                i += 1;
                name
            } else {
                return Err("Expected table name after JOIN".to_string());
            };

            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("ON") {
                i += 1;
            } else {
                return Err(format!("Expected ON clause after JOIN {joined_table}"));
            }

            let on_left = if i < tokens.len() {
                let col = tokens[i].to_string();
                i += 1;
                col
            } else {
                return Err("Expected left column in JOIN ON clause".to_string());
            };

            if i < tokens.len() && tokens[i] == "=" {
                i += 1;
            } else {
                return Err("Expected '=' in JOIN ON clause".to_string());
            }

            let on_right = if i < tokens.len() {
                let col = tokens[i].trim_matches(';').to_string();
                i += 1;
                col
            } else {
                return Err("Expected right column in JOIN ON clause".to_string());
            };

            joins.push(crate::ast::JoinClause {
                join_type,
                collection: joined_table,
                on_left,
                on_right,
            });
        } else if token_upper == "LIMIT" {
            i += 1;
            if i < tokens.len() {
                limit = tokens[i].trim_matches(';').parse::<usize>().ok();
                i += 1;
            }
        } else if token_upper == "SKIP" || token_upper == "OFFSET" {
            i += 1;
            if i < tokens.len() {
                skip = tokens[i].trim_matches(';').parse::<usize>().ok();
                i += 1;
            }
        } else if token_upper == "ORDER" {
            // ORDER BY field [ASC|DESC]
            i += 1;
            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("BY") {
                i += 1;
            }
            if i < tokens.len() {
                let field = tokens[i]
                    .trim_matches(|c| c == ';' || c == ',' || c == '"' || c == '\'')
                    .to_string();
                i += 1;
                let mut dir = 1i8;
                if i < tokens.len() {
                    let next_up = tokens[i].to_uppercase();
                    if next_up.starts_with("DESC") {
                        dir = -1;
                        i += 1;
                    } else if next_up.starts_with("ASC") {
                        dir = 1;
                        i += 1;
                    }
                }
                sort_by = Some((field, dir));
            }
        } else if token_upper == "VECTOR" {
            // VECTOR NEAR [0.1, 0.2] TOP 10 [USING INDEX index_name]
            i += 1;
            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("NEAR") {
                i += 1;
            }
            // parse array string
            let mut vec_str = String::new();
            while i < tokens.len()
                && !tokens[i].eq_ignore_ascii_case("TOP")
                && !tokens[i].eq_ignore_ascii_case("LIMIT")
                && !tokens[i].eq_ignore_ascii_case("USING")
                && !tokens[i].eq_ignore_ascii_case("TRAVERSE")
            {
                vec_str.push_str(tokens[i]);
                vec_str.push(' ');
                i += 1;
            }
            let top_k = if i < tokens.len() && tokens[i].eq_ignore_ascii_case("TOP") {
                i += 1;
                let k = tokens
                    .get(i)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(10);
                i += 1;
                k
            } else {
                10
            };

            let mut index_name = None;
            if i + 2 < tokens.len()
                && tokens[i].eq_ignore_ascii_case("USING")
                && tokens[i + 1].eq_ignore_ascii_case("INDEX")
            {
                index_name = Some(
                    tokens[i + 2]
                        .trim_matches(|c| c == '\'' || c == '"' || c == ';')
                        .to_string(),
                );
                i += 3;
            }

            let vec_parsed: Result<Vec<f32>, _> = serde_json::from_str(vec_str.trim());
            if let Ok(v) = vec_parsed {
                vector_search = Some(VectorSearchClause {
                    vector: v,
                    top_k,
                    index_name,
                });
            }
        } else if token_upper == "TRAVERSE" {
            // Sintaks: TRAVERSE FROM "start_id" DEPTH <n> [VIA "relation_type"]
            i += 1;
            let mut start_id = String::new();
            let mut max_depth = 1usize;
            let mut relation = None;

            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("FROM") {
                i += 1;
                if i < tokens.len() {
                    start_id = tokens[i]
                        .trim_matches(|c| c == '\'' || c == '"' || c == ';')
                        .to_string();
                    i += 1;
                }
            }

            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("DEPTH") {
                i += 1;
                if i < tokens.len() {
                    max_depth = tokens[i].trim_matches(';').parse::<usize>().unwrap_or(1);
                    i += 1;
                }
            }

            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("VIA") {
                i += 1;
                if i < tokens.len() {
                    relation = Some(
                        tokens[i]
                            .trim_matches(|c| c == '\'' || c == '"' || c == ';')
                            .to_string(),
                    );
                    i += 1;
                }
            }

            if !start_id.is_empty() {
                traverse = Some(TraverseClause {
                    start_id,
                    max_depth,
                    relation,
                });
            }
        } else {
            i += 1;
        }
    }

    Ok(Statement::Find {
        collection,
        filter,
        sort_by,
        limit,
        skip,
        vector_search,
        traverse,
        joins,
    })
}

/// Simple SQL WHERE parser: `field = 'val'` or `age > 25 AND city = 'KL'`
fn parse_sql_where(where_str: &str) -> Result<FilterExpr, String> {
    let and_parts: Vec<&str> = where_str.split(" AND ").collect();
    let mut exprs = Vec::new();

    for part in and_parts {
        let part = part.trim();
        if let Some((field, val_str)) = part.split_once(">=") {
            exprs.push(FilterExpr::Field {
                field: field.trim().to_string(),
                op: Operator::Gte,
                value: parse_literal(val_str.trim()),
            });
        } else if let Some((field, val_str)) = part.split_once("<=") {
            exprs.push(FilterExpr::Field {
                field: field.trim().to_string(),
                op: Operator::Lte,
                value: parse_literal(val_str.trim()),
            });
        } else if let Some((field, val_str)) = part.split_once("!=") {
            exprs.push(FilterExpr::Field {
                field: field.trim().to_string(),
                op: Operator::Neq,
                value: parse_literal(val_str.trim()),
            });
        } else if let Some((field, val_str)) = part.split_once('>') {
            exprs.push(FilterExpr::Field {
                field: field.trim().to_string(),
                op: Operator::Gt,
                value: parse_literal(val_str.trim()),
            });
        } else if let Some((field, val_str)) = part.split_once('<') {
            exprs.push(FilterExpr::Field {
                field: field.trim().to_string(),
                op: Operator::Lt,
                value: parse_literal(val_str.trim()),
            });
        } else if let Some((field, val_str)) = part.split_once('=') {
            exprs.push(FilterExpr::Field {
                field: field.trim().to_string(),
                op: Operator::Eq,
                value: parse_literal(val_str.trim()),
            });
        }
    }

    if exprs.is_empty() {
        Ok(FilterExpr::AlwaysTrue)
    } else if exprs.len() == 1 {
        Ok(exprs.pop().unwrap())
    } else {
        Ok(FilterExpr::And(exprs))
    }
}

fn parse_literal(s: &str) -> Value {
    let s = s.trim_matches(';').trim();
    if (s.starts_with('\'') && s.ends_with('\'')) || (s.starts_with('"') && s.ends_with('"')) {
        return Value::String(s[1..s.len() - 1].to_string());
    }
    if let Ok(i) = s.parse::<i64>() {
        return Value::Integer(i);
    }
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }
    if s.eq_ignore_ascii_case("true") {
        return Value::Boolean(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Value::Boolean(false);
    }
    Value::String(s.to_string())
}

/// Parse INSERT INTO <table> [(col1, col2)] VALUES (val1, val2) or INSERT INTO <table> {"key": "value"}
fn parse_insert_query(input: &str) -> Result<Statement, String> {
    let clean = input.trim_end_matches(';').trim();
    let upper = clean.to_uppercase();

    // 1. JSON body format: INSERT INTO <table> {"key": "value"}
    if let Some(json_start) = clean.find('{') {
        let prefix = clean[..json_start].trim();
        let tokens: Vec<&str> = prefix.split_whitespace().collect();
        if tokens.len() < 3 || !tokens[1].eq_ignore_ascii_case("INTO") {
            return Err("Expected 'INSERT INTO <collection> ...'".to_string());
        }
        let collection = tokens[2].to_string();
        let json_str = clean[json_start..].trim();
        let val: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON insert: {e}"))?;
        let doc = Document::from_json_value(val).ok_or("Expected JSON object")?;
        return Ok(Statement::Insert {
            collection,
            documents: vec![doc],
        });
    }

    // 2. Standard SQL format: INSERT INTO <table> [(cols)] VALUES (vals)
    if let Some(values_pos) = upper.find("VALUES") {
        let before_values = clean[..values_pos].trim();
        let after_values = clean[values_pos + 6..].trim();

        let tokens: Vec<&str> = before_values.split_whitespace().collect();
        if tokens.len() < 3 || !tokens[1].eq_ignore_ascii_case("INTO") {
            return Err("Expected 'INSERT INTO <collection> ...'".to_string());
        }

        let collection_token = tokens[2];
        let collection = if let Some(p) = collection_token.find('(') {
            collection_token[..p].to_string()
        } else {
            collection_token.to_string()
        };

        // Extract column list if provided
        let cols: Vec<String> = if let Some(paren_start) = before_values.find('(') {
            if let Some(paren_end) = before_values.rfind(')') {
                before_values[paren_start + 1..paren_end]
                    .split(',')
                    .map(|s| {
                        s.trim()
                            .trim_matches('"')
                            .trim_matches('\'')
                            .trim()
                            .to_string()
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let val_paren_start = after_values.find('(').ok_or("Expected '(' after VALUES")?;
        let val_paren_end = after_values.rfind(')').ok_or("Expected ')' after VALUES")?;
        let val_strs: Vec<&str> = after_values[val_paren_start + 1..val_paren_end]
            .split(',')
            .map(|s| s.trim())
            .collect();

        let mut doc = Document::new();
        if !cols.is_empty() {
            for (idx, col) in cols.iter().enumerate() {
                if let Some(val_str) = val_strs.get(idx) {
                    doc.set(col.as_str(), parse_literal(val_str));
                }
            }
        } else {
            for (idx, val_str) in val_strs.iter().enumerate() {
                doc.set(format!("col_{}", idx + 1), parse_literal(val_str));
            }
        }

        return Ok(Statement::Insert {
            collection,
            documents: vec![doc],
        });
    }

    Err(
        "INSERT requires JSON body or VALUES clause: INSERT INTO <table> (cols) VALUES (vals)"
            .to_string(),
    )
}

/// Parse UPDATE <table> SET col1 = val1, col2 = val2 [WHERE ...]
fn parse_update_query(input: &str) -> Result<Statement, String> {
    let clean = input.trim_end_matches(';').trim();
    let upper = clean.to_uppercase();

    let tokens: Vec<&str> = clean.split_whitespace().collect();
    if tokens.len() < 4 || !tokens[0].eq_ignore_ascii_case("UPDATE") {
        return Err("Expected 'UPDATE <collection> SET ...'".to_string());
    }

    let collection = tokens[1].trim_matches(';').to_string();

    let set_pos = upper
        .find("SET")
        .ok_or("Expected 'SET' clause in UPDATE statement")?;
    let after_set = clean[set_pos + 3..].trim();

    let (set_part, where_part) = if let Some(where_pos) = after_set.to_uppercase().find("WHERE") {
        let set_str = after_set[..where_pos].trim();
        let where_str = after_set[where_pos + 5..].trim();
        (set_str, Some(where_str))
    } else {
        (after_set, None)
    };

    let mut updates = Vec::new();
    for pair_str in set_part.split(',') {
        let pair_str = pair_str.trim();
        if pair_str.is_empty() {
            continue;
        }
        let eq_pos = pair_str
            .find('=')
            .ok_or_else(|| format!("Expected 'field = value' in SET clause, got '{pair_str}'"))?;
        let field = pair_str[..eq_pos]
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        let val_str = pair_str[eq_pos + 1..].trim();
        let val = parse_literal(val_str);
        updates.push((field, val));
    }

    if updates.is_empty() {
        return Err(
            "UPDATE statement requires at least one field assignment in SET clause".to_string(),
        );
    }

    let filter = if let Some(w) = where_part {
        parse_sql_where(w)?
    } else {
        FilterExpr::AlwaysTrue
    };

    Ok(Statement::Update {
        collection,
        filter,
        updates,
    })
}

/// Parse DELETE FROM <table> WHERE ...
fn parse_delete_query(input: &str) -> Result<Statement, String> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 3 || !tokens[1].eq_ignore_ascii_case("FROM") {
        return Err("Expected 'DELETE FROM <collection> WHERE ...'".to_string());
    }

    let collection = tokens[2].to_string();
    let where_pos = input
        .to_uppercase()
        .find("WHERE")
        .ok_or("DELETE requires WHERE clause")?;
    let where_str = &input[where_pos + 5..].trim_end_matches(';').trim();
    let filter = parse_sql_where(where_str)?;

    Ok(Statement::Delete { collection, filter })
}

/// Parse COUNT FROM <table> [WHERE ...]
fn parse_count_query(input: &str) -> Result<Statement, String> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 3 || !tokens[1].eq_ignore_ascii_case("FROM") {
        return Err("Expected 'COUNT FROM <collection>'".to_string());
    }

    let collection = tokens[2].trim_matches(';').to_string();
    let filter = if let Some(where_pos) = input.to_uppercase().find("WHERE") {
        let where_str = &input[where_pos + 5..].trim_end_matches(';').trim();
        Some(parse_sql_where(where_str)?)
    } else {
        None
    };

    Ok(Statement::Count { collection, filter })
}

/// Parse CREATE [UNIQUE] INDEX [idx_name] ON <collection>(<field>) [UNIQUE]
fn parse_create_index_query(input: &str) -> Result<Statement, String> {
    let clean = input.trim_end_matches(';').trim();
    let upper = clean.to_uppercase();
    let unique = upper.contains("UNIQUE");

    let on_pos = upper
        .find("ON")
        .ok_or("Expected 'ON <collection>(<field>)' in CREATE INDEX")?;
    let target = clean[on_pos + 2..].trim();

    let paren_open = target
        .find('(')
        .ok_or("Expected '(' around indexed field name")?;
    let paren_close = target
        .find(')')
        .ok_or("Expected ')' around indexed field name")?;

    let collection = target[..paren_open].trim().to_string();
    let field = target[paren_open + 1..paren_close].trim().to_string();

    if collection.is_empty() || field.is_empty() {
        return Err("Invalid collection or field in CREATE INDEX".to_string());
    }

    Ok(Statement::CreateIndex {
        collection,
        field,
        unique,
    })
}

/// Parse DROP INDEX [idx_name] ON <collection> or DROP INDEX <field> ON <collection>
fn parse_drop_index_query(input: &str) -> Result<Statement, String> {
    let clean = input.trim_end_matches(';').trim();
    let upper = clean.to_uppercase();
    let on_pos = upper
        .find("ON")
        .ok_or("Expected 'ON <collection>' in DROP INDEX")?;

    let index_part = clean[..on_pos].trim();
    let field = index_part
        .split_whitespace()
        .last()
        .unwrap_or("")
        .trim_start_matches("idx_")
        .to_string();
    let collection = clean[on_pos + 2..].trim().to_string();

    Ok(Statement::DropIndex { collection, field })
}

// =========================================================================
// openCypher Graph Syntax Parser (MATCH / CREATE)
// =========================================================================

/// Tokenize openCypher input preserving parenthesis, bracket, and brace groupings
fn tokenize_cypher_top_level(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut p_depth = 0usize;
    let mut b_depth = 0usize;
    let mut br_depth = 0usize;

    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' && !in_double {
            in_single = !in_single;
            cur.push(c);
        } else if c == '"' && !in_single {
            in_double = !in_double;
            cur.push(c);
        } else if !in_single && !in_double {
            match c {
                '(' => {
                    p_depth += 1;
                    cur.push(c);
                }
                ')' => {
                    p_depth = p_depth.saturating_sub(1);
                    cur.push(c);
                }
                '[' => {
                    b_depth += 1;
                    cur.push(c);
                }
                ']' => {
                    b_depth = b_depth.saturating_sub(1);
                    cur.push(c);
                }
                '{' => {
                    br_depth += 1;
                    cur.push(c);
                }
                '}' => {
                    br_depth = br_depth.saturating_sub(1);
                    cur.push(c);
                }
                ' ' | '\t' | '\r' | '\n' if p_depth == 0 && b_depth == 0 && br_depth == 0 => {
                    if !cur.is_empty() {
                        tokens.push(cur.clone());
                        cur.clear();
                    }
                }
                _ => cur.push(c),
            }
        } else {
            cur.push(c);
        }
        i += 1;
    }
    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

/// Parsed node information
struct CypherNode {
    var: String,
    label: Option<String>,
    id: Option<String>,
    props_filters: Vec<FilterExpr>,
    doc: Document,
}

/// Parse properties map inside `{...}` into key-values, id, filters, and Document
fn parse_cypher_props(props_str: &str) -> (Option<String>, Vec<FilterExpr>, Document) {
    let mut doc = Document::new();
    let mut node_id = None;
    let mut filters = Vec::new();

    let trimmed = props_str
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    if trimmed.is_empty() {
        return (node_id, filters, doc);
    }

    let mut pairs = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;

    for c in trimmed.chars() {
        if c == '\'' && !in_double {
            in_single = !in_single;
            cur.push(c);
        } else if c == '"' && !in_single {
            in_double = !in_double;
            cur.push(c);
        } else if c == ',' && !in_single && !in_double {
            if !cur.trim().is_empty() {
                pairs.push(cur.trim().to_string());
                cur.clear();
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.trim().is_empty() {
        pairs.push(cur.trim().to_string());
    }

    for pair in pairs {
        if let Some((k, v)) = pair.split_once(':') {
            let key = k.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
            let val_str = v.trim();
            let value = parse_literal(val_str);

            if key == "id" || key == "_id" {
                if let Value::String(ref s) = value {
                    node_id = Some(s.clone());
                } else if let Value::Integer(i) = value {
                    node_id = Some(i.to_string());
                }
            }

            filters.push(FilterExpr::Field {
                field: key.clone(),
                op: Operator::Eq,
                value: value.clone(),
            });

            doc.set(key, value);
        }
    }

    if let Some(ref id) = node_id {
        doc.set("_id", id.clone());
        doc.set("id", id.clone());
    }

    (node_id, filters, doc)
}

/// Parse a single Cypher node e.g. `(n:Person {id: 'p1'})`
fn parse_cypher_node(node_str: &str) -> Result<CypherNode, String> {
    let trimmed = node_str
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();

    let (var_and_label, props_str) = if let Some(pos) = trimmed.find('{') {
        let (vl, rest) = trimmed.split_at(pos);
        (vl.trim(), Some(rest.trim()))
    } else {
        (trimmed, None)
    };

    let (id, props_filters, doc) = if let Some(p) = props_str {
        parse_cypher_props(p)
    } else {
        (None, Vec::new(), Document::new())
    };

    let (var, label) = if var_and_label.contains(':') {
        let (v, l) = var_and_label.split_once(':').unwrap();
        let var_name = v.trim().to_string();
        let label_name = l.trim().to_string();
        (
            var_name,
            if label_name.is_empty() {
                None
            } else {
                Some(label_name)
            },
        )
    } else {
        (var_and_label.to_string(), None)
    };

    Ok(CypherNode {
        var,
        label,
        id,
        props_filters,
        doc,
    })
}

/// Parse an edge string e.g. `-[:KNOWS]->` or `-[r:KNOWS*2]->` or `-[r:KNOWS {weight: 1.5}]->`
fn parse_cypher_edge(edge_str: &str) -> Result<(Option<String>, usize, Option<f32>, bool), String> {
    let trimmed = edge_str.trim();
    let is_incoming = trimmed.starts_with("<-");

    let mut relation = None;
    let mut max_depth = 1usize;
    let mut weight = None;

    if let (Some(open), Some(close)) = (trimmed.find('['), trimmed.rfind(']')) {
        let mut inside = trimmed[open + 1..close].trim().to_string();

        // Check for inline properties e.g. {weight: 1.5}
        if let (Some(p_open), Some(p_close)) = (inside.find('{'), inside.rfind('}')) {
            let props_part = &inside[p_open..=p_close];
            let (_, _, doc) = parse_cypher_props(props_part);
            if let Some(w) = doc.get("weight").and_then(|v| v.as_f64()) {
                weight = Some(w as f32);
            }
            inside = format!("{}{}", &inside[..p_open], &inside[p_close + 1..]).trim().to_string();
        }

        // Check for variable depth traversal e.g. *2 or *1..3
        let (rel_part, depth_part) = match inside.split_once('*') {
            Some((r, d)) => (r.trim(), Some(d.trim())),
            None => (inside.as_str(), None),
        };

        if let Some(d_str) = depth_part {
            if let Some((_, max_str)) = d_str.split_once("..") {
                if let Ok(d) = max_str.trim().parse::<usize>() {
                    max_depth = d;
                }
            } else if let Ok(d) = d_str.parse::<usize>() {
                max_depth = d;
            }
        }

        if rel_part.contains(':') {
            let rel = rel_part.split_once(':').unwrap().1.trim();
            if !rel.is_empty() {
                relation = Some(rel.to_string());
            }
        }
    }

    Ok((relation, max_depth, weight, is_incoming))
}

/// Parse openCypher MATCH queries into Statement::Find, Statement::Count, or Statement::Delete
fn parse_cypher_match(input: &str) -> Result<Statement, String> {
    let clean = input.trim_end_matches(';').trim();
    let tokens = tokenize_cypher_top_level(clean);
    if tokens.is_empty() || !tokens[0].eq_ignore_ascii_case("MATCH") {
        return Err("Expected query starting with MATCH".to_string());
    }

    #[derive(Debug, PartialEq, Eq)]
    enum CypherClause {
        Pattern,
        Where,
        Vector,
        OrderBy,
        Skip,
        Limit,
        Return,
        Delete,
    }

    let mut cur_clause = CypherClause::Pattern;
    let mut pattern_tokens = Vec::new();
    let mut where_tokens = Vec::new();
    let mut vector_tokens = Vec::new();
    let mut order_tokens = Vec::new();
    let mut skip_token = None;
    let mut limit_token = None;
    let mut return_tokens = Vec::new();
    let mut delete_tokens = Vec::new();

    let mut i = 1; // skip MATCH
    while i < tokens.len() {
        let tok = &tokens[i];
        let tok_upper = tok.to_uppercase();

        if tok_upper == "WHERE" {
            cur_clause = CypherClause::Where;
            i += 1;
            continue;
        } else if tok_upper == "VECTOR" {
            cur_clause = CypherClause::Vector;
            i += 1;
            continue;
        } else if tok_upper == "ORDER" {
            cur_clause = CypherClause::OrderBy;
            i += 1;
            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("BY") {
                i += 1;
            }
            continue;
        } else if tok_upper == "SKIP" || tok_upper == "OFFSET" {
            cur_clause = CypherClause::Skip;
            i += 1;
            continue;
        } else if tok_upper == "LIMIT" {
            cur_clause = CypherClause::Limit;
            i += 1;
            continue;
        } else if tok_upper == "RETURN" {
            cur_clause = CypherClause::Return;
            i += 1;
            continue;
        } else if tok_upper == "DELETE" {
            cur_clause = CypherClause::Delete;
            i += 1;
            continue;
        } else if tok_upper == "DETACH" {
            i += 1;
            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("DELETE") {
                cur_clause = CypherClause::Delete;
                i += 1;
            }
            continue;
        }

        match cur_clause {
            CypherClause::Pattern => pattern_tokens.push(tok.clone()),
            CypherClause::Where => where_tokens.push(tok.clone()),
            CypherClause::Vector => vector_tokens.push(tok.clone()),
            CypherClause::OrderBy => order_tokens.push(tok.clone()),
            CypherClause::Skip => {
                if skip_token.is_none() {
                    skip_token = Some(tok.clone());
                }
            }
            CypherClause::Limit => {
                if limit_token.is_none() {
                    limit_token = Some(tok.clone());
                }
            }
            CypherClause::Return => return_tokens.push(tok.clone()),
            CypherClause::Delete => delete_tokens.push(tok.clone()),
        }
        i += 1;
    }

    let pattern_str = pattern_tokens.join(" ");
    if pattern_str.is_empty() {
        return Err("MATCH statement requires a graph pattern".to_string());
    }

    // Find nodes in pattern_str
    let mut node_spans = Vec::new();
    let chars: Vec<char> = pattern_str.chars().collect();
    let mut p_start = None;
    let mut p_depth = 0usize;
    for (idx, &c) in chars.iter().enumerate() {
        if c == '(' {
            if p_depth == 0 {
                p_start = Some(idx);
            }
            p_depth += 1;
        } else if c == ')' {
            p_depth = p_depth.saturating_sub(1);
            if p_depth == 0 {
                if let Some(s) = p_start {
                    node_spans.push((s, idx));
                    p_start = None;
                }
            }
        }
    }

    if node_spans.is_empty() {
        return Err("No node pattern found in MATCH clause (expected (var:Label))".to_string());
    }

    let mut traverse = None;
    let mut inline_filters = Vec::new();
    let mut start_id_candidate = None;

    let (target_var, collection) = if node_spans.len() == 1 {
        // Single node query: MATCH (n:Person ...)
        let node_str = &pattern_str[node_spans[0].0..=node_spans[0].1];
        let node = parse_cypher_node(node_str)?;
        let t_var = node.var.clone();
        let col = node
            .label
            .ok_or_else(|| "Single node MATCH requires a collection label, e.g. (n:Person)".to_string())?;
        inline_filters.extend(node.props_filters);
        (t_var, col)
    } else {
        // Two-node relationship traversal: MATCH (a:Person)-[:KNOWS]->(b:Person)
        let (s1, e1) = node_spans[0];
        let (s2, e2) = node_spans[1];
        let node1 = parse_cypher_node(&pattern_str[s1..=e1])?;
        let node2 = parse_cypher_node(&pattern_str[s2..=e2])?;
        let edge_str = &pattern_str[e1 + 1..s2];
        let (relation, max_depth, _weight, is_incoming) = parse_cypher_edge(edge_str)?;

        let return_text = return_tokens.join(" ");
        let delete_text = delete_tokens.join(" ");

        // Determine target node vs source node
        let (source_node, target_node) = if is_incoming {
            if !node1.var.is_empty() && (return_text.contains(&node1.var) || delete_text.contains(&node1.var)) {
                (node2, node1)
            } else {
                (node1, node2)
            }
        } else {
            if !node1.var.is_empty() && (return_text.contains(&node1.var) || delete_text.contains(&node1.var)) {
                (node2, node1)
            } else {
                (node1, node2)
            }
        };

        let t_var = target_node.var.clone();
        let col = target_node
            .label
            .clone()
            .or_else(|| source_node.label.clone())
            .unwrap_or_else(|| "default".to_string());

        inline_filters.extend(target_node.props_filters);

        // Check if source node has start ID inline
        if let Some(ref s_id) = source_node.id {
            start_id_candidate = Some(s_id.clone());
        }

        // We will finalize `traverse` after checking WHERE for source_node.id
        traverse = Some((source_node.var.clone(), max_depth, relation));

        (t_var, col)
    };


    // Process WHERE conditions
    let mut where_filters = Vec::new();
    let where_raw = where_tokens.join(" ");
    if !where_raw.is_empty() {
        let and_parts: Vec<&str> = where_raw.split(" AND ").collect();
        for part in and_parts {
            let p_trim = part.trim();
            if p_trim.is_empty() {
                continue;
            }

            // Check if condition specifies start node ID: e.g. a.id = 'p1' or a._id = 'p1' or id(a) = 'p1'
            let mut is_start_id = false;
            if let Some((ref src_var, _, _)) = traverse {
                let id_pattern_1 = format!("{src_var}.id");
                let id_pattern_2 = format!("{src_var}._id");
                let id_pattern_3 = format!("id({src_var})");

                if p_trim.contains(&id_pattern_1)
                    || p_trim.contains(&id_pattern_2)
                    || p_trim.contains(&id_pattern_3)
                {
                    if let Some((_, val_str)) = p_trim.split_once('=') {
                        let parsed_val = parse_literal(val_str.trim());
                        let id_str = match parsed_val {
                            Value::String(s) => s,
                            Value::Integer(i) => i.to_string(),
                            other => other.to_string(),
                        };
                        start_id_candidate = Some(id_str);
                        is_start_id = true;
                    }
                }
            }

            if !is_start_id {
                // Strip target variable prefix (e.g. `b.age >= 18` -> `age >= 18` or `n.status = 'active'`)
                let cleaned_cond = if !target_var.is_empty() {
                    let prefix = format!("{target_var}.");
                    p_trim.replace(&prefix, "")
                } else {
                    p_trim.to_string()
                };

                let parsed_filter = parse_sql_where(&cleaned_cond)?;
                where_filters.push(parsed_filter);
            }
        }
    }

    // Finalize TraverseClause
    let final_traverse = if let Some((_, max_depth, relation)) = traverse {
        if let Some(start_id) = start_id_candidate {
            Some(TraverseClause {
                start_id,
                max_depth,
                relation,
            })
        } else {
            None
        }
    } else {
        None
    };

    // Combine all filters
    let mut all_filters = inline_filters;
    all_filters.extend(where_filters);

    let filter = if all_filters.is_empty() {
        None
    } else if all_filters.len() == 1 {
        Some(all_filters.pop().unwrap())
    } else {
        Some(FilterExpr::And(all_filters))
    };

    // Vector Search
    let mut vector_search = None;
    if !vector_tokens.is_empty() {
        let mut v_i = 0;
        if v_i < vector_tokens.len() && vector_tokens[v_i].eq_ignore_ascii_case("NEAR") {
            v_i += 1;
        }
        let mut vec_str = String::new();
        while v_i < vector_tokens.len()
            && !vector_tokens[v_i].eq_ignore_ascii_case("TOP")
            && !vector_tokens[v_i].eq_ignore_ascii_case("USING")
        {
            vec_str.push_str(&vector_tokens[v_i]);
            vec_str.push(' ');
            v_i += 1;
        }

        let top_k = if v_i < vector_tokens.len() && vector_tokens[v_i].eq_ignore_ascii_case("TOP") {
            v_i += 1;
            let k = vector_tokens
                .get(v_i)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(10);
            v_i += 1;
            k
        } else {
            10
        };

        let mut index_name = None;
        if v_i + 2 < vector_tokens.len()
            && vector_tokens[v_i].eq_ignore_ascii_case("USING")
            && vector_tokens[v_i + 1].eq_ignore_ascii_case("INDEX")
        {
            index_name = Some(
                vector_tokens[v_i + 2]
                    .trim_matches(|c| c == '\'' || c == '"' || c == ';')
                    .to_string(),
            );
        }

        if let Ok(vec) = serde_json::from_str::<Vec<f32>>(vec_str.trim()) {
            vector_search = Some(VectorSearchClause {
                vector: vec,
                top_k,
                index_name,
            });
        }
    }

    // ORDER BY
    let mut sort_by = None;
    if !order_tokens.is_empty() {
        let mut field = order_tokens[0]
            .trim_matches(|c| c == ';' || c == ',' || c == '"' || c == '\'')
            .to_string();
        if !target_var.is_empty() {
            let prefix = format!("{target_var}.");
            if field.starts_with(&prefix) {
                field = field[prefix.len()..].to_string();
            }
        }
        let mut dir = 1i8;
        if order_tokens.len() > 1 {
            let dir_str = order_tokens[1].to_uppercase();
            if dir_str.starts_with("DESC") {
                dir = -1;
            }
        }
        sort_by = Some((field, dir));
    }

    let skip = skip_token.and_then(|s| s.trim_matches(';').parse::<usize>().ok());
    let limit = limit_token.and_then(|s| s.trim_matches(';').parse::<usize>().ok());

    // Check if DELETE
    if !delete_tokens.is_empty() {
        return Ok(Statement::Delete {
            collection,
            filter: filter.unwrap_or(FilterExpr::AlwaysTrue),
        });
    }

    // Check if RETURN count(...)
    let return_str = return_tokens.join(" ").to_lowercase();
    if return_str.contains("count(") {
        return Ok(Statement::Count { collection, filter });
    }

    Ok(Statement::Find {
        collection,
        filter,
        sort_by,
        limit,
        skip,
        vector_search,
        traverse: final_traverse,
        joins: Vec::new(),
    })
}

/// Parse openCypher CREATE statements (node creation or edge creation)
fn parse_cypher_create(input: &str) -> Result<Statement, String> {
    let clean = input.trim_end_matches(';').trim();
    let without_create = clean
        .strip_prefix("CREATE")
        .or_else(|| clean.strip_prefix("create"))
        .unwrap_or(clean)
        .trim();

    let tokens = tokenize_cypher_top_level(without_create);
    let pattern_str = tokens.join(" ");

    // Find node spans
    let mut node_spans = Vec::new();
    let chars: Vec<char> = pattern_str.chars().collect();
    let mut p_start = None;
    let mut p_depth = 0usize;
    for (idx, &c) in chars.iter().enumerate() {
        if c == '(' {
            if p_depth == 0 {
                p_start = Some(idx);
            }
            p_depth += 1;
        } else if c == ')' {
            p_depth = p_depth.saturating_sub(1);
            if p_depth == 0 {
                if let Some(s) = p_start {
                    node_spans.push((s, idx));
                    p_start = None;
                }
            }
        }
    }

    if node_spans.len() == 1 {
        // Node creation: CREATE (n:Person {id: 'p1', name: 'Alice'})
        let node_str = &pattern_str[node_spans[0].0..=node_spans[0].1];
        let node = parse_cypher_node(node_str)?;
        let collection = node
            .label
            .ok_or_else(|| "CREATE node requires a collection label, e.g. (:Person {id: '...'})".to_string())?;

        return Ok(Statement::Insert {
            collection,
            documents: vec![node.doc],
        });
    }

    if node_spans.len() >= 2 {
        // Edge creation: CREATE (a {id: 'p1'})-[:KNOWS {weight: 1.5}]->(b {id: 'p2'})
        let (s1, e1) = node_spans[0];
        let (s2, e2) = node_spans[1];
        let node1 = parse_cypher_node(&pattern_str[s1..=e1])?;
        let node2 = parse_cypher_node(&pattern_str[s2..=e2])?;
        let edge_str = &pattern_str[e1 + 1..s2];
        let (relation, _, weight, is_incoming) = parse_cypher_edge(edge_str)?;

        let from = if is_incoming {
            node2.id.clone().or_else(|| Some(node2.var.clone())).unwrap_or_default()
        } else {
            node1.id.clone().or_else(|| Some(node1.var.clone())).unwrap_or_default()
        };

        let to = if is_incoming {
            node1.id.clone().or_else(|| Some(node1.var.clone())).unwrap_or_default()
        } else {
            node2.id.clone().or_else(|| Some(node2.var.clone())).unwrap_or_default()
        };


        let rel_name = relation.unwrap_or_else(|| "RELATED".to_string());

        return Ok(Statement::CreateEdge {
            from,
            to,
            relation: rel_name,
            weight,
            properties: None,
        });
    }

    Err(format!("Malformed Cypher CREATE query: '{input}'"))
}

#[cfg(test)]

mod tests {
    use super::*;

    #[test]
    fn test_parse_mongo_find() {
        let stmt = parse_query(r#"db.users.find({"city": "KL", "age": {"$gt": 25}})"#).unwrap();
        match stmt {
            Statement::Find {
                collection, filter, ..
            } => {
                assert_eq!(collection, "users");
                assert!(filter.is_some());
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_sql_select() {
        let stmt =
            parse_query("SELECT * FROM users WHERE age >= 21 AND city = 'KL' LIMIT 10").unwrap();
        match stmt {
            Statement::Find {
                collection,
                limit,
                filter,
                ..
            } => {
                assert_eq!(collection, "users");
                assert_eq!(limit, Some(10));
                assert!(filter.is_some());
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_mongo_insert() {
        let stmt =
            parse_query(r#"db.articles.insert({"title": "FaizDB AI", "views": 1000})"#).unwrap();
        match stmt {
            Statement::Insert {
                collection,
                documents,
            } => {
                assert_eq!(collection, "articles");
                assert_eq!(documents.len(), 1);
                assert_eq!(
                    documents[0].get("title").unwrap().as_str(),
                    Some("FaizDB AI")
                );
            }
            _ => panic!("Expected Statement::Insert"),
        }
    }

    #[test]
    fn test_parse_faizql_traverse() {
        let stmt = parse_query("FIND prod TRAVERSE FROM 'p1' DEPTH 2").unwrap();
        match stmt {
            Statement::Find {
                collection,
                traverse,
                filter,
                ..
            } => {
                assert_eq!(collection, "prod");
                assert!(filter.is_none());
                let trav = traverse.expect("TRAVERSE clause should be parsed");
                assert_eq!(trav.start_id, "p1");
                assert_eq!(trav.max_depth, 2);
                assert_eq!(trav.relation, None);
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_faizql_where_and_traverse() {
        let stmt =
            parse_query("FIND prod WHERE cat = 'tech' TRAVERSE FROM 'p1' DEPTH 3 VIA 'related'")
                .unwrap();
        match stmt {
            Statement::Find {
                collection,
                filter,
                traverse,
                ..
            } => {
                assert_eq!(collection, "prod");
                assert!(
                    filter.is_some(),
                    "WHERE filter must not be eaten by TRAVERSE"
                );
                let trav = traverse.expect("TRAVERSE clause should be parsed");
                assert_eq!(trav.start_id, "p1");
                assert_eq!(trav.max_depth, 3);
                assert_eq!(trav.relation, Some("related".to_string()));
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_faizql_vector_using_index() {
        let stmt = parse_query(
            "FIND prod WHERE cat = 'tech' VECTOR NEAR [1.0, 0.5] TOP 3 USING INDEX 'custom_emb'",
        )
        .unwrap();
        match stmt {
            Statement::Find {
                collection,
                filter,
                vector_search,
                ..
            } => {
                assert_eq!(collection, "prod");
                assert!(filter.is_some());
                let vec_clause = vector_search.expect("VECTOR clause should be parsed");
                assert_eq!(vec_clause.top_k, 3);
                assert_eq!(vec_clause.index_name, Some("custom_emb".to_string()));
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_mongo_traverse() {
        let stmt = parse_query(r#"db.prod.find({"cat": "tech", "$traverse": {"from": "p1", "depth": 2, "via": "knows"}})"#).unwrap();
        match stmt {
            Statement::Find {
                collection,
                filter,
                traverse,
                ..
            } => {
                assert_eq!(collection, "prod");
                assert!(filter.is_some());
                let trav = traverse.expect("TRAVERSE clause should be parsed from Mongo syntax");
                assert_eq!(trav.start_id, "p1");
                assert_eq!(trav.max_depth, 2);
                assert_eq!(trav.relation, Some("knows".to_string()));
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_sql_update() {
        let stmt =
            parse_query("UPDATE users SET age = 30, city = 'Kuala Lumpur' WHERE name = 'Alice'")
                .unwrap();
        match stmt {
            Statement::Update {
                collection,
                filter,
                updates,
            } => {
                assert_eq!(collection, "users");
                assert_eq!(updates.len(), 2);
                assert_eq!(updates[0].0, "age");
                assert_eq!(updates[0].1, Value::Integer(30));
                assert_eq!(updates[1].0, "city");
                assert_eq!(updates[1].1, Value::String("Kuala Lumpur".to_string()));
                match filter {
                    FilterExpr::Field { field, value, .. } => {
                        assert_eq!(field, "name");
                        assert_eq!(value, Value::String("Alice".to_string()));
                    }
                    _ => panic!("Expected field filter"),
                }
            }
            _ => panic!("Expected Statement::Update"),
        }
    }

    #[test]
    fn test_parse_mongo_update() {
        let stmt =
            parse_query(r#"db.users.updateOne({"name": "Alice"}, {"$set": {"age": 31}})"#).unwrap();
        match stmt {
            Statement::Update {
                collection,
                updates,
                ..
            } => {
                assert_eq!(collection, "users");
                assert_eq!(updates.len(), 1);
                assert_eq!(updates[0].0, "age");
                assert_eq!(updates[0].1, Value::Integer(31));
            }
            _ => panic!("Expected Statement::Update"),
        }
    }

    #[test]
    fn test_parse_sql_order_by() {
        let stmt =
            parse_query("SELECT * FROM users WHERE age >= 21 ORDER BY age DESC LIMIT 10").unwrap();
        match stmt {
            Statement::Find {
                collection,
                sort_by,
                limit,
                ..
            } => {
                assert_eq!(collection, "users");
                assert_eq!(sort_by, Some(("age".to_string(), -1)));
                assert_eq!(limit, Some(10));
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_mongo_sort() {
        let stmt = parse_query(r#"db.users.find({"active": true}).sort({"score": 1})"#).unwrap();
        match stmt {
            Statement::Find {
                collection,
                sort_by,
                ..
            } => {
                assert_eq!(collection, "users");
                assert_eq!(sort_by, Some(("score".to_string(), 1)));
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_sql_joins() {
        let stmt = parse_query("SELECT * FROM orders JOIN customers ON orders.user_id = customers.id WHERE orders.amount > 100").unwrap();
        match stmt {
            Statement::Find {
                collection,
                filter,
                joins,
                ..
            } => {
                assert_eq!(collection, "orders");
                assert!(filter.is_some());
                assert_eq!(joins.len(), 1);
                assert_eq!(joins[0].join_type, crate::ast::JoinType::Inner);
                assert_eq!(joins[0].collection, "customers");
                assert_eq!(joins[0].on_left, "orders.user_id");
                assert_eq!(joins[0].on_right, "customers.id");
            }
            _ => panic!("Expected Statement::Find with JOIN"),
        }

        let left_stmt =
            parse_query("SELECT * FROM orders LEFT JOIN profiles ON orders.user_id = profiles.id")
                .unwrap();
        match left_stmt {
            Statement::Find { joins, .. } => {
                assert_eq!(joins.len(), 1);
                assert_eq!(joins[0].join_type, crate::ast::JoinType::Left);
                assert_eq!(joins[0].collection, "profiles");
            }
            _ => panic!("Expected Statement::Find with LEFT JOIN"),
        }
    }

    #[test]
    fn test_parse_cypher_single_node() {
        let stmt = parse_query("MATCH (n:Person) RETURN n").unwrap();
        match stmt {
            Statement::Find {
                collection,
                filter,
                traverse,
                ..
            } => {
                assert_eq!(collection, "Person");
                assert!(filter.is_none());
                assert!(traverse.is_none());
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_cypher_node_with_inline_properties() {
        let stmt = parse_query("MATCH (n:Person {id: 'p1', role: 'admin'}) RETURN n").unwrap();
        match stmt {
            Statement::Find {
                collection, filter, ..
            } => {
                assert_eq!(collection, "Person");
                assert!(filter.is_some());
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_cypher_node_where_filter() {
        let stmt =
            parse_query("MATCH (n:Person) WHERE n.age >= 18 AND n.city = 'KL' RETURN n").unwrap();
        match stmt {
            Statement::Find {
                collection, filter, ..
            } => {
                assert_eq!(collection, "Person");
                let f = filter.expect("Filter should be present");
                let doc = Document::from_json(r#"{"_id": "1", "age": 20, "city": "KL"}"#).unwrap();
                assert!(f.matches(&doc));
                let doc_young = Document::from_json(r#"{"_id": "2", "age": 16, "city": "KL"}"#).unwrap();
                assert!(!f.matches(&doc_young));

            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_cypher_traversal_simple() {
        let stmt =
            parse_query("MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.id = 'p1' RETURN b")
                .unwrap();
        match stmt {
            Statement::Find {
                collection,
                traverse,
                ..
            } => {
                assert_eq!(collection, "Person");
                let t = traverse.expect("Traversal clause expected");
                assert_eq!(t.start_id, "p1");
                assert_eq!(t.max_depth, 1);
                assert_eq!(t.relation, Some("KNOWS".to_string()));
            }
            _ => panic!("Expected Statement::Find with Traverse"),
        }
    }

    #[test]
    fn test_parse_cypher_traversal_with_depth() {
        let stmt =
            parse_query("MATCH (a:Person {id: 'p1'})-[:KNOWS*2]->(b:Person) RETURN b").unwrap();
        match stmt {
            Statement::Find {
                collection,
                traverse,
                ..
            } => {
                assert_eq!(collection, "Person");
                let t = traverse.expect("Traversal clause expected");
                assert_eq!(t.start_id, "p1");
                assert_eq!(t.max_depth, 2);
                assert_eq!(t.relation, Some("KNOWS".to_string()));
            }
            _ => panic!("Expected Statement::Find with Traverse"),
        }
    }

    #[test]
    fn test_parse_cypher_traversal_range_depth() {
        let stmt =
            parse_query("MATCH (a:Person {id: 'p1'})-[:KNOWS*1..3]->(b:Person) RETURN b").unwrap();
        match stmt {
            Statement::Find {
                collection,
                traverse,
                ..
            } => {
                assert_eq!(collection, "Person");
                let t = traverse.expect("Traversal clause expected");
                assert_eq!(t.start_id, "p1");
                assert_eq!(t.max_depth, 3);
            }
            _ => panic!("Expected Statement::Find with Traverse"),
        }
    }

    #[test]
    fn test_parse_cypher_graphrag_with_vector() {
        let stmt = parse_query(
            "MATCH (a:prod)-[:related]->(b:prod) WHERE a.id = 'p1' VECTOR NEAR [0.1, 0.2, 0.3] TOP 5 RETURN b LIMIT 10",
        )
        .unwrap();
        match stmt {
            Statement::Find {
                collection,
                traverse,
                vector_search,
                limit,
                ..
            } => {
                assert_eq!(collection, "prod");
                let t = traverse.expect("Traversal clause expected");
                assert_eq!(t.start_id, "p1");
                assert_eq!(t.relation, Some("related".to_string()));

                let v = vector_search.expect("Vector search expected");
                assert_eq!(v.vector, vec![0.1, 0.2, 0.3]);
                assert_eq!(v.top_k, 5);

                assert_eq!(limit, Some(10));
            }
            _ => panic!("Expected Statement::Find with GraphRAG + Vector"),
        }
    }

    #[test]
    fn test_parse_cypher_count_and_delete() {
        let count_stmt = parse_query("MATCH (n:Person) RETURN count(n)").unwrap();
        match count_stmt {
            Statement::Count { collection, .. } => {
                assert_eq!(collection, "Person");
            }
            _ => panic!("Expected Statement::Count"),
        }

        let del_stmt = parse_query("MATCH (n:Person {id: 'p1'}) DELETE n").unwrap();
        match del_stmt {
            Statement::Delete { collection, .. } => {
                assert_eq!(collection, "Person");
            }
            _ => panic!("Expected Statement::Delete"),
        }
    }

    #[test]
    fn test_parse_cypher_create_node_and_edge() {
        let node_stmt =
            parse_query("CREATE (n:Person {id: 'p1', name: 'Alice', age: 30})").unwrap();
        match node_stmt {
            Statement::Insert {
                collection,
                documents,
            } => {
                assert_eq!(collection, "Person");
                assert_eq!(documents.len(), 1);
                assert_eq!(documents[0].id.as_str(), "p1");
                assert_eq!(
                    documents[0].get("name").and_then(|v| v.as_str()),
                    Some("Alice")
                );
                assert_eq!(documents[0].get("age").and_then(|v| v.as_i64()), Some(30));
            }
            _ => panic!("Expected Statement::Insert"),
        }

        let edge_stmt =
            parse_query("CREATE (a {id: 'p1'})-[:KNOWS {weight: 1.5}]->(b {id: 'p2'})").unwrap();
        match edge_stmt {
            Statement::CreateEdge {
                from,
                to,
                relation,
                weight,
                ..
            } => {
                assert_eq!(from, "p1");
                assert_eq!(to, "p2");
                assert_eq!(relation, "KNOWS");
                assert_eq!(weight, Some(1.5));
            }
            _ => panic!("Expected Statement::CreateEdge"),
        }
    }
}

