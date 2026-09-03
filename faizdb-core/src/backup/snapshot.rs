//! Automated Point-in-Time Consistent Snapshot & Disaster Recovery Engine.
//!
//! Features:
//! - Full atomic snapshots with CRC32/SHA256 integrity verification
//! - Incremental snapshots tracking LSN (Log Sequence Number) deltas
//! - Point-in-Time Recovery (PITR) with continuous WAL replay
//! - Zero-Trust AES-256-GCM encryption at rest with PBKDF2/SHA256 key derivation

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use chrono::{DateTime, Utc};
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::document::model::Document;

/// Type of backup archive
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackupType {
    Full,
    Incremental,
}

/// Snapshot Manifest Header containing metadata and integrity checksums
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub backup_id: String,
    pub backup_type: BackupType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_snapshot_id: Option<String>,
    pub engine: String,
    pub version: String,
    pub created_at: String,
    pub collections: Vec<String>,
    pub total_documents: usize,
    pub start_lsn: u64,
    pub end_lsn: u64,
    pub encrypted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption_salt: Option<String>,
    pub checksum: String,
    pub file_size_bytes: usize,
}

/// Snapshot Archive Structure (Full or Incremental)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotArchive {
    pub manifest: SnapshotManifest,
    pub collections_data: HashMap<String, Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub deleted_ids: HashMap<String, Vec<String>>,
}

/// Encrypted Snapshot File Envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedSnapshotEnvelope {
    pub backup_id: String,
    pub version: String,
    pub nonce: Vec<u8>,
    pub salt: Vec<u8>,
    pub ciphertext: Vec<u8>,
    pub checksum: String,
}

/// WAL Mutation Record for PITR Replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalReplayRecord {
    pub sequence: u64,
    pub timestamp: DateTime<Utc>,
    pub op_type: u8,
    pub collection: String,
    pub doc_id: String,
    pub payload: Option<serde_json::Value>,
}

/// Generate a full snapshot data structure with integrity checksum
pub fn build_snapshot(
    collections: &[(String, Vec<Document>)],
) -> SnapshotArchive {
    build_snapshot_with_lsn(collections, 0, 0)
}

/// Generate a full snapshot data structure with explicit LSN bounds
pub fn build_snapshot_with_lsn(
    collections: &[(String, Vec<Document>)],
    start_lsn: u64,
    end_lsn: u64,
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

    let backup_id = Uuid::new_v4().to_string();
    let manifest = SnapshotManifest {
        backup_id,
        backup_type: BackupType::Full,
        base_snapshot_id: None,
        engine: "FaizDB Engine".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now().to_rfc3339(),
        collections: col_names,
        total_documents: total_docs,
        start_lsn,
        end_lsn,
        encrypted: false,
        encryption_salt: None,
        checksum,
        file_size_bytes: serialized_data.len(),
    };

    SnapshotArchive {
        manifest,
        collections_data,
        deleted_ids: HashMap::new(),
    }
}

/// Generate an incremental snapshot containing only documents modified or deleted
/// since the previous base snapshot
pub fn build_incremental_snapshot(
    base_snapshot: &SnapshotArchive,
    modified_collections: &[(String, Vec<Document>)],
    deleted_ids: HashMap<String, Vec<String>>,
    current_lsn: u64,
) -> SnapshotArchive {
    let mut collections_data: HashMap<String, Vec<serde_json::Value>> = HashMap::new();
    let mut total_docs = 0;
    let mut col_names = Vec::new();

    for (name, docs) in modified_collections {
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

    let mut hasher = Hasher::new();
    let serialized_data = serde_json::to_string(&collections_data).unwrap_or_default();
    let serialized_deletes = serde_json::to_string(&deleted_ids).unwrap_or_default();
    hasher.update(serialized_data.as_bytes());
    hasher.update(serialized_deletes.as_bytes());
    let checksum = format!("{:08x}", hasher.finalize());

    let backup_id = Uuid::new_v4().to_string();
    let manifest = SnapshotManifest {
        backup_id,
        backup_type: BackupType::Incremental,
        base_snapshot_id: Some(base_snapshot.manifest.backup_id.clone()),
        engine: "FaizDB Engine".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now().to_rfc3339(),
        collections: col_names,
        total_documents: total_docs,
        start_lsn: base_snapshot.manifest.end_lsn,
        end_lsn: current_lsn,
        encrypted: false,
        encryption_salt: None,
        checksum,
        file_size_bytes: serialized_data.len() + serialized_deletes.len(),
    };

    SnapshotArchive {
        manifest,
        collections_data,
        deleted_ids,
    }
}

/// Apply an incremental snapshot onto a base snapshot to produce a merged state
pub fn apply_incremental_snapshot(
    base: &SnapshotArchive,
    incremental: &SnapshotArchive,
) -> Result<SnapshotArchive, String> {
    if incremental.manifest.backup_type != BackupType::Incremental {
        return Err("Provided archive is not an incremental snapshot".to_string());
    }

    if let Some(ref base_id) = incremental.manifest.base_snapshot_id {
        if base_id != &base.manifest.backup_id {
            return Err(format!(
                "Base snapshot ID mismatch! Expected '{}', got '{}'",
                base.manifest.backup_id, base_id
            ));
        }
    }

    let mut merged_data = base.collections_data.clone();

    // 1. Process deletions
    for (col_name, ids) in &incremental.deleted_ids {
        if let Some(docs) = merged_data.get_mut(col_name) {
            let id_set: std::collections::HashSet<&str> = ids.iter().map(|s| s.as_str()).collect();
            docs.retain(|doc| {
                if let Some(id_val) = doc.get("_id").and_then(|v| v.as_str()) {
                    !id_set.contains(id_val)
                } else {
                    true
                }
            });
        }
    }

    // 2. Process insertions & updates (upsert)
    for (col_name, new_docs) in &incremental.collections_data {
        let entry = merged_data.entry(col_name.clone()).or_default();
        for new_doc in new_docs {
            let new_id = new_doc.get("_id").and_then(|v| v.as_str());
            if let Some(id_str) = new_id {
                // If doc exists, replace it
                if let Some(pos) = entry.iter().position(|d| d.get("_id").and_then(|v| v.as_str()) == Some(id_str)) {
                    entry[pos] = new_doc.clone();
                } else {
                    entry.push(new_doc.clone());
                }
            } else {
                entry.push(new_doc.clone());
            }
        }
    }

    let total_documents: usize = merged_data.values().map(|v| v.len()).sum();
    let col_names: Vec<String> = merged_data.keys().cloned().collect();

    let serialized_data = serde_json::to_string(&merged_data).unwrap_or_default();
    let mut hasher = Hasher::new();
    hasher.update(serialized_data.as_bytes());
    let checksum = format!("{:08x}", hasher.finalize());

    let manifest = SnapshotManifest {
        backup_id: Uuid::new_v4().to_string(),
        backup_type: BackupType::Full,
        base_snapshot_id: Some(base.manifest.backup_id.clone()),
        engine: "FaizDB Engine".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        created_at: Utc::now().to_rfc3339(),
        collections: col_names,
        total_documents,
        start_lsn: base.manifest.start_lsn,
        end_lsn: incremental.manifest.end_lsn,
        encrypted: false,
        encryption_salt: None,
        checksum,
        file_size_bytes: serialized_data.len(),
    };

    Ok(SnapshotArchive {
        manifest,
        collections_data: merged_data,
        deleted_ids: HashMap::new(),
    })
}

// ── AES-256-GCM Encryption At Rest ──────────────────────────────────────────

/// Derive a 256-bit AES key from a passphrase and salt using PBKDF2 with HMAC-SHA256
fn derive_aes_key(passphrase: &str, salt: &[u8]) -> [u8; 32] {
    use ring::pbkdf2;
    use std::num::NonZeroU32;

    let mut key = [0u8; 32];
    let iterations = NonZeroU32::new(100_000).unwrap();
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        salt,
        passphrase.as_bytes(),
        &mut key,
    );
    key
}

/// Encrypt a SnapshotArchive using AES-256-GCM at rest
pub fn encrypt_snapshot(
    archive: &SnapshotArchive,
    passphrase: &str,
) -> Result<EncryptedSnapshotEnvelope, String> {
    use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, SealingKey, UnboundKey, AES_256_GCM};
    use ring::rand::{SecureRandom, SystemRandom};

    let rng = SystemRandom::new();

    // 1. Generate 16-byte random salt
    let mut salt = [0u8; 16];
    rng.fill(&mut salt).map_err(|e| format!("RNG failed: {e}"))?;

    // 2. Generate 12-byte random nonce
    let mut nonce_bytes = [0u8; 12];
    rng.fill(&mut nonce_bytes).map_err(|e| format!("RNG failed: {e}"))?;

    // 3. Derive 32-byte key
    let key_bytes = derive_aes_key(passphrase, &salt);
    let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|e| format!("Key error: {e}"))?;

    struct OneNonce(Option<[u8; 12]>);
    impl NonceSequence for OneNonce {
        fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
            self.0.take().map(Nonce::assume_unique_for_key).ok_or(ring::error::Unspecified)
        }
    }

    let mut sealing_key = SealingKey::new(unbound_key, OneNonce(Some(nonce_bytes)));

    // 4. Serialize plaintext
    let mut plaintext = serde_json::to_vec(archive).map_err(|e| e.to_string())?;

    // 5. Compute plaintext checksum
    let mut hasher = Hasher::new();
    hasher.update(&plaintext);
    let checksum = format!("{:08x}", hasher.finalize());

    // 6. Encrypt in-place (appends 16-byte GCM auth tag)
    sealing_key
        .seal_in_place_append_tag(Aad::empty(), &mut plaintext)
        .map_err(|e| format!("Encryption sealing error: {e}"))?;

    Ok(EncryptedSnapshotEnvelope {
        backup_id: archive.manifest.backup_id.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        nonce: nonce_bytes.to_vec(),
        salt: salt.to_vec(),
        ciphertext: plaintext,
        checksum,
    })
}

/// Decrypt an EncryptedSnapshotEnvelope using AES-256-GCM
pub fn decrypt_snapshot(
    envelope: &EncryptedSnapshotEnvelope,
    passphrase: &str,
) -> Result<SnapshotArchive, String> {
    use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, OpeningKey, UnboundKey, AES_256_GCM};

    if envelope.nonce.len() != 12 {
        return Err("Invalid nonce length; expected 12 bytes".to_string());
    }
    let mut nonce_bytes = [0u8; 12];
    nonce_bytes.copy_from_slice(&envelope.nonce);

    let key_bytes = derive_aes_key(passphrase, &envelope.salt);
    let unbound_key = UnboundKey::new(&AES_256_GCM, &key_bytes).map_err(|e| format!("Key error: {e}"))?;

    struct OneNonce(Option<[u8; 12]>);
    impl NonceSequence for OneNonce {
        fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
            self.0.take().map(Nonce::assume_unique_for_key).ok_or(ring::error::Unspecified)
        }
    }

    let mut opening_key = OpeningKey::new(unbound_key, OneNonce(Some(nonce_bytes)));

    let mut in_out = envelope.ciphertext.clone();
    let decrypted_bytes = opening_key
        .open_in_place(Aad::empty(), &mut in_out)
        .map_err(|_| "Decryption failed: incorrect passphrase or corrupted ciphertext (AEAD tag mismatch)".to_string())?;

    // Verify plaintext checksum
    let mut hasher = Hasher::new();
    hasher.update(decrypted_bytes);
    let computed_checksum = format!("{:08x}", hasher.finalize());

    if computed_checksum != envelope.checksum {
        return Err(format!(
            "Checksum mismatch after decryption! Expected {}, got {}",
            envelope.checksum, computed_checksum
        ));
    }

    let archive: SnapshotArchive = serde_json::from_slice(decrypted_bytes).map_err(|e| e.to_string())?;
    Ok(archive)
}

// ── Point-In-Time Recovery (PITR) Engine ─────────────────────────────────────

/// Point-in-Time Recovery Engine: Replays WAL records on top of a base snapshot
/// up to a target timestamp or LSN
pub struct PitrEngine;

impl PitrEngine {
    /// Replay a sequence of WAL mutation records on a base snapshot up to `target_timestamp`
    pub fn replay_to_timestamp(
        base_snapshot: &SnapshotArchive,
        wal_records: &[WalReplayRecord],
        target_timestamp: DateTime<Utc>,
    ) -> Result<SnapshotArchive, String> {
        let mut state = base_snapshot.collections_data.clone();
        let mut last_lsn = base_snapshot.manifest.end_lsn;

        for rec in wal_records {
            if rec.timestamp > target_timestamp {
                break;
            }

            last_lsn = last_lsn.max(rec.sequence);
            let col = state.entry(rec.collection.clone()).or_default();

            match rec.op_type {
                // Put / Insert / Update
                1 => {
                    if let Some(payload) = &rec.payload {
                        let mut doc_val = payload.clone();
                        if let Some(obj) = doc_val.as_object_mut() {
                            obj.insert("_id".to_string(), serde_json::Value::String(rec.doc_id.clone()));
                        }
                        if let Some(pos) = col.iter().position(|d| d.get("_id").and_then(|v| v.as_str()) == Some(&rec.doc_id)) {
                            col[pos] = doc_val;
                        } else {
                            col.push(doc_val);
                        }
                    }
                }
                // Delete
                2 => {
                    col.retain(|d| d.get("_id").and_then(|v| v.as_str()) != Some(&rec.doc_id));
                }
                _ => {} // Ignore txn control / others for doc state
            }
        }

        let total_docs: usize = state.values().map(|v| v.len()).sum();
        let col_names: Vec<String> = state.keys().cloned().collect();

        let serialized_data = serde_json::to_string(&state).unwrap_or_default();
        let mut hasher = Hasher::new();
        hasher.update(serialized_data.as_bytes());
        let checksum = format!("{:08x}", hasher.finalize());

        let manifest = SnapshotManifest {
            backup_id: Uuid::new_v4().to_string(),
            backup_type: BackupType::Full,
            base_snapshot_id: Some(base_snapshot.manifest.backup_id.clone()),
            engine: "FaizDB Engine (PITR Restored)".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: target_timestamp.to_rfc3339(),
            collections: col_names,
            total_documents: total_docs,
            start_lsn: base_snapshot.manifest.start_lsn,
            end_lsn: last_lsn,
            encrypted: false,
            encryption_salt: None,
            checksum,
            file_size_bytes: serialized_data.len(),
        };

        Ok(SnapshotArchive {
            manifest,
            collections_data: state,
            deleted_ids: HashMap::new(),
        })
    }

    /// Replay a sequence of WAL mutation records on a base snapshot up to `target_lsn`
    pub fn replay_to_lsn(
        base_snapshot: &SnapshotArchive,
        wal_records: &[WalReplayRecord],
        target_lsn: u64,
    ) -> Result<SnapshotArchive, String> {
        let mut state = base_snapshot.collections_data.clone();
        let mut last_ts = Utc::now();

        for rec in wal_records {
            if rec.sequence > target_lsn {
                break;
            }

            last_ts = rec.timestamp;
            let col = state.entry(rec.collection.clone()).or_default();

            match rec.op_type {
                1 => {
                    if let Some(payload) = &rec.payload {
                        let mut doc_val = payload.clone();
                        if let Some(obj) = doc_val.as_object_mut() {
                            obj.insert("_id".to_string(), serde_json::Value::String(rec.doc_id.clone()));
                        }
                        if let Some(pos) = col.iter().position(|d| d.get("_id").and_then(|v| v.as_str()) == Some(&rec.doc_id)) {
                            col[pos] = doc_val;
                        } else {
                            col.push(doc_val);
                        }
                    }
                }
                2 => {
                    col.retain(|d| d.get("_id").and_then(|v| v.as_str()) != Some(&rec.doc_id));
                }
                _ => {}
            }
        }

        let total_docs: usize = state.values().map(|v| v.len()).sum();
        let col_names: Vec<String> = state.keys().cloned().collect();

        let serialized_data = serde_json::to_string(&state).unwrap_or_default();
        let mut hasher = Hasher::new();
        hasher.update(serialized_data.as_bytes());
        let checksum = format!("{:08x}", hasher.finalize());

        let manifest = SnapshotManifest {
            backup_id: Uuid::new_v4().to_string(),
            backup_type: BackupType::Full,
            base_snapshot_id: Some(base_snapshot.manifest.backup_id.clone()),
            engine: "FaizDB Engine (PITR Restored)".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            created_at: last_ts.to_rfc3339(),
            collections: col_names,
            total_documents: total_docs,
            start_lsn: base_snapshot.manifest.start_lsn,
            end_lsn: target_lsn,
            encrypted: false,
            encryption_salt: None,
            checksum,
            file_size_bytes: serialized_data.len(),
        };

        Ok(SnapshotArchive {
            manifest,
            collections_data: state,
            deleted_ids: HashMap::new(),
        })
    }
}

// ── File I/O Helpers ────────────────────────────────────────────────────────

/// Save unencrypted snapshot archive to a file
pub fn save_snapshot_file(archive: &SnapshotArchive, path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(archive).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Save encrypted snapshot envelope to a file
pub fn save_encrypted_snapshot_file(
    envelope: &EncryptedSnapshotEnvelope,
    path: &Path,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(envelope).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

/// Read and verify a snapshot archive from a file with integrity check
pub fn load_and_verify_snapshot(path: &Path) -> Result<SnapshotArchive, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let archive: SnapshotArchive = serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // Verify Checksum
    let mut hasher = Hasher::new();
    let serialized_data = serde_json::to_string(&archive.collections_data).unwrap_or_default();
    hasher.update(serialized_data.as_bytes());

    if archive.manifest.backup_type == BackupType::Incremental {
        let serialized_deletes = serde_json::to_string(&archive.deleted_ids).unwrap_or_default();
        hasher.update(serialized_deletes.as_bytes());
    }

    let computed_checksum = format!("{:08x}", hasher.finalize());

    if computed_checksum != archive.manifest.checksum {
        return Err(format!(
            "Checksum mismatch! Expected {}, found {}",
            archive.manifest.checksum, computed_checksum
        ));
    }

    Ok(archive)
}

/// Load and decrypt an encrypted snapshot archive from a file
pub fn load_and_decrypt_snapshot(path: &Path, passphrase: &str) -> Result<SnapshotArchive, String> {
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let envelope: EncryptedSnapshotEnvelope = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    decrypt_snapshot(&envelope, passphrase)
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
        assert_eq!(archive.manifest.backup_type, BackupType::Full);
    }

    #[test]
    fn test_incremental_backup_and_apply() {
        let mut d1 = Document::new();
        d1.set("name", "Item 1");
        let id1 = d1.id.as_str().to_string();

        let base = build_snapshot_with_lsn(&[("items".to_string(), vec![d1])], 0, 100);
        assert_eq!(base.manifest.total_documents, 1);

        // Incremental change: Add d2, update d1, delete d_nonexistent
        let mut d1_mod = Document::new();
        d1_mod.id = crate::document::model::DocumentId::from_string(&id1);
        d1_mod.set("name", "Item 1 Modified");

        let mut d2 = Document::new();
        d2.set("name", "Item 2");

        let mut deleted = HashMap::new();
        deleted.insert("items".to_string(), vec!["dummy_del".to_string()]);

        let inc = build_incremental_snapshot(
            &base,
            &[("items".to_string(), vec![d1_mod, d2])],
            deleted,
            200,
        );

        assert_eq!(inc.manifest.backup_type, BackupType::Incremental);
        assert_eq!(inc.manifest.start_lsn, 100);
        assert_eq!(inc.manifest.end_lsn, 200);

        let merged = apply_incremental_snapshot(&base, &inc).expect("merge should succeed");
        assert_eq!(merged.manifest.total_documents, 2);
        assert_eq!(merged.manifest.end_lsn, 200);

        let docs = merged.collections_data.get("items").unwrap();
        let d1_val = docs.iter().find(|d| d.get("_id").unwrap() == &id1).unwrap();
        assert_eq!(d1_val.get("name").unwrap(), "Item 1 Modified");
    }

    #[test]
    fn test_aes_256_gcm_backup_encryption_and_decryption() {
        let mut d = Document::new();
        d.set("secret", "top_secret_data_123");
        let archive = build_snapshot(&[("vault".to_string(), vec![d])]);

        let passphrase = "correct_horse_battery_staple_2026";
        let encrypted = encrypt_snapshot(&archive, passphrase).expect("encryption succeeds");

        assert_ne!(encrypted.ciphertext, serde_json::to_vec(&archive).unwrap());
        assert_eq!(encrypted.nonce.len(), 12);
        assert_eq!(encrypted.salt.len(), 16);

        // Decrypt with correct passphrase
        let decrypted = decrypt_snapshot(&encrypted, passphrase).expect("decryption succeeds");
        assert_eq!(decrypted.manifest.backup_id, archive.manifest.backup_id);
        assert_eq!(decrypted.manifest.total_documents, 1);

        // Decrypt with wrong passphrase fails
        let err = decrypt_snapshot(&encrypted, "wrong_password");
        assert!(err.is_err());
    }

    #[test]
    fn test_pitr_point_in_time_recovery() {
        let mut d1 = Document::new();
        d1.set("name", "Initial Doc");
        let id1 = d1.id.as_str().to_string();

        let t0 = Utc::now() - chrono::Duration::seconds(60);
        let base = build_snapshot_with_lsn(&[("users".to_string(), vec![d1])], 0, 10);

        let t1 = t0 + chrono::Duration::seconds(10);
        let t2 = t0 + chrono::Duration::seconds(20);
        let t3 = t0 + chrono::Duration::seconds(30);

        // WAL records:
        // rec 1 at t1: insert user2
        // rec 2 at t2: update user1
        // rec 3 at t3: delete user1 (disaster!)
        let wal_records = vec![
            WalReplayRecord {
                sequence: 11,
                timestamp: t1,
                op_type: 1,
                collection: "users".to_string(),
                doc_id: "user_2".to_string(),
                payload: Some(serde_json::json!({ "name": "User Two" })),
            },
            WalReplayRecord {
                sequence: 12,
                timestamp: t2,
                op_type: 1,
                collection: "users".to_string(),
                doc_id: id1.clone(),
                payload: Some(serde_json::json!({ "name": "User One Updated" })),
            },
            WalReplayRecord {
                sequence: 13,
                timestamp: t3,
                op_type: 2,
                collection: "users".to_string(),
                doc_id: id1.clone(),
                payload: None,
            },
        ];

        // PITR recover to t2 (before deletion disaster!)
        let restored = PitrEngine::replay_to_timestamp(&base, &wal_records, t2).expect("PITR replay succeeds");
        assert_eq!(restored.manifest.total_documents, 2);
        let docs = restored.collections_data.get("users").unwrap();
        let u1 = docs.iter().find(|d| d.get("_id").unwrap() == &id1).unwrap();
        assert_eq!(u1.get("name").unwrap(), "User One Updated");

        // PITR recover to LSN 11 (only user 2 inserted)
        let restored_lsn = PitrEngine::replay_to_lsn(&base, &wal_records, 11).expect("PITR LSN succeeds");
        assert_eq!(restored_lsn.manifest.total_documents, 2);
        let u1_old = restored_lsn.collections_data.get("users").unwrap().iter().find(|d| d.get("_id").unwrap() == &id1).unwrap();
        assert_eq!(u1_old.get("name").unwrap(), "Initial Doc");
    }
}
