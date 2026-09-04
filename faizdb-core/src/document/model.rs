//! Core document model and value types.
//!
//! FaizDB documents are flexible, schema-optional data structures that
//! support a rich set of types beyond what MongoDB or PostgreSQL JSONB offer.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use uuid::Uuid;

/// A unique identifier for a document.
///
/// Uses UUID v7 (time-sortable) by default, which provides:
/// - Natural ordering by insertion time
/// - Global uniqueness without coordination
/// - Better index locality than random UUIDs
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DocumentId(String);

impl DocumentId {
    /// Create a new time-sortable document ID (UUID v7)
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }

    /// Create a DocumentId from a string
    pub fn from_string(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Get the ID as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for DocumentId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for DocumentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for DocumentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// A flexible value type that represents any data in FaizDB.
///
/// This is richer than MongoDB's BSON types and PostgreSQL's JSONB,
/// combining the best of both worlds with AI-native extensions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    /// Null/missing value
    Null,

    /// Boolean value
    Boolean(bool),

    /// 64-bit signed integer
    Integer(i64),

    /// 64-bit floating point number
    Float(f64),

    /// UTF-8 string
    String(String),

    /// Ordered array of values
    Array(Vec<Value>),

    /// Key-value object (BTreeMap for ordered keys)
    Object(BTreeMap<String, Value>),

    /// Binary data (for files, images, etc.)
    Binary(Vec<u8>),

    /// UTC datetime with nanosecond precision
    DateTime(DateTime<Utc>),

    /// UUID value
    Uuid(Uuid),

    /// Vector embedding for AI/ML operations
    /// Stored as f32 for memory efficiency (standard in ML)
    Vector(Vec<f32>),
}

impl Value {
    /// Check if the value is null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Try to get as a string reference
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as an integer
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(n) => Some(*n),
            Value::Float(f) => Some(*f as i64),
            _ => None,
        }
    }

    /// Try to get as a float
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            Value::Integer(n) => Some(*n as f64),
            _ => None,
        }
    }

    /// Try to get as a boolean
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as an array
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// Try to get as an object
    pub fn as_object(&self) -> Option<&BTreeMap<String, Value>> {
        match self {
            Value::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// Try to get as a vector embedding
    pub fn as_vector(&self) -> Option<&[f32]> {
        match self {
            Value::Vector(v) => Some(v),
            _ => None,
        }
    }

    /// Get the type name as a string
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Boolean(_) => "boolean",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
            Value::Binary(_) => "binary",
            Value::DateTime(_) => "datetime",
            Value::Uuid(_) => "uuid",
            Value::Vector(_) => "vector",
        }
    }

    /// Get the approximate memory size in bytes
    pub fn size_bytes(&self) -> usize {
        match self {
            Value::Null => 0,
            Value::Boolean(_) => 1,
            Value::Integer(_) => 8,
            Value::Float(_) => 8,
            Value::String(s) => s.len(),
            Value::Array(arr) => arr.iter().map(|v| v.size_bytes()).sum::<usize>() + 24,
            Value::Object(obj) => {
                obj.iter()
                    .map(|(k, v)| k.len() + v.size_bytes())
                    .sum::<usize>()
                    + 48
            }
            Value::Binary(b) => b.len(),
            Value::DateTime(_) => 12,
            Value::Uuid(_) => 16,
            Value::Vector(v) => v.len() * 4,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Boolean(b) => write!(f, "{b}"),
            Value::Integer(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::String(s) => write!(f, "\"{s}\""),
            Value::Array(arr) => {
                write!(f, "[")?;
                for (i, v) in arr.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{v}")?;
                }
                write!(f, "]")
            }
            Value::Object(obj) => {
                write!(f, "{{")?;
                for (i, (k, v)) in obj.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{k}\": {v}")?;
                }
                write!(f, "}}")
            }
            Value::Binary(b) => write!(f, "<binary {} bytes>", b.len()),
            Value::DateTime(dt) => write!(f, "{dt}"),
            Value::Uuid(u) => write!(f, "\"{u}\""),
            Value::Vector(v) => write!(f, "<vector dim={}>", v.len()),
        }
    }
}

// ── Convenient From implementations ──────────────────────────────

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Boolean(v)
    }
}

impl From<i32> for Value {
    fn from(v: i32) -> Self {
        Value::Integer(v as i64)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Integer(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Float(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v)
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string())
    }
}

impl<T: Into<Value>> From<Vec<T>> for Value {
    fn from(v: Vec<T>) -> Self {
        Value::Array(v.into_iter().map(Into::into).collect())
    }
}

impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Boolean(b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::Integer(i)
                } else {
                    Value::Float(n.as_f64().unwrap_or(0.0))
                }
            }
            serde_json::Value::String(s) => Value::String(s),
            serde_json::Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Value::from).collect())
            }
            serde_json::Value::Object(obj) => {
                Value::Object(obj.into_iter().map(|(k, v)| (k, Value::from(v))).collect())
            }
        }
    }
}

/// A FaizDB document — the fundamental unit of data.
///
/// Every document has:
/// - A unique `_id` field (auto-generated UUID v7 if not provided)
/// - A flexible set of fields (key-value pairs)
/// - Metadata: creation time, update time, version
///
/// # Example
///
/// ```rust
/// use faizdb_core::document::model::{Document, Value};
///
/// let doc = Document::new()
///     .field("name", "Ahmad Faiz")
///     .field("age", 30)
///     .field("skills", vec!["rust", "databases", "ai"]);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    /// Unique document identifier
    #[serde(rename = "_id")]
    pub id: DocumentId,

    /// The document's fields
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,

    /// Document metadata (internal use)
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    pub metadata: Option<DocumentMetadata>,
}

/// Internal metadata for a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Document version (incremented on each update)
    pub version: u64,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    pub updated_at: DateTime<Utc>,

    /// Collection this document belongs to
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
}

impl Document {
    /// Create a new empty document with auto-generated ID
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: DocumentId::new(),
            fields: BTreeMap::new(),
            metadata: Some(DocumentMetadata {
                version: 1,
                created_at: now,
                updated_at: now,
                collection: None,
            }),
        }
    }

    /// Create a document with a specific ID
    pub fn with_id(id: impl Into<DocumentId>) -> Self {
        let now = Utc::now();
        Self {
            id: id.into(),
            fields: BTreeMap::new(),
            metadata: Some(DocumentMetadata {
                version: 1,
                created_at: now,
                updated_at: now,
                collection: None,
            }),
        }
    }

    /// Builder pattern: set a field and return self
    pub fn field(mut self, key: impl Into<String>, value: impl Into<Value>) -> Self {
        self.fields.insert(key.into(), value.into());
        self
    }

    /// Set a field on an existing document
    pub fn set(&mut self, key: impl Into<String>, value: impl Into<Value>) {
        let k = key.into();
        let v = value.into();
        if k == "_id" {
            if let Some(s) = v.as_str() {
                self.id = DocumentId::from(s);
            }
        }
        self.fields.insert(k, v);
        if let Some(meta) = &mut self.metadata {
            meta.updated_at = Utc::now();
            meta.version += 1;
        }
    }

    /// Get a field value
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    /// Get a nested field using dot notation (e.g., "address.city")
    pub fn get_nested(&self, path: &str) -> Option<&Value> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current: &Value = self.fields.get(parts[0])?;

        for &part in &parts[1..] {
            match current {
                Value::Object(obj) => {
                    current = obj.get(part)?;
                }
                Value::Array(arr) => {
                    let idx: usize = part.parse().ok()?;
                    current = arr.get(idx)?;
                }
                _ => return None,
            }
        }

        Some(current)
    }

    /// Remove a field and return its value
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        let result = self.fields.remove(key);
        if result.is_some() {
            if let Some(meta) = &mut self.metadata {
                meta.updated_at = Utc::now();
                meta.version += 1;
            }
        }
        result
    }

    /// Check if a field exists
    pub fn contains(&self, key: &str) -> bool {
        self.fields.contains_key(key)
    }

    /// Get the number of top-level fields (excluding _id and _meta)
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Calculate the approximate size of this document in bytes
    pub fn size_bytes(&self) -> usize {
        let id_size = self.id.as_str().len();
        let fields_size: usize = self
            .fields
            .iter()
            .map(|(k, v)| k.len() + v.size_bytes())
            .sum();
        id_size + fields_size + 128 // 128 bytes overhead for metadata
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }

    /// Convert to pretty JSON string
    pub fn to_json_pretty(&self) -> serde_json::Result<String> {
        serde_json::to_string_pretty(self)
    }

    /// Create document from JSON string
    pub fn from_json(json: &str) -> serde_json::Result<Self> {
        serde_json::from_str(json)
    }

    /// Create document from a serde_json::Value
    pub fn from_json_value(value: serde_json::Value) -> Option<Self> {
        if let serde_json::Value::Object(map) = value {
            let mut doc = Document::new();

            for (key, val) in map {
                if key == "_id" {
                    if let serde_json::Value::String(id) = val {
                        doc.id = DocumentId::from_string(id);
                    }
                } else {
                    doc.fields.insert(key, Value::from(val));
                }
            }

            Some(doc)
        } else {
            None
        }
    }

    /// Serialize document to bytes for storage
    pub fn to_bytes(&self) -> crate::error::FaizResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| crate::error::FaizError::Internal(e.to_string()))
    }

    /// Deserialize document from bytes
    pub fn from_bytes(bytes: &[u8]) -> crate::error::FaizResult<Self> {
        serde_json::from_slice(bytes).map_err(|e| crate::error::FaizError::Internal(e.to_string()))
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Document {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_json_pretty() {
            Ok(json) => write!(f, "{json}"),
            Err(_) => write!(f, "Document({})", self.id),
        }
    }
}

/// Macro for creating documents with a concise syntax
///
/// # Example
/// ```rust
/// use faizdb_core::doc;
///
/// let user = doc! {
///     "name" => "Faiz",
///     "age" => 30,
///     "active" => true
/// };
/// ```
#[macro_export]
macro_rules! doc {
    ($($key:expr => $value:expr),* $(,)?) => {{
        let mut doc = $crate::document::model::Document::new();
        $(
            doc.set($key, $value);
        )*
        doc
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_document() {
        let doc = Document::new()
            .field("name", "Ahmad Faiz")
            .field("age", 30)
            .field("active", true);

        assert_eq!(doc.get("name").unwrap().as_str(), Some("Ahmad Faiz"));
        assert_eq!(doc.get("age").unwrap().as_i64(), Some(30));
        assert_eq!(doc.get("active").unwrap().as_bool(), Some(true));
        assert_eq!(doc.field_count(), 3);
    }

    #[test]
    fn test_nested_document() {
        let mut address = BTreeMap::new();
        address.insert("city".to_string(), Value::String("Kuala Lumpur".into()));
        address.insert("country".to_string(), Value::String("Malaysia".into()));

        let doc = Document::new()
            .field("name", "Faiz")
            .field("address", Value::Object(address));

        assert_eq!(
            doc.get_nested("address.city").unwrap().as_str(),
            Some("Kuala Lumpur")
        );
    }

    #[test]
    fn test_document_serialization() {
        let doc = Document::new().field("name", "Faiz").field("score", 99.5);

        let json = doc.to_json().unwrap();
        let restored = Document::from_json(&json).unwrap();

        assert_eq!(restored.get("name").unwrap().as_str(), Some("Faiz"));
    }

    #[test]
    fn test_document_id_ordering() {
        let id1 = DocumentId::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let id2 = DocumentId::new();

        // UUID v7 IDs should be naturally ordered by creation time
        assert!(id1 < id2);
    }

    #[test]
    fn test_vector_value() {
        let embedding = Value::Vector(vec![0.1, 0.2, 0.3, 0.4]);
        assert_eq!(embedding.as_vector().unwrap().len(), 4);
        assert_eq!(embedding.type_name(), "vector");
        assert_eq!(embedding.size_bytes(), 16); // 4 floats × 4 bytes
    }

    #[test]
    fn test_doc_macro() {
        let doc = doc! {
            "name" => "Faiz",
            "role" => "Creator"
        };
        assert_eq!(doc.get("name").unwrap().as_str(), Some("Faiz"));
        assert_eq!(doc.get("role").unwrap().as_str(), Some("Creator"));
    }

    #[test]
    fn test_document_size() {
        let doc = Document::new()
            .field("name", "Faiz")
            .field("bio", "A long biography text that takes up space");

        let size = doc.size_bytes();
        assert!(size > 0);
    }
}
