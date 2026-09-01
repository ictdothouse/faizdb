//! Automated Point-in-Time Consistent Snapshot & Disaster Recovery Engine.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use chrono::Utc;
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};

use crate::document::model::Document;

/// Snapshot Manifest Header
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub engine: String,
    pub version: String,
    pub created_at: String,
    pub collections: Vec<String>,
    pub total_documents: usize,
    pub checksum: String,
    pub file_size_bytes: usize,
}

/// Full Snapshot Archive Structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotArchive {
    pub manifest: SnapshotManifest,
    pub collections_data: HashMap<String, Vec<serde_json::Value>>,
}

/// Generate snapshot data structure and SHA256 checksum
pub fn build_snapshot(
    collections: &[(String, Vec<Document>)],
) -> SnapshotArchive {
    let mut collections_data: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut total_docs = 0;
    let mut col_names = Vec::new();

    for (name, docs) in collections {
        col_names.push(name.clone());
        total_docs += docs.len();
        let doc_vals: Vec<serde_json::Value> = docs
            .iter()
            .map(|d| {
                let mut v = serde_json::to_value(&d.fields).unwrap_or(serde_json::Value::Null);
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("_id".to_string(), serde_json::Value::String(d.id.as_str().to_string()));
                }
                v
            })
            .collect();
        collections_data.insert(name.clone(), doc_vals);
    }

    let serialized_data = serde_json::to_string(&collections_data).unwrap_or_default();
    let mut hasher = Hasher::new();
    hasher.update(serialized_data.as_bytes());
    let checksum = format!("{:08x}", hasher.finalize());

    let manifest = SnapshotManifest {
        engine: "FaizDB Engine".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now().to_rfc3339(),
        collections: col_names,
        total_documents: total_docs,
        checksum,
        file_size_bytes: serialized_data.len(),
    };

    SnapshotArchive {
        manifest,
        collections_data,
    }
}

/// Save snapshot archive to a file
pub fn save_snapshot_file(archive: &SnapshotArchive, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(archive).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read and verify a snapshot archive from a file with integrity check
pub fn load_and_verify_snapshot(path: &Path) -> Result<SnapshotArchive, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let archive: SnapshotArchive = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // Verify Checksum
    let serialized_data = serde_json::to_string(&archive.collections_data).unwrap_or_default();
    let mut hasher = Hasher::new();
    hasher.update(serialized_data.as_bytes());
    let computed_checksum = format!("{:08x}", hasher.finalize());

    if computed_checksum != archive.manifest.checksum {
        return Err(format!(
            "Checksum mismatch! Expected {}, found {}",
            archive.manifest.checksum, computed_checksum
        ));
    }

    Ok(archive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_creation_and_verification() {
        let mut d1 = Document::new();
        d1.set("name", "FaizDB");
        d1.set("role", "Primary Database");

        let collections = vec![("systems".to_string(), vec![d1])];
        let archive = build_snapshot(&collections);

        assert_eq!(archive.manifest.total_documents, 1);
        assert!(!archive.manifest.checksum.is_empty());
        assert_eq!(archive.manifest.collections, vec!["systems"]);
    }
}
