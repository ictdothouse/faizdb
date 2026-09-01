//! Error types for FaizDB
//!
//! All errors in the core engine are represented by [`FaizError`].
//! This ensures consistent error handling across all layers.

use std::path::PathBuf;

/// Result type alias for FaizDB operations
pub type FaizResult<T> = Result<T, FaizError>;

/// Core error type for all FaizDB operations
#[derive(Debug, thiserror::Error)]
pub enum FaizError {
    // ── Storage Errors ───────────────────────────────────────────
    #[error("I/O error at '{path}': {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },

    #[error("WAL corrupted at offset {offset}: {detail}")]
    WalCorrupted { offset: u64, detail: String },

    #[error("SSTable corrupted: {0}")]
    SsTableCorrupted(String),

    #[error("Data file has invalid magic bytes")]
    InvalidMagicBytes,

    #[error("Checksum mismatch: expected {expected:#010x}, got {actual:#010x}")]
    ChecksumMismatch { expected: u32, actual: u32 },

    #[error("Storage engine is closed")]
    EngineClosed,

    // ── Document Errors ──────────────────────────────────────────
    #[error("Document not found: {collection}/{id}")]
    DocumentNotFound { collection: String, id: String },

    #[error("Document too large: {size} bytes (max: {max} bytes)")]
    DocumentTooLarge { size: usize, max: usize },

    #[error("Invalid document: {0}")]
    InvalidDocument(String),

    #[error("Collection not found: {0}")]
    CollectionNotFound(String),

    #[error("Collection already exists: {0}")]
    CollectionAlreadyExists(String),

    #[error("Duplicate key: {collection}.{field} = {value}")]
    DuplicateKey {
        collection: String,
        field: String,
        value: String,
    },

    // ── Encoding Errors ──────────────────────────────────────────
    #[error("BSON encoding error: {0}")]
    BsonEncode(String),

    #[error("BSON decoding error: {0}")]
    BsonDecode(String),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    // ── Transaction Errors ───────────────────────────────────────
    #[error("Transaction conflict: {0}")]
    TransactionConflict(String),

    #[error("Transaction aborted: {0}")]
    TransactionAborted(String),

    #[error("Transaction timeout after {0}ms")]
    TransactionTimeout(u64),

    #[error("Deadlock detected")]
    Deadlock,

    // ── Schema Errors ────────────────────────────────────────────
    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    // ── Security Errors ──────────────────────────────────────────
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    // ── Internal Errors ──────────────────────────────────────────
    #[error("Internal error: {0}")]
    Internal(String),
}

impl FaizError {
    /// Create an I/O error with path context
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        FaizError::Io {
            path: path.into(),
            source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = FaizError::DocumentNotFound {
            collection: "users".into(),
            id: "abc123".into(),
        };
        assert_eq!(err.to_string(), "Document not found: users/abc123");
    }

    #[test]
    fn test_checksum_error() {
        let err = FaizError::ChecksumMismatch {
            expected: 0xDEADBEEF,
            actual: 0xCAFEBABE,
        };
        assert!(err.to_string().contains("0xdeadbeef"));
    }
}
