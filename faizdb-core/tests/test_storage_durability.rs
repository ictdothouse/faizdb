//! Comprehensive Durability, WAL Crash Recovery & LSM-Tree Compaction Integration Tests.

use std::sync::Arc;
use tempfile::tempdir;

use faizdb_core::document::collection::Collection;
use faizdb_core::document::model::Document;
use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use faizdb_core::storage::sstable::{SSTableReader, SSTableWriter};

#[test]
fn test_wal_crash_recovery_durability() {
    let temp_dir = tempdir().unwrap();
    let data_dir = temp_dir.path().to_path_buf();

    // 1. First session: Write 100 entries to storage engine and crash (exit without compaction)
    {
        let config = StorageConfig {
            data_dir: data_dir.clone(),
            sync_writes: true,
            enable_wal: true,
            ..Default::default()
        };
        let engine = StorageEngine::open(config).expect("StorageEngine open succeeds");

        for i in 0..100 {
            let key = format!("user:{i:04}");
            let val = format!("{{\"name\":\"User_{i}\",\"score\":{i}}}");
            engine
                .put(key.as_bytes(), val.as_bytes())
                .expect("Put succeeds");
        }
        // Simulating crash: engine drops without manual flush or compaction
    }

    // 2. Second session: Re-open engine on same data_dir, must recover all 100 entries from WAL
    {
        let config = StorageConfig {
            data_dir: data_dir.clone(),
            sync_writes: true,
            enable_wal: true,
            ..Default::default()
        };
        let engine = StorageEngine::open(config).expect("StorageEngine re-open succeeds");

        for i in 0..100 {
            let key = format!("user:{i:04}");
            let res = engine.get(key.as_bytes()).expect("Get succeeds");
            assert!(res.is_some(), "Key {key} must be recovered from WAL");
            let expected_val = format!("{{\"name\":\"User_{i}\",\"score\":{i}}}");
            assert_eq!(res.unwrap(), expected_val.as_bytes());
        }

        // Test non-existent key
        assert_eq!(engine.get(b"user:9999").unwrap(), None);
    }
}

#[test]
fn test_sstable_bloom_filter_guarantee() {
    let temp_dir = tempdir().unwrap();
    let sstable_path = temp_dir.path().join("level0_001.sst");

    // Write SSTable with 500 keys
    {
        let mut writer = SSTableWriter::new(&sstable_path, 500).expect("Create SSTable writer");
        for i in 0..500 {
            let key = format!("key_{i:04}");
            let val = format!("val_{i:04}");
            writer
                .write_entry(
                    key.as_bytes(),
                    &faizdb_core::storage::memtable::MemEntry::Value(val.as_bytes().to_vec()),
                )
                .expect("Add to SSTable");
        }
        writer.finish().expect("Finish SSTable");
    }

    // Read and verify Bloom filter guarantees NO false negatives
    let reader = SSTableReader::open(&sstable_path).expect("Open SSTable reader");

    for i in 0..500 {
        let key = format!("key_{i:04}");
        assert!(
            reader.may_contain(key.as_bytes()),
            "Bloom filter must never produce false negative"
        );
        let val = reader.get(key.as_bytes()).expect("Get succeeds");
        assert!(val.is_some());
        assert_eq!(
            val.unwrap().as_value().unwrap(),
            format!("val_{i:04}").as_bytes()
        );
    }

    // Verify key definitely not present
    let absent_key = b"key_definitely_absent_9999";
    let val = reader.get(absent_key).expect("Get succeeds");
    assert_eq!(val, None);
}

#[test]
fn test_collection_persistence_with_storage_engine() {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        data_dir: temp_dir.path().to_path_buf(),
        ..Default::default()
    };
    let storage = Arc::new(StorageEngine::open(config).unwrap());
    let col = Collection::with_storage("customers", storage.clone());

    let mut d = Document::new();
    d.set("name", "Alice");
    d.set("email", "alice@example.com");
    let doc_id = col.insert(d).expect("Insert succeeds");

    // Verify retrieval through collection
    let retrieved = col.find_by_id(doc_id.as_str());
    assert!(retrieved.is_ok());
    assert_eq!(
        retrieved.unwrap().get("name").unwrap().as_str().unwrap(),
        "Alice"
    );

    // Verify raw key in storage engine starts with doc:customers:
    let expected_key = format!("doc:customers:{}", doc_id.as_str());
    let raw = storage.get(expected_key.as_bytes()).unwrap();
    assert!(
        raw.is_some(),
        "Document must be stored under doc prefix in storage engine"
    );
}
