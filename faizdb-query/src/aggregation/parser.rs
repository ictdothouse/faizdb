//! Aggregation Pipeline JSON and AST Parser.

use std::collections::HashMap;
use serde_json::Value as JsonValue;

use faizdb_core::Value;
use crate::ast::{FilterExpr, Operator};
use super::pipeline::{Accumulator, PipelineStage};

/// Parse a JSON array representing an aggregation pipeline
pub fn parse_pipeline(json: &JsonValue) -> Result<Vec<PipelineStage>, String> {
    let array = match json {
        JsonValue::Array(arr) => arr,
        _ => return Err("Expected JSON array for aggregation pipeline".to_string()),
    };

    let mut stages = Vec::with_capacity(array.len());

    for item in array {
        let obj = match item {
            JsonValue::Object(o) => o,
            _ => return Err("Each pipeline stage must be a JSON object".to_string()),
        };

        if let Some(match_val) = obj.get("$match") {
            let filter = parse_match_expr(match_val)?;
            stages.push(PipelineStage::Match(filter));
        } else if let Some(group_val) = obj.get("$group") {
            let (id_expr, accumulators) = parse_group_expr(group_val)?;
            stages.push(PipelineStage::Group { id_expr, accumulators });
        } else if let Some(project_val) = obj.get("$project") {
            let (inclusions, exclusions) = parse_project_expr(project_val)?;
            stages.push(PipelineStage::Project { inclusions, exclusions });
        } else if let Some(sort_val) = obj.get("$sort") {
            let sorts = parse_sort_expr(sort_val)?;
            stages.push(PipelineStage::Sort(sorts));
        } else if let Some(limit_val) = obj.get("$limit") {
            let limit = limit_val.as_u64().ok_or("Invalid $limit number")? as usize;
            stages.push(PipelineStage::Limit(limit));
        } else if let Some(skip_val) = obj.get("$skip") {
            let skip = skip_val.as_u64().ok_or("Invalid $skip number")? as usize;
            stages.push(PipelineStage::Skip(skip));
        } else if let Some(count_val) = obj.get("$count") {
            let count_field = count_val.as_str().unwrap_or("count").to_string();
            stages.push(PipelineStage::Count(count_field));
        } else if let Some(unwind_val) = obj.get("$unwind") {
            let (path, preserve_null_and_empty_arrays) = match unwind_val {
                JsonValue::String(s) => (s.clone(), false),
                JsonValue::Object(opts) => {
                    let path = opts.get("path")
                        .and_then(|p| p.as_str())
                        .ok_or("$unwind object must contain 'path' string")?
                        .to_string();
                    let preserve = opts.get("preserveNullAndEmptyArrays")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(false);
                    (path, preserve)
                }
                _ => return Err("Invalid $unwind expression: expected string or object".to_string()),
            };
            stages.push(PipelineStage::Unwind { path, preserve_null_and_empty_arrays });
        } else if let Some(lookup_val) = obj.get("$lookup") {
            let lookup_obj = lookup_val.as_object().ok_or("$lookup stage must be a JSON object")?;
            let from = lookup_obj.get("from")
                .and_then(|v| v.as_str())
                .ok_or("$lookup requires 'from' string field")?
                .to_string();
            let local_field = lookup_obj.get("localField")
                .and_then(|v| v.as_str())
                .ok_or("$lookup requires 'localField' string field")?
                .to_string();
            let foreign_field = lookup_obj.get("foreignField")
                .and_then(|v| v.as_str())
                .ok_or("$lookup requires 'foreignField' string field")?
                .to_string();
            let as_field = lookup_obj.get("as")
                .and_then(|v| v.as_str())
                .ok_or("$lookup requires 'as' string field")?
                .to_string();

            stages.push(PipelineStage::Lookup {
                from,
                local_field,
                foreign_field,
                as_field,
            });
        }
    }

    Ok(stages)
}

fn parse_match_expr(val: &JsonValue) -> Result<FilterExpr, String> {
    let obj = match val {
        JsonValue::Object(o) => o,
        _ => return Ok(FilterExpr::AlwaysTrue),
    };

    let mut exprs = Vec::new();
    for (k, v) in obj {
        match v {
            JsonValue::String(s) => {
                exprs.push(FilterExpr::Field {
                    field: k.clone(),
                    op: Operator::Eq,
                    value: Value::String(s.clone()),
                });
            }
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    exprs.push(FilterExpr::Field {
                        field: k.clone(),
                        op: Operator::Eq,
                        value: Value::Integer(i),
                    });
                } else if let Some(f) = n.as_f64() {
                    exprs.push(FilterExpr::Field {
                        field: k.clone(),
                        op: Operator::Eq,
                        value: Value::Float(f),
                    });
                }
            }
            JsonValue::Bool(b) => {
                exprs.push(FilterExpr::Field {
                    field: k.clone(),
                    op: Operator::Eq,
                    value: Value::Boolean(*b),
                });
            }
            JsonValue::Object(sub) => {
                for (op, op_val) in sub {
                    let faiz_val = match op_val {
                        JsonValue::String(s) => Value::String(s.clone()),
                        JsonValue::Number(n) => {
                            if let Some(i) = n.as_i64() {
                                Value::Integer(i)
                            } else {
                                Value::Float(n.as_f64().unwrap_or(0.0))
                            }
                        }
                        JsonValue::Bool(b) => Value::Boolean(*b),
                        _ => Value::Null,
                    };

                    let operator = match op.as_str() {
                        "$gt" => Operator::Gt,
                        "$gte" => Operator::Gte,
                        "$lt" => Operator::Lt,
                        "$lte" => Operator::Lte,
                        "$ne" => Operator::Neq,
                        _ => Operator::Eq,
                    };

                    exprs.push(FilterExpr::Field {
                        field: k.clone(),
                        op: operator,
                        value: faiz_val,
                    });
                }
            }
            _ => {}
        }
    }

    if exprs.is_empty() {
        Ok(FilterExpr::AlwaysTrue)
    } else if exprs.len() == 1 {
        Ok(exprs.remove(0))
    } else {
        Ok(FilterExpr::And(exprs))
    }
}

fn parse_group_expr(val: &JsonValue) -> Result<(String, HashMap<String, Accumulator>), String> {
    let obj = match val {
        JsonValue::Object(o) => o,
        _ => return Err("Invalid $group object".to_string()),
    };

    let id_expr = match obj.get("_id") {
        Some(JsonValue::String(s)) => s.clone(),
        Some(JsonValue::Null) => "null".to_string(),
        _ => "".to_string(),
    };

    let mut accumulators = HashMap::new();
    for (k, v) in obj {
        if k == "_id" {
            continue;
        }

        if let JsonValue::Object(acc_obj) = v {
            if let Some(sum_val) = acc_obj.get("$sum") {
                let field = match sum_val {
                    JsonValue::String(s) => s.clone(),
                    JsonValue::Number(n) => n.to_string(),
                    _ => "1".to_string(),
                };
                accumulators.insert(k.clone(), Accumulator::Sum(field));
            } else if let Some(avg_val) = acc_obj.get("$avg") {
                let field = avg_val.as_str().unwrap_or("").to_string();
                accumulators.insert(k.clone(), Accumulator::Avg(field));
            } else if let Some(min_val) = acc_obj.get("$min") {
                let field = min_val.as_str().unwrap_or("").to_string();
                accumulators.insert(k.clone(), Accumulator::Min(field));
            } else if let Some(max_val) = acc_obj.get("$max") {
                let field = max_val.as_str().unwrap_or("").to_string();
                accumulators.insert(k.clone(), Accumulator::Max(field));
            } else if let Some(push_val) = acc_obj.get("$push") {
                let field = push_val.as_str().unwrap_or("").to_string();
                accumulators.insert(k.clone(), Accumulator::Push(field));
            } else if let Some(first_val) = acc_obj.get("$first") {
                let field = first_val.as_str().unwrap_or("").to_string();
                accumulators.insert(k.clone(), Accumulator::First(field));
            } else if let Some(last_val) = acc_obj.get("$last") {
                let field = last_val.as_str().unwrap_or("").to_string();
                accumulators.insert(k.clone(), Accumulator::Last(field));
            } else if acc_obj.contains_key("$count") {
                accumulators.insert(k.clone(), Accumulator::Count);
            }
        }
    }

    Ok((id_expr, accumulators))
}

fn parse_project_expr(val: &JsonValue) -> Result<(Vec<String>, Vec<String>), String> {
    let obj = match val {
        JsonValue::Object(o) => o,
        _ => return Ok((vec![], vec![])),
    };

    let mut inclusions = Vec::new();
    let mut exclusions = Vec::new();

    for (k, v) in obj {
        match v {
            JsonValue::Number(n) => {
                if n.as_i64() == Some(1) {
                    inclusions.push(k.clone());
                } else if n.as_i64() == Some(0) {
                    exclusions.push(k.clone());
                }
            }
            JsonValue::Bool(true) => inclusions.push(k.clone()),
            JsonValue::Bool(false) => exclusions.push(k.clone()),
            _ => inclusions.push(k.clone()),
        }
    }

    Ok((inclusions, exclusions))
}

fn parse_sort_expr(val: &JsonValue) -> Result<Vec<(String, i8)>, String> {
    let obj = match val {
        JsonValue::Object(o) => o,
        _ => return Ok(vec![]),
    };

    let mut sorts = Vec::new();
    for (k, v) in obj {
        let dir = match v {
            JsonValue::Number(n) => {
                if n.as_i64().unwrap_or(1) < 0 { -1 } else { 1 }
            }
            _ => 1,
        };
        sorts.push((k.clone(), dir));
    }

    Ok(sorts)
}
