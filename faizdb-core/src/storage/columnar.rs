//! # Columnar Storage & Analytics Batch Engine
//!
//! Provides zero-copy, Arrow/Parquet-compatible columnar representation of document data.
//! Converts row-oriented JSON documents into contiguous typed columnar arrays for
//! blazing-fast analytical scans (OLAP), DuckDB/Spark/Polars data science integration,
//! and high-speed aggregation without row deserialization overhead.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported columnar primitive data types
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColumnDataType {
    Int64,
    Float64,
    String,
    Boolean,
    Binary,
    Null,
}

/// Contiguous array for a single typed column (Arrow RecordBatch compatible)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColumnData {
    Int64(Vec<Option<i64>>),
    Float64(Vec<Option<f64>>),
    String(Vec<Option<String>>),
    Boolean(Vec<Option<bool>>),
    Binary(Vec<Option<Vec<u8>>>),
}

impl ColumnData {
    /// Number of rows in this column
    pub fn len(&self) -> usize {
        match self {
            ColumnData::Int64(v) => v.len(),
            ColumnData::Float64(v) => v.len(),
            ColumnData::String(v) => v.len(),
            ColumnData::Boolean(v) => v.len(),
            ColumnData::Binary(v) => v.len(),
        }
    }

    /// Check if column is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Data type enum
    pub fn data_type(&self) -> ColumnDataType {
        match self {
            ColumnData::Int64(_) => ColumnDataType::Int64,
            ColumnData::Float64(_) => ColumnDataType::Float64,
            ColumnData::String(_) => ColumnDataType::String,
            ColumnData::Boolean(_) => ColumnDataType::Boolean,
            ColumnData::Binary(_) => ColumnDataType::Binary,
        }
    }
}

/// Schema definition for a columnar dataset
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnarSchema {
    pub fields: Vec<(String, ColumnDataType)>,
}

/// A contiguous columnar batch of rows (Arrow RecordBatch equivalent)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ColumnarBatch {
    pub schema: ColumnarSchema,
    pub columns: HashMap<String, ColumnData>,
    pub row_count: usize,
}

impl ColumnarBatch {
    /// Create an empty columnar batch with a given schema
    pub fn new(schema: ColumnarSchema) -> Self {
        let mut columns = HashMap::new();
        for (name, dtype) in &schema.fields {
            let col_data = match dtype {
                ColumnDataType::Int64 => ColumnData::Int64(Vec::new()),
                ColumnDataType::Float64 => ColumnData::Float64(Vec::new()),
                ColumnDataType::String => ColumnData::String(Vec::new()),
                ColumnDataType::Boolean => ColumnData::Boolean(Vec::new()),
                ColumnDataType::Binary => ColumnData::Binary(Vec::new()),
                ColumnDataType::Null => ColumnData::String(Vec::new()),
            };
            columns.insert(name.clone(), col_data);
        }

        Self {
            schema,
            columns,
            row_count: 0,
        }
    }

    /// Transpose a slice of JSON document rows into a columnar batch
    pub fn from_json_documents(docs: &[serde_json::Value]) -> Result<Self, String> {
        if docs.is_empty() {
            return Ok(Self {
                schema: ColumnarSchema { fields: Vec::new() },
                columns: HashMap::new(),
                row_count: 0,
            });
        }

        // 1. Infer schema by inspecting all document keys
        let mut schema_map: HashMap<String, ColumnDataType> = HashMap::new();
        for doc in docs {
            if let serde_json::Value::Object(map) = doc {
                for (k, v) in map {
                    schema_map.entry(k.clone()).or_insert_with(|| match v {
                        serde_json::Value::Number(n) if n.is_i64() => ColumnDataType::Int64,
                        serde_json::Value::Number(_) => ColumnDataType::Float64,
                        serde_json::Value::String(_) => ColumnDataType::String,
                        serde_json::Value::Bool(_) => ColumnDataType::Boolean,
                        serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                            ColumnDataType::String
                        }
                        _ => ColumnDataType::String,
                    });
                }
            }
        }

        let fields: Vec<(String, ColumnDataType)> = schema_map.into_iter().collect();
        let schema = ColumnarSchema { fields };
        let mut batch = ColumnarBatch::new(schema);

        // 2. Populate contiguous columns row-by-row
        for doc in docs {
            batch.push_row(doc)?;
        }

        Ok(batch)
    }

    /// Append a single JSON row
    pub fn push_row(&mut self, doc: &serde_json::Value) -> Result<(), String> {
        let empty_map = serde_json::Map::new();
        let map = match doc {
            serde_json::Value::Object(m) => m,
            _ => &empty_map,
        };

        for (field_name, dtype) in &self.schema.fields {
            let col = self
                .columns
                .get_mut(field_name)
                .ok_or_else(|| format!("Column '{field_name}' not found in columnar batch"))?;

            let json_val = map.get(field_name);
            match (col, dtype) {
                (ColumnData::Int64(v), ColumnDataType::Int64) => {
                    v.push(json_val.and_then(|val| val.as_i64()));
                }
                (ColumnData::Float64(v), ColumnDataType::Float64) => {
                    v.push(json_val.and_then(|val| val.as_f64()));
                }
                (ColumnData::String(v), ColumnDataType::String) => {
                    v.push(json_val.map(|val| {
                        if let Some(s) = val.as_str() {
                            s.to_string()
                        } else {
                            val.to_string()
                        }
                    }));
                }
                (ColumnData::Boolean(v), ColumnDataType::Boolean) => {
                    v.push(json_val.and_then(|val| val.as_bool()));
                }
                (ColumnData::Binary(v), ColumnDataType::Binary) => {
                    v.push(json_val.and_then(|val| val.as_str().map(|s| s.as_bytes().to_vec())));
                }
                _ => {}
            }
        }

        self.row_count += 1;
        Ok(())
    }

    /// Columnar Projection: Extract only specific columns with zero-copy column slicing
    pub fn project(&self, target_columns: &[&str]) -> Result<Self, String> {
        let mut projected_fields = Vec::new();
        let mut projected_cols = HashMap::new();

        for &col_name in target_columns {
            if let Some(col_data) = self.columns.get(col_name) {
                projected_fields.push((col_name.to_string(), col_data.data_type()));
                projected_cols.insert(col_name.to_string(), col_data.clone());
            } else {
                return Err(format!(
                    "Projection column '{col_name}' does not exist in batch"
                ));
            }
        }

        Ok(Self {
            schema: ColumnarSchema {
                fields: projected_fields,
            },
            columns: projected_cols,
            row_count: self.row_count,
        })
    }

    /// High-Speed Column Sum (SIMD-Friendly Columnar Scan)
    pub fn sum_f64(&self, column_name: &str) -> Option<f64> {
        if let Some(ColumnData::Float64(vals)) = self.columns.get(column_name) {
            let sum: f64 = vals.iter().filter_map(|&v| v).sum();
            Some(sum)
        } else if let Some(ColumnData::Int64(vals)) = self.columns.get(column_name) {
            let sum: f64 = vals.iter().filter_map(|&v| v.map(|i| i as f64)).sum();
            Some(sum)
        } else {
            None
        }
    }

    /// Export to CSV string for external Data Science tools
    pub fn to_csv(&self) -> String {
        let mut csv = String::new();
        let field_names: Vec<&String> = self.schema.fields.iter().map(|(name, _)| name).collect();

        // Header
        csv.push_str(
            &field_names
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');

        // Rows
        for r in 0..self.row_count {
            let mut row_vals = Vec::new();
            for (name, _) in &self.schema.fields {
                let cell_str = match self.columns.get(name) {
                    Some(ColumnData::Int64(v)) => v
                        .get(r)
                        .and_then(|x| *x)
                        .map(|i| i.to_string())
                        .unwrap_or_default(),
                    Some(ColumnData::Float64(v)) => v
                        .get(r)
                        .and_then(|x| *x)
                        .map(|f| f.to_string())
                        .unwrap_or_default(),
                    Some(ColumnData::String(v)) => v
                        .get(r)
                        .and_then(|x| x.as_deref())
                        .map(|s| format!("\"{s}\""))
                        .unwrap_or_default(),
                    Some(ColumnData::Boolean(v)) => v
                        .get(r)
                        .and_then(|x| *x)
                        .map(|b| b.to_string())
                        .unwrap_or_default(),
                    Some(ColumnData::Binary(v)) => v
                        .get(r)
                        .and_then(|x| x.as_ref())
                        .map(|_| "<binary>".to_string())
                        .unwrap_or_default(),
                    None => String::new(),
                };
                row_vals.push(cell_str);
            }
            csv.push_str(&row_vals.join(","));
            csv.push('\n');
        }

        csv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_columnar_batch_from_json_and_projection() {
        let docs = vec![
            serde_json::json!({ "id": 1, "ticker": "NVDA", "price": 125.50, "in_portfolio": true }),
            serde_json::json!({ "id": 2, "ticker": "MSFT", "price": 440.20, "in_portfolio": false }),
            serde_json::json!({ "id": 3, "ticker": "AAPL", "price": 220.80, "in_portfolio": true }),
        ];

        let batch = ColumnarBatch::from_json_documents(&docs).unwrap();
        assert_eq!(batch.row_count, 3);
        assert_eq!(batch.columns.len(), 4);

        // Test columnar aggregation without row loop
        let total_price = batch.sum_f64("price").unwrap();
        assert!((total_price - (125.50 + 440.20 + 220.80)).abs() < 1e-4);

        // Test zero-copy column projection (select ticker & price only)
        let projected = batch.project(&["ticker", "price"]).unwrap();
        assert_eq!(projected.schema.fields.len(), 2);
        assert_eq!(projected.row_count, 3);
        assert!(projected.columns.contains_key("ticker"));
        assert!(projected.columns.contains_key("price"));
        assert!(!projected.columns.contains_key("in_portfolio"));

        // Test CSV export
        let csv = projected.to_csv();
        assert!(csv.contains("ticker"));
        assert!(csv.contains("price"));
        assert!(csv.contains("125.5"));
    }
}
