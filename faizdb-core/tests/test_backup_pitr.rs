//! Incremental Backup, Point-In-Time Recovery (PITR) & AES-256-GCM Integration Tests.

use std::collections::HashMap;
use chrono::Utc;
use tempfile::tempdir;

use faizdb_core::backup::{
    apply_incremental_snapshot, build_incremental_snapshot, build_snapshot, build_snapshot_with_lsn,
    decrypt_snapshot, encrypt_snapshot, load_and_verify_snapshot, save_snapshot_file,
    BackupType, PitrEngine, WalReplayRecord,
};
use faizdb_core::document::model::Document;

#[test]
fn test_full_and_incremental_backup_lifecycle() {
    let temp = tempdir().unwrap();
    let snapshot_file = temp.path().join("base_backup.json");

    // 1. Create base snapshot with 2 documents
    let mut d1 = Document::new();
    d1.set("title", "Doc 1");
    let id1 = d1.id.as_str().to_string();

    let mut d2 = Document::new();
    d2.set("title", "Doc 2");

    let base = build_snapshot_with_lsn(&[("articles".to_string(), vec![d1, d2])], 0, 50);
    assert_eq!(base.manifest.backup_type, BackupType::Full);
    assert_eq!(base.manifest.total_documents, 2);

    // Save and verify on disk
    save_snapshot_file(&base, &snapshot_file).unwrap();
    let loaded = load_and_verify_snapshot(&snapshot_file).unwrap();
    assert_eq!(loaded.manifest.checksum, base.manifest.checksum);

    // 2. Build incremental snapshot with 1 update and 1 new document
    let mut d1_updated = Document::new();
    d1_updated.id = faizdb_core::document::model::DocumentId::from_string(&id1);
    d1_updated.set("title", "Doc 1 - Revised");

    let mut d3 = Document::new();
    d3.set("title", "Doc 3 - New");

    let inc = build_incremental_snapshot(
        &base,
        &[("articles".to_string(), vec![d1_updated, d3])],
        HashMap::new(),
        100,
    );
    assert_eq!(inc.manifest.backup_type, BackupType::Incremental);
    assert_eq!(inc.manifest.start_lsn, 50);
    assert_eq!(inc.manifest.end_lsn, 100);

    // 3. Apply incremental snapshot onto base
    let restored = apply_incremental_snapshot(&base, &inc).unwrap();
    assert_eq!(restored.manifest.total_documents, 3);
    let docs = restored.collections_data.get("articles").unwrap();
    let doc1 = docs.iter().find(|d| d.get("_id").unwrap() == &id1).unwrap();
    assert_eq!(doc1.get("title").unwrap(), "Doc 1 - Revised");
}

#[test]
fn test_pitr_disaster_recovery_replay() {
    let mut doc = Document::new();
    doc.set("balance", 1000);
    let doc_id = doc.id.as_str().to_string();

    let t0 = Utc::now() - chrono::Duration::seconds(30);
    let base = build_snapshot_with_lsn(&[("accounts".to_string(), vec![doc])], 0, 10);

    let t1 = t0 + chrono::Duration::seconds(5);
    let t2 = t0 + chrono::Duration::seconds(10);
    let t_disaster = t0 + chrono::Duration::seconds(15);

    // WAL transactions:
    // tx 1 at t1: balance = 1500
    // tx 2 at t2: balance = 2000 (CORRECT DESIRED STATE)
    // tx 3 at t_disaster: accidental deletion!
    let wal = vec![
        WalReplayRecord {
            sequence: 11,
            timestamp: t1,
            op_type: 1,
            collection: "accounts".to_string(),
            doc_id: doc_id.clone(),
            payload: Some(serde_json::json!({ "balance": 1500 })),
        },
        WalReplayRecord {
            sequence: 12,
            timestamp: t2,
            op_type: 1,
            collection: "accounts".to_string(),
            doc_id: doc_id.clone(),
            payload: Some(serde_json::json!({ "balance": 2000 })),
        },
        WalReplayRecord {
            sequence: 13,
            timestamp: t_disaster,
            op_type: 2, // DELETE!
            collection: "accounts".to_string(),
            doc_id: doc_id.clone(),
            payload: None,
        },
    ];

    // PITR: Recover to t2, before the disaster!
    let recovered = PitrEngine::replay_to_timestamp(&base, &wal, t2).unwrap();
    assert_eq!(recovered.manifest.total_documents, 1);
    let accounts = recovered.collections_data.get("accounts").unwrap();
    let recovered_acc = accounts.iter().find(|d| d.get("_id").unwrap() == &doc_id).unwrap();
    assert_eq!(recovered_acc.get("balance").unwrap(), 2000);
}

#[test]
fn test_aes_gcm_passphrase_derivation_security() {
    let mut d = Document::new();
    d.set("confidential", "financial_records_2026");
    let archive = build_snapshot(&[("finance".to_string(), vec![d])]);

    let key = "Production-Passphrase-Complex!2026";
    let encrypted = encrypt_snapshot(&archive, key).unwrap();

    // Authenticated decryption succeeds
    let decrypted = decrypt_snapshot(&encrypted, key).unwrap();
    assert_eq!(decrypted.manifest.total_documents, 1);

    // Tampered ciphertext fails AEAD authentication
    let mut tampered = encrypted.clone();
    if let Some(byte) = tampered.ciphertext.get_mut(10) {
        *byte ^= 0xFF; // Flip bits
    }
    assert!(decrypt_snapshot(&tampered, key).is_err(), "Tampered ciphertext must fail AEAD decryption");
}
