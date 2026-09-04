//! Aggregation Pipeline Execution Engine.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

use crate::ast::FilterExpr;
use faizdb_core::document::model::{Document, Value};

/// Statistical accumulator operators in `$group` stage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Accumulator {
    Sum(String),   // "$field" or literal "1"
    Avg(String),   // "$field"
    Min(String),   // "$field"
    Max(String),   // "$field"
    Count,         // Equivalent to $sum: 1
    Push(String),  // Accumulate array of values
    First(String), // First value encountered
    Last(String),  // Last value encountered
}

/// A stage in the aggregation pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PipelineStage {
    Match(FilterExpr),
    Group {
        id_expr: String, // e.g. "$country", "$category" or "" (all)
        accumulators: HashMap<String, Accumulator>,
    },
    Project {
        inclusions: Vec<String>,
        exclusions: Vec<String>,
    },
    Sort(Vec<(String, i8)>), // (field, 1 = asc, -1 = desc)
    Limit(usize),
    Skip(usize),
    Count(String), // Output field name for count
    Unwind {
        path: String,
        preserve_null_and_empty_arrays: bool,
    },
    Lookup {
        from: String,
        local_field: String,
        foreign_field: String,
        as_field: String,
    },
}

/// Execute a sequence of pipeline stages over input documents with a foreign collection resolver
pub fn execute_pipeline_with_collections<F>(
    mut docs: Vec<Document>,
    stages: &[PipelineStage],
    get_collection_docs: F,
) -> Vec<Document>
where
    F: Fn(&str) -> Vec<Document>,
{
    for stage in stages {
        docs = match stage {
            PipelineStage::Match(filter) => {
                docs.into_iter().filter(|doc| filter.matches(doc)).collect()
            }

            PipelineStage::Group {
                id_expr,
                accumulators,
            } => execute_group_stage(docs, id_expr, accumulators),

            PipelineStage::Project {
                inclusions,
                exclusions,
            } => docs
                .into_iter()
                .map(|doc| {
                    let mut new_doc = Document::new();
                    if let Some(id) = doc.get("_id") {
                        new_doc.set("_id", id.clone());
                    }
                    if !inclusions.is_empty() {
                        for field in inclusions {
                            if let Some(val) = doc.get_nested(field) {
                                new_doc.set(field.clone(), val.clone());
                            }
                        }
                    } else {
                        for (k, v) in &doc.fields {
                            if !exclusions.contains(k) {
                                new_doc.set(k.clone(), v.clone());
                            }
                        }
                    }
                    new_doc
                })
                .collect(),

            PipelineStage::Sort(sort_keys) => {
                docs.sort_by(|a, b| {
                    for (field, dir) in sort_keys {
                        let va = a.get_nested(field);
                        let vb = b.get_nested(field);
                        let ord = match (va, vb) {
                            (Some(Value::Integer(x)), Some(Value::Integer(y))) => x.cmp(y),
                            (Some(Value::Float(x)), Some(Value::Float(y))) => {
                                x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal)
                            }
                            (Some(Value::String(x)), Some(Value::String(y))) => x.cmp(y),
                            (Some(x), Some(y)) => x.to_string().cmp(&y.to_string()),
                            (Some(_), None) => std::cmp::Ordering::Greater,
                            (None, Some(_)) => std::cmp::Ordering::Less,
                            (None, None) => std::cmp::Ordering::Equal,
                        };
                        if ord != std::cmp::Ordering::Equal {
                            return if *dir < 0 { ord.reverse() } else { ord };
                        }
                    }
                    std::cmp::Ordering::Equal
                });
                docs
            }

            PipelineStage::Skip(n) => docs.into_iter().skip(*n).collect(),

            PipelineStage::Limit(n) => docs.into_iter().take(*n).collect(),

            PipelineStage::Count(output_field) => {
                let mut doc = Document::new();
                doc.set(output_field.clone(), docs.len() as i64);
                vec![doc]
            }

            PipelineStage::Unwind {
                path,
                preserve_null_and_empty_arrays,
            } => {
                let clean_path = path.strip_prefix('$').unwrap_or(path);
                let mut unwound = Vec::new();
                for doc in docs {
                    match doc.get_nested(clean_path) {
                        Some(Value::Array(arr)) if !arr.is_empty() => {
                            for item in arr {
                                let mut new_doc = doc.clone();
                                new_doc.set(clean_path, item.clone());
                                unwound.push(new_doc);
                            }
                        }
                        _ => {
                            if *preserve_null_and_empty_arrays {
                                let mut new_doc = doc.clone();
                                new_doc.set(clean_path, Value::Null);
                                unwound.push(new_doc);
                            }
                        }
                    }
                }
                unwound
            }

            PipelineStage::Lookup {
                from,
                local_field,
                foreign_field,
                as_field,
            } => {
                let foreign_docs = get_collection_docs(from);
                let clean_local = local_field.strip_prefix('$').unwrap_or(local_field);
                let clean_foreign = foreign_field.strip_prefix('$').unwrap_or(foreign_field);

                for doc in &mut docs {
                    let local_val = doc.get_nested(clean_local).cloned();
                    let mut matched = Vec::new();
                    if let Some(ref l_val) = local_val {
                        for f_doc in &foreign_docs {
                            if let Some(f_val) = f_doc.get_nested(clean_foreign) {
                                if f_val == l_val {
                                    matched.push(Value::Object(f_doc.fields.clone()));
                                }
                            }
                        }
                    }
                    doc.set(as_field.clone(), Value::Array(matched));
                }
                docs
            }
        };
    }

    docs
}

/// Execute a sequence of pipeline stages over input documents
pub fn execute_pipeline(docs: Vec<Document>, stages: &[PipelineStage]) -> Vec<Document> {
    execute_pipeline_with_collections(docs, stages, |_| Vec::new())
}

fn execute_group_stage(
    docs: Vec<Document>,
    id_expr: &str,
    accumulators: &HashMap<String, Accumulator>,
) -> Vec<Document> {
    let clean_id_field = id_expr.trim_start_matches('$');
    let mut groups: BTreeMap<String, Vec<Document>> = BTreeMap::new();

    for doc in docs {
        let group_key = if id_expr.is_empty() || id_expr == "null" {
            "null".to_string()
        } else if let Some(val) = doc.get_nested(clean_id_field) {
            match val {
                Value::String(s) => s.clone(),
                Value::Integer(i) => i.to_string(),
                Value::Float(f) => f.to_string(),
                Value::Boolean(b) => b.to_string(),
                _ => val.to_string(),
            }
        } else {
            "null".to_string()
        };

        groups.entry(group_key).or_default().push(doc);
    }

    let mut result_docs = Vec::with_capacity(groups.len());

    for (group_key, group_docs) in groups {
        let mut out_doc = Document::new();
        if group_key != "null" {
            out_doc.set("_id", group_key);
        } else {
            out_doc.set("_id", Value::Null);
        }

        for (out_field, acc) in accumulators {
            match acc {
                Accumulator::Sum(field_expr) => {
                    let field = field_expr.trim_start_matches('$');
                    if field == "1" || field_expr == "1" {
                        out_doc.set(out_field.clone(), group_docs.len() as i64);
                    } else {
                        let sum: f64 = group_docs
                            .iter()
                            .filter_map(|d| match d.get_nested(field) {
                                Some(Value::Integer(i)) => Some(*i as f64),
                                Some(Value::Float(f)) => Some(*f),
                                _ => None,
                            })
                            .sum();
                        if sum.fract() == 0.0 {
                            out_doc.set(out_field.clone(), sum as i64);
                        } else {
                            out_doc.set(out_field.clone(), sum);
                        }
                    }
                }

                Accumulator::Avg(field_expr) => {
                    let field = field_expr.trim_start_matches('$');
                    let values: Vec<f64> = group_docs
                        .iter()
                        .filter_map(|d| match d.get_nested(field) {
                            Some(Value::Integer(i)) => Some(*i as f64),
                            Some(Value::Float(f)) => Some(*f),
                            _ => None,
                        })
                        .collect();

                    if !values.is_empty() {
                        let avg = values.iter().sum::<f64>() / (values.len() as f64);
                        let rounded = (avg * 100.0).round() / 100.0;
                        out_doc.set(out_field.clone(), rounded);
                    } else {
                        out_doc.set(out_field.clone(), 0.0);
                    }
                }

                Accumulator::Min(field_expr) => {
                    let field = field_expr.trim_start_matches('$');
                    let mut min_val: Option<f64> = None;
                    for d in &group_docs {
                        if let Some(val) = d.get_nested(field) {
                            let num = match val {
                                Value::Integer(i) => Some(*i as f64),
                                Value::Float(f) => Some(*f),
                                _ => None,
                            };
                            if let Some(n) = num {
                                min_val = Some(min_val.map_or(n, |m| m.min(n)));
                            }
                        }
                    }
                    if let Some(m) = min_val {
                        out_doc.set(out_field.clone(), m);
                    }
                }

                Accumulator::Max(field_expr) => {
                    let field = field_expr.trim_start_matches('$');
                    let mut max_val: Option<f64> = None;
                    for d in &group_docs {
                        if let Some(val) = d.get_nested(field) {
                            let num = match val {
                                Value::Integer(i) => Some(*i as f64),
                                Value::Float(f) => Some(*f),
                                _ => None,
                            };
                            if let Some(n) = num {
                                max_val = Some(max_val.map_or(n, |m| m.max(n)));
                            }
                        }
                    }
                    if let Some(m) = max_val {
                        out_doc.set(out_field.clone(), m);
                    }
                }

                Accumulator::Count => {
                    out_doc.set(out_field.clone(), group_docs.len() as i64);
                }

                Accumulator::Push(field_expr) => {
                    let field = field_expr.trim_start_matches('$');
                    let arr: Vec<Value> = group_docs
                        .iter()
                        .filter_map(|d| d.get_nested(field).cloned())
                        .collect();
                    out_doc.set(out_field.clone(), Value::Array(arr));
                }

                Accumulator::First(field_expr) => {
                    let field = field_expr.trim_start_matches('$');
                    if let Some(first_doc) = group_docs.first() {
                        if let Some(val) = first_doc.get_nested(field) {
                            out_doc.set(out_field.clone(), val.clone());
                        }
                    }
                }

                Accumulator::Last(field_expr) => {
                    let field = field_expr.trim_start_matches('$');
                    if let Some(last_doc) = group_docs.last() {
                        if let Some(val) = last_doc.get_nested(field) {
                            out_doc.set(out_field.clone(), val.clone());
                        }
                    }
                }
            }
        }

        result_docs.push(out_doc);
    }

    result_docs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unwind_pipeline_stage() {
        let mut d1 = Document::new();
        d1.set("name", "Faiz");
        d1.set(
            "skills",
            Value::Array(vec![
                Value::String("Rust".into()),
                Value::String("Databases".into()),
                Value::String("AI".into()),
            ]),
        );

        let mut d2 = Document::new();
        d2.set("name", "Solo");
        d2.set("skills", Value::Array(vec![]));

        let docs = vec![d1, d2];
        let stages = vec![PipelineStage::Unwind {
            path: "$skills".into(),
            preserve_null_and_empty_arrays: false,
        }];

        let res = execute_pipeline(docs.clone(), &stages);
        assert_eq!(res.len(), 3);
        assert_eq!(res[0].get("skills").unwrap().as_str().unwrap(), "Rust");
        assert_eq!(res[1].get("skills").unwrap().as_str().unwrap(), "Databases");
        assert_eq!(res[2].get("skills").unwrap().as_str().unwrap(), "AI");

        let stages_preserve = vec![PipelineStage::Unwind {
            path: "$skills".into(),
            preserve_null_and_empty_arrays: true,
        }];
        let res_preserve = execute_pipeline(docs, &stages_preserve);
        assert_eq!(res_preserve.len(), 4);
    }

    #[test]
    fn test_lookup_pipeline_stage() {
        let mut user1 = Document::new();
        user1.set("_id", "u1");
        user1.set("name", "Faiz");

        let mut user2 = Document::new();
        user2.set("_id", "u2");
        user2.set("name", "Elon");

        let mut order1 = Document::new();
        order1.set("_id", "o1");
        order1.set("user_id", "u1");
        order1.set("total", 100.0);

        let mut order2 = Document::new();
        order2.set("_id", "o2");
        order2.set("user_id", "u1");
        order2.set("total", 250.0);

        let users = vec![user1, user2];
        let orders = vec![order1, order2];

        let stages = vec![PipelineStage::Lookup {
            from: "orders".into(),
            local_field: "_id".into(),
            foreign_field: "user_id".into(),
            as_field: "user_orders".into(),
        }];

        let res = execute_pipeline_with_collections(users, &stages, |from_col| {
            if from_col == "orders" {
                orders.clone()
            } else {
                vec![]
            }
        });

        assert_eq!(res.len(), 2);
        // user1 ("Faiz") should have 2 orders
        if let Some(Value::Array(items)) = res[0].get("user_orders") {
            assert_eq!(items.len(), 2);
        } else {
            panic!("Expected array of orders for user1");
        }

        // user2 ("Elon") should have 0 orders
        if let Some(Value::Array(items)) = res[1].get("user_orders") {
            assert_eq!(items.len(), 0);
        } else {
            panic!("Expected empty array of orders for user2");
        }
    }
}
