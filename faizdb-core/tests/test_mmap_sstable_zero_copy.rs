//! Comprehensive integration tests for FaizDB Phase 2: Memory-Mapped (Mmap) Zero-Copy Storage Engine.
//!
//! Validates:
//! 1. Zero-syscall point lookups and tombstone handling over mmap slices.
//! 2. Zero-copy borrowed byte slice lookups (`get_ref`).
//! 3. High-concurrency lock-free multi-threaded reading over cloned mmap readers.
//! 4. Compaction lifecycle and clean file descriptor cleanup.

use std::sync::Arc;
use std::thread;

use faizdb_core::storage::compaction::merge_sstables;
use faizdb_core::storage::memtable::MemEntry;
use faizdb_core::storage::sstable::{SSTableReader, SSTableWriter};

#[test]
fn test_mmap_sstable_point_lookup() {
    let dir = tempfile::tempdir().expect("Failed to create tempdir");
    let path = dir.path().join("mmap_lookup.sst");

    let count = 500;
    {
        let mut writer = SSTableWriter::new(&path, count).expect("Failed to create writer");
        for i in 0..count {
            let key = format!("user:{i:05}");
            if i % 10 == 0 {
                // Tombstone
                writer
                    .write_entry(key.as_bytes(), &MemEntry::Tombstone)
                    .expect("Failed to write tombstone");
            } else {
                let val = format!("payload_data_content_{i}");
                writer
                    .write_entry(key.as_bytes(), &MemEntry::Value(val.into_bytes()))
                    .expect("Failed to write value");
            }
        }
        writer.finish().expect("Failed to finalize SSTable");
    }

    // Open via memory-mapped reader
    let reader = SSTableReader::open(&path).expect("Failed to open mmap SSTable");
    assert_eq!(reader.entry_count(), count as u64);
    assert!(reader.mmap().len() > 0);

    // Verify values
    for i in 0..count {
        let key = format!("user:{i:05}");
        let entry = reader
            .get(key.as_bytes())
            .expect("Lookup error")
            .expect("Entry must exist");
        if i % 10 == 0 {
            assert!(entry.is_tombstone(), "Key {key} must be a tombstone");
        } else {
            let expected = format!("payload_data_content_{i}");
            assert_eq!(
                entry.as_value().unwrap(),
                expected.as_bytes(),
                "Value mismatch for key {key}"
            );
        }
    }

    // Negative lookup (bloom filter fast path)
    let non_existent = reader
        .get(b"user:9999999_nonexistent")
        .expect("Lookup error");
    assert!(non_existent.is_none());
}

#[test]
fn test_mmap_zero_copy_borrowed_slice() {
    let dir = tempfile::tempdir().expect("Failed to create tempdir");
    let path = dir.path().join("mmap_zero_copy.sst");

    {
        let mut writer = SSTableWriter::new(&path, 3).expect("Failed to create writer");
        writer
            .write_entry(b"doc:001", &MemEntry::Value(b"first_document_payload".to_vec()))
            .expect("write entry");
        writer
            .write_entry(b"doc:002", &MemEntry::Tombstone)
            .expect("write entry");
        writer
            .write_entry(b"doc:003", &MemEntry::Value(b"third_document_payload".to_vec()))
            .expect("write entry");
        writer.finish().expect("finish");
    }

    let reader = SSTableReader::open(&path).expect("open mmap");

    // Zero-copy borrow for active document
    let slice1 = reader.get_ref(b"doc:001").expect("get_ref").expect("found");
    assert_eq!(slice1, Some(b"first_document_payload".as_slice()));

    // Zero-copy borrow for tombstone
    let slice2 = reader.get_ref(b"doc:002").expect("get_ref").expect("found");
    assert_eq!(slice2, None); // Tombstone marker

    // Zero-copy borrow for third document
    let slice3 = reader.get_ref(b"doc:003").expect("get_ref").expect("found");
    assert_eq!(slice3, Some(b"third_document_payload".as_slice()));

    // Non-existent key
    let missing = reader.get_ref(b"doc:missing").expect("get_ref");
    assert!(missing.is_none());
}

#[test]
fn test_mmap_concurrent_readers() {
    let dir = tempfile::tempdir().expect("Failed to create tempdir");
    let path = dir.path().join("mmap_concurrent.sst");

    let count = 1000;
    {
        let mut writer = SSTableWriter::new(&path, count).expect("Failed to create writer");
        for i in 0..count {
            let key = format!("entity:{i:04}");
            let val = format!("val_{i}");
            writer
                .write_entry(key.as_bytes(), &MemEntry::Value(val.into_bytes()))
                .expect("write");
        }
        writer.finish().expect("finish");
    }

    let reader = Arc::new(SSTableReader::open(&path).expect("open mmap"));

    // Spawn 20 concurrent reader threads accessing the shared mmap slice simultaneously
    let mut handles = Vec::new();
    for thread_id in 0..20 {
        let r = Arc::clone(&reader);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let idx = (thread_id * 50 + i) % 1000;
                let key = format!("entity:{idx:04}");
                let entry = r.get(key.as_bytes()).expect("lookup").expect("found");
                let expected = format!("val_{idx}");
                assert_eq!(entry.as_value().unwrap(), expected.as_bytes());
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join failed");
    }
}

#[test]
fn test_mmap_compaction_and_file_deletion() {
    let dir = tempfile::tempdir().expect("Failed to create tempdir");

    let mut input_paths = Vec::new();
    for table_idx in 0..3 {
        let p = dir.path().join(format!("input_{table_idx}.sst"));
        let mut writer = SSTableWriter::new(&p, 10).expect("writer");
        for i in 0..10 {
            let key = format!("k:{:02}", i * 3 + table_idx);
            let val = format!("t{table_idx}_v{i}");
            writer
                .write_entry(key.as_bytes(), &MemEntry::Value(val.into_bytes()))
                .expect("write");
        }
        writer.finish().expect("finish");
        input_paths.push(p);
    }

    let merged_path = dir.path().join("merged.sst");

    // Perform compaction merge over the input files
    let result_path = merge_sstables(&input_paths, &merged_path, true).expect("merge");
    assert_eq!(result_path, merged_path);

    // Verify merged SSTable is readable with mmap
    let merged_reader = SSTableReader::open(&merged_path).expect("open merged");
    assert_eq!(merged_reader.entry_count(), 30);

    // Now verify that old files can be cleanly deleted from disk without file lock errors
    for p in &input_paths {
        std::fs::remove_file(p).expect("Old SSTable must be deletable after compaction");
        assert!(!p.exists());
    }

    // Verify merged data integrity
    for i in 0..10 {
        for table_idx in 0..3 {
            let key = format!("k:{:02}", i * 3 + table_idx);
            let entry = merged_reader.get(key.as_bytes()).expect("get").expect("found");
            let expected = format!("t{table_idx}_v{i}");
            assert_eq!(entry.as_value().unwrap(), expected.as_bytes());
        }
    }
}
