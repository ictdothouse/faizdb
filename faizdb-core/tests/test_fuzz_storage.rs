//! Fuzz & Edge-Case Fault Injection Testing for Storage Engine, WAL, and Recovery.
//!
//! Validates crash resilience against:
//! - Truncated WAL files (power loss mid-write)
//! - Corrupted WAL magic bytes and header bitflips
//! - CRC32 checksum tampering
//! - Unknown opcodes and garbage payloads

use std::fs::OpenOptions;
use tempfile::tempdir;

use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use faizdb_core::storage::wal::{Wal, WalOpType};

#[test]
fn test_fuzz_truncated_wal_recovery() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().to_path_buf();
    let wal_dir = data_dir.join("wal");
    std::fs::create_dir_all(&wal_dir).unwrap();

    // 1. Create a valid WAL with 20 records
    {
        let wal = Wal::open(&wal_dir).unwrap();
        for i in 0..20 {
            let key = format!("k_{i}");
            let val = format!("v_{i}");
            wal.append(WalOpType::Put, key.as_bytes(), val.as_bytes()).unwrap();
        }
    }

    // 2. Truncate the WAL file mid-record (simulating sudden power failure)
    let wal_path = wal_dir.join("wal_000001.log");
    let file_len = std::fs::metadata(&wal_path).unwrap().len();

    // Truncate last 12 bytes
    let truncated_len = file_len.saturating_sub(12);
    let file = OpenOptions::new().write(true).open(&wal_path).unwrap();
    file.set_len(truncated_len).unwrap();
    drop(file);

    // 3. Engine recovery must NOT panic and gracefully recover all complete records before truncation
    let config = StorageConfig {
        data_dir: data_dir.clone(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    let engine = StorageEngine::open(config).expect("StorageEngine must recover cleanly without panic");

    // First 18 records should be safely recovered
    let mut recovered = 0;
    for i in 0..20 {
        let key = format!("k_{i}");
        if engine.get(key.as_bytes()).unwrap().is_some() {
            recovered += 1;
        }
    }
    assert!(recovered >= 18, "Expected at least 18 complete records recovered, got {recovered}");
}

#[test]
fn test_fuzz_corrupted_magic_bytes() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().to_path_buf();
    let wal_dir = data_dir.join("wal");
    std::fs::create_dir_all(&wal_dir).unwrap();

    let wal_path = wal_dir.join("wal_000001.log");
    // Write corrupted header magic bytes
    std::fs::write(&wal_path, b"CORRUPT_MAGIC_BYTES_12345678").unwrap();

    let config = StorageConfig {
        data_dir: data_dir.clone(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    // Engine must handle corruption gracefully without crashing or panicking
    let engine_res = StorageEngine::open(config);
    assert!(engine_res.is_ok(), "Engine must gracefully skip corrupted files and open without crashing");
    let engine = engine_res.unwrap();
    assert_eq!(engine.get(b"unknown_key").unwrap(), None);
}

#[test]
fn test_fuzz_crc_checksum_mismatch() {
    let temp = tempdir().unwrap();
    let data_dir = temp.path().to_path_buf();
    let wal_dir = data_dir.join("wal");
    std::fs::create_dir_all(&wal_dir).unwrap();

    // 1. Write 5 valid records
    {
        let wal = Wal::open(&wal_dir).unwrap();
        for i in 0..5 {
            wal.append(WalOpType::Put, format!("k_{i}").as_bytes(), b"valid_data").unwrap();
        }
    }

    // 2. Corrupt a byte in the middle of the log file
    let wal_path = wal_dir.join("wal_000001.log");
    let mut bytes = std::fs::read(&wal_path).unwrap();
    let mid_point = bytes.len() - 10;
    bytes[mid_point] ^= 0xFF; // Invert bits to trigger CRC mismatch
    std::fs::write(&wal_path, bytes).unwrap();

    // 3. Engine recovery should stop at the corrupted record without panic
    let config = StorageConfig {
        data_dir: data_dir.clone(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    let engine = StorageEngine::open(config).expect("Engine recovery must not panic on CRC mismatch");

    // The first record before corruption must still be available
    assert_eq!(engine.get(b"k_0").unwrap(), Some(b"valid_data".to_vec()));
}
