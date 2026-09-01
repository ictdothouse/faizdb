//! Integration tests for FaizDB document CRUD operations.
//!
//! Tests the storage engine, document layer, and collection operations
//! using an in-memory + temporary directory configuration.

use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use std::path::Path;

fn open_test_engine(dir: &Path) -> StorageEngine {
    StorageEngine::open(StorageConfig {
        data_dir: dir.to_path_buf(),
        memtable_size: 65536, // 64KB — triggers flushes quickly for testing
        sync_writes: false,
        enable_wal: true,
    })
    .expect("Failed to open storage engine")
}

#[test]
fn test_basic_put_get_delete() {
    let dir = tempfile::tempdir().unwrap();
    let engine = open_test_engine(dir.path());

    engine.put(b"users:1", b"{\"name\":\"Faiz\"}").unwrap();
    engine.put(b"users:2", b"{\"name\":\"Ali\"}").unwrap();

    assert_eq!(engine.get(b"users:1").unwrap(), Some(b"{\"name\":\"Faiz\"}".to_vec()));
    assert_eq!(engine.get(b"users:2").unwrap(), Some(b"{\"name\":\"Ali\"}".to_vec()));
    assert_eq!(engine.get(b"users:99").unwrap(), None);

    engine.delete(b"users:1").unwrap();
    assert_eq!(engine.get(b"users:1").unwrap(), None);
    // users:2 must still be accessible
    assert_eq!(engine.get(b"users:2").unwrap(), Some(b"{\"name\":\"Ali\"}".to_vec()));
}

#[test]
fn test_overwrite_returns_latest() {
    let dir = tempfile::tempdir().unwrap();
    let engine = open_test_engine(dir.path());

    engine.put(b"config:mode", b"debug").unwrap();
    engine.put(b"config:mode", b"release").unwrap();

    assert_eq!(engine.get(b"config:mode").unwrap(), Some(b"release".to_vec()));
}

#[test]
fn test_prefix_scan_isolation() {
    let dir = tempfile::tempdir().unwrap();
    let engine = open_test_engine(dir.path());

    for i in 0..5u32 {
        engine.put(format!("users:{i}").as_bytes(), format!("user_{i}").as_bytes()).unwrap();
        engine.put(format!("orders:{i}").as_bytes(), format!("order_{i}").as_bytes()).unwrap();
    }

    let users = engine.prefix_scan(b"users:").unwrap();
    let orders = engine.prefix_scan(b"orders:").unwrap();

    assert_eq!(users.len(), 5, "Must return exactly 5 user entries");
    assert_eq!(orders.len(), 5, "Must return exactly 5 order entries");

    // Ensure no cross-contamination between prefixes
    for (key, _) in &users {
        assert!(key.starts_with(b"users:"), "User scan returned non-user key");
    }
}

#[test]
fn test_crash_recovery_via_wal() {
    let dir = tempfile::tempdir().unwrap();

    // Write data WITHOUT flushing to SSTable — simulates crash after WAL write
    {
        let engine = open_test_engine(dir.path());
        engine.put(b"durable:key", b"durable_value").unwrap();
        // Drop without flush — WAL must preserve this
    }

    // Re-open and verify WAL replay
    {
        let engine = open_test_engine(dir.path());
        assert_eq!(
            engine.get(b"durable:key").unwrap(),
            Some(b"durable_value".to_vec()),
            "WAL replay must recover data written before crash"
        );
    }
}

#[test]
fn test_high_volume_stress() {
    let dir = tempfile::tempdir().unwrap();
    let engine = open_test_engine(dir.path());
    let n = 1000u32;

    // Insert n entries — triggers multiple MemTable flushes to SSTables
    for i in 0..n {
        let key = format!("stress:{i:06}");
        let val = format!("value_{i}_payload_data_bytes_padded_to_simulate_real_workload");
        engine.put(key.as_bytes(), val.as_bytes()).unwrap();
    }

    // Verify all entries are readable
    let mut misses = 0u32;
    for i in 0..n {
        let key = format!("stress:{i:06}");
        let expected = format!("value_{i}_payload_data_bytes_padded_to_simulate_real_workload");
        if engine.get(key.as_bytes()).unwrap() != Some(expected.into_bytes()) {
            misses += 1;
        }
    }

    assert_eq!(misses, 0, "All {n} entries must survive MemTable flushes");

    // Verify stats are sane
    let stats = engine.stats();
    assert!(
        stats.sstable_count > 0 || stats.memtable_entries > 0,
        "Data must exist in storage layer"
    );
}
