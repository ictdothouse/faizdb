//! Change Stream Event Data Model.

use std::collections::BTreeMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::model::{Document, Value};

/// Type of database operation in the change stream
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationType {
    Insert,
    Update,
    Delete,
    Replace,
    Drop,
}

/// A reactive change event emitted on document mutations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeEvent {
    /// Resume token for reliable reconnection
    pub resume_token: String,
    /// Wall-clock timestamp when mutation occurred
    pub timestamp: DateTime<Utc>,
    /// Target collection name
    pub collection: String,
    /// Type of mutation operation
    pub operation_type: OperationType,
    /// Identifier of the affected document
    pub document_id: String,
    /// Full document snapshot (for inserts, replaces, or full updates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_document: Option<Document>,
    /// Sparse map of modified fields (for partial updates)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_fields: Option<BTreeMap<String, Value>>,
}

impl ChangeEvent {
    /// Create an Insert change event
    pub fn insert(collection: &str, doc: Document) -> Self {
        let doc_id = doc.id.as_str().to_string();
        Self {
            resume_token: Uuid::now_v7().to_string(),
            timestamp: Utc::now(),
            collection: collection.to_string(),
            operation_type: OperationType::Insert,
            document_id: doc_id,
            full_document: Some(doc),
            updated_fields: None,
        }
    }

    /// Create an Update change event
    pub fn update(collection: &str, doc_id: &str, updated_fields: BTreeMap<String, Value>, full_doc: Option<Document>) -> Self {
        Self {
            resume_token: Uuid::now_v7().to_string(),
            timestamp: Utc::now(),
            collection: collection.to_string(),
            operation_type: OperationType::Update,
            document_id: doc_id.to_string(),
            full_document: full_doc,
            updated_fields: Some(updated_fields),
        }
    }

    /// Create a Delete change event
    pub fn delete(collection: &str, doc_id: &str) -> Self {
        Self {
            resume_token: Uuid::now_v7().to_string(),
            timestamp: Utc::now(),
            collection: collection.to_string(),
            operation_type: OperationType::Delete,
            document_id: doc_id.to_string(),
            full_document: None,
            updated_fields: None,
        }
    }

    /// Create a Drop collection change event
    pub fn drop_collection(collection: &str) -> Self {
        Self {
            resume_token: Uuid::now_v7().to_string(),
            timestamp: Utc::now(),
            collection: collection.to_string(),
            operation_type: OperationType::Drop,
            document_id: String::new(),
            full_document: None,
            updated_fields: None,
        }
    }
}
