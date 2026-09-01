//! Parser for SQL, MongoDB, and FaizQL dialect statements into AST Statements.

use faizdb_core::document::model::{Document, Value};
use crate::ast::{FilterExpr, Operator, Statement, VectorSearchClause};

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

    // 3. SQL format: SELECT / INSERT / DELETE / COUNT
    if upper.starts_with("SELECT") || upper.starts_with("FIND") {
        return parse_select_query(trimmed);
    }

    if upper.starts_with("INSERT") {
        return parse_insert_query(trimmed);
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
    let dot_pos = without_db.find('.').ok_or("Expected collection name after 'db.'")?;
    let collection = without_db[..dot_pos].to_string();

    let remainder = &without_db[dot_pos + 1..];
    let paren_open = remainder.find('(').ok_or("Expected '(' after method name")?;
    let paren_close = remainder.rfind(')').ok_or("Expected ')' at end of statement")?;

    let method = &remainder[..paren_open].trim();
    let args_str = &remainder[paren_open + 1..paren_close].trim();

    match *method {
        "find" => {
            let filter = if args_str.is_empty() {
                None
            } else {
                Some(parse_json_filter(args_str)?)
            };
            Ok(Statement::Find {
                collection,
                filter,
                sort_by: None,
                limit: None,
                skip: None,
                vector_search: None,
                traverse: None,
            })
        }
        "insert" | "insertOne" => {
            let doc_val: serde_json::Value =
                serde_json::from_str(args_str).map_err(|e| format!("Invalid JSON document: {e}"))?;
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
            let field = val.as_object().and_then(|o| o.keys().next().cloned()).unwrap_or_else(|| "id".to_string());
            let unique = args_str.to_lowercase().contains("unique") && args_str.to_lowercase().contains("true");
            Ok(Statement::CreateIndex { collection, field, unique })
        }
        "dropIndex" => {
            let field = args_str.trim_matches(|c| c == '"' || c == '\'' || c == ' ').to_string();
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

    while i < tokens.len() {
        let token_upper = tokens[i].to_uppercase();

        if token_upper == "WHERE" {
            i += 1;
            let mut where_tokens = Vec::new();
            while i < tokens.len() {
                let next_upper = tokens[i].to_uppercase();
                if ["LIMIT", "SKIP", "ORDER", "VECTOR"].contains(&next_upper.as_str()) {
                    break;
                }
                where_tokens.push(tokens[i]);
                i += 1;
            }
            if !where_tokens.is_empty() {
                filter = Some(parse_sql_where(&where_tokens.join(" "))?);
            }
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
        } else if token_upper == "VECTOR" {
            // VECTOR NEAR [0.1, 0.2] TOP 10
            i += 1;
            // look for NEAR
            if i < tokens.len() && tokens[i].eq_ignore_ascii_case("NEAR") {
                i += 1;
            }
            // parse array string
            let mut vec_str = String::new();
            while i < tokens.len() && !tokens[i].eq_ignore_ascii_case("TOP") && !tokens[i].eq_ignore_ascii_case("LIMIT") {
                vec_str.push_str(tokens[i]);
                vec_str.push(' ');
                i += 1;
            }
            let top_k = if i < tokens.len() && tokens[i].eq_ignore_ascii_case("TOP") {
                i += 1;
                let k = tokens.get(i).and_then(|s| s.parse::<usize>().ok()).unwrap_or(10);
                i += 1;
                k
            } else {
                10
            };

            let vec_parsed: Result<Vec<f32>, _> = serde_json::from_str(vec_str.trim());
            if let Ok(v) = vec_parsed {
                vector_search = Some(VectorSearchClause { vector: v, top_k });
            }
        } else {
            i += 1;
        }
    }

    Ok(Statement::Find {
        collection,
        filter,
        sort_by: None,
        limit,
        skip,
        vector_search,
        traverse: None,
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

/// Parse INSERT INTO <table> ...
fn parse_insert_query(input: &str) -> Result<Statement, String> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 3 || !tokens[1].eq_ignore_ascii_case("INTO") {
        return Err("Expected 'INSERT INTO <collection> ...'".to_string());
    }

    let collection = tokens[2].to_string();

    // Check if JSON follows or VALUES
    if let Some(json_start) = input.find('{') {
        let json_str = &input[json_start..].trim_end_matches(';').trim();
        let val: serde_json::Value =
            serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON insert: {e}"))?;
        let doc = Document::from_json_value(val).ok_or("Expected JSON object")?;
        return Ok(Statement::Insert {
            collection,
            documents: vec![doc],
        });
    }

    Err("INSERT requires JSON body: INSERT INTO <table> {\"key\": \"value\"}".to_string())
}

/// Parse DELETE FROM <table> WHERE ...
fn parse_delete_query(input: &str) -> Result<Statement, String> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 3 || !tokens[1].eq_ignore_ascii_case("FROM") {
        return Err("Expected 'DELETE FROM <collection> WHERE ...'".to_string());
    }

    let collection = tokens[2].to_string();
    let where_pos = input.to_uppercase().find("WHERE").ok_or("DELETE requires WHERE clause")?;
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

    let on_pos = upper.find("ON").ok_or("Expected 'ON <collection>(<field>)' in CREATE INDEX")?;
    let target = clean[on_pos + 2..].trim();
    
    let paren_open = target.find('(').ok_or("Expected '(' around indexed field name")?;
    let paren_close = target.find(')').ok_or("Expected ')' around indexed field name")?;

    let collection = target[..paren_open].trim().to_string();
    let field = target[paren_open + 1..paren_close].trim().to_string();

    if collection.is_empty() || field.is_empty() {
        return Err("Invalid collection or field in CREATE INDEX".to_string());
    }

    Ok(Statement::CreateIndex { collection, field, unique })
}

/// Parse DROP INDEX [idx_name] ON <collection> or DROP INDEX <field> ON <collection>
fn parse_drop_index_query(input: &str) -> Result<Statement, String> {
    let clean = input.trim_end_matches(';').trim();
    let upper = clean.to_uppercase();
    let on_pos = upper.find("ON").ok_or("Expected 'ON <collection>' in DROP INDEX")?;
    
    let index_part = clean[..on_pos].trim();
    let field = index_part.split_whitespace().last().unwrap_or("").trim_start_matches("idx_").to_string();
    let collection = clean[on_pos + 2..].trim().to_string();

    Ok(Statement::DropIndex { collection, field })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mongo_find() {
        let stmt = parse_query(r#"db.users.find({"city": "KL", "age": {"$gt": 25}})"#).unwrap();
        match stmt {
            Statement::Find { collection, filter, .. } => {
                assert_eq!(collection, "users");
                assert!(filter.is_some());
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_sql_select() {
        let stmt = parse_query("SELECT * FROM users WHERE age >= 21 AND city = 'KL' LIMIT 10").unwrap();
        match stmt {
            Statement::Find { collection, limit, filter, .. } => {
                assert_eq!(collection, "users");
                assert_eq!(limit, Some(10));
                assert!(filter.is_some());
            }
            _ => panic!("Expected Statement::Find"),
        }
    }

    #[test]
    fn test_parse_mongo_insert() {
        let stmt = parse_query(r#"db.articles.insert({"title": "FaizDB AI", "views": 1000})"#).unwrap();
        match stmt {
            Statement::Insert { collection, documents } => {
                assert_eq!(collection, "articles");
                assert_eq!(documents.len(), 1);
                assert_eq!(documents[0].get("title").unwrap().as_str(), Some("FaizDB AI"));
            }
            _ => panic!("Expected Statement::Insert"),
        }
    }
}
