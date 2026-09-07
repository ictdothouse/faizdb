//! Integration Tests for Collection Streaming Pagination & Transparent Disk Fallback.

use std::sync::Arc;
use tempfile::tempdir;

use faizdb_core::document::collection::Collection;
use faizdb_core::document::model::Document;
use faizdb_core::storage::engine::{StorageConfig, StorageEngine};

#[test]
fn test_collection_streaming_pagination() {
    let col = Collection::new("test_paginated_users");

    for i in 0..50 {
        let mut doc = Document::new();
        doc.set("name", format!("User_{i:02}"));
        doc.set("index", i as i64);
        col.insert(doc).expect("Insert succeeds");
    }

    assert_eq!(col.stats().document_count, 50);

    // Page 1: skip 0, limit 10
    let p1 = col.find_paginated(0, 10);
    assert_eq!(p1.len(), 10);

    // Page 2: skip 10, limit 10
    let p2 = col.find_paginated(10, 10);
    assert_eq!(p2.len(), 10);

    // Page 5 (last partial page): skip 45, limit 10 -> 5 items
    let p5 = col.find_paginated(45, 10);
    assert_eq!(p5.len(), 5);

    // Out of bounds: skip 100, limit 10 -> 0 items
    let p_empty = col.find_paginated(100, 10);
    assert_eq!(p_empty.len(), 0);
}

#[test]
fn test_transparent_disk_fallback_on_cache_miss() {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        data_dir: temp_dir.path().to_path_buf(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    let storage = Arc::new(StorageEngine::open(config).expect("StorageEngine opens"));
    let col = Collection::with_storage("customers", storage.clone());

    // Insert 10 documents
    let mut doc_ids = Vec::new();
    for i in 0..10 {
        let mut doc = Document::new();
        doc.set("name", format!("Customer_{i}"));
        doc.set("balance", i as i64);
        let id = col.insert(doc).unwrap();
        doc_ids.push(id.as_str().to_string());
    }

    // Verify all can be retrieved normally
    for id in &doc_ids {
        let doc = col.find_by_id(id).unwrap();
        assert_eq!(doc.id.as_str(), id.as_str());
    }

    // Direct disk verification: verify the LSM storage engine actually has the document
    let sample_id = &doc_ids[0];
    let disk_key = format!("doc:customers:{sample_id}").into_bytes();
    let stored_bytes = storage.get(&disk_key).unwrap().expect("Must exist in LSM store");
    assert!(!stored_bytes.is_empty());

    // Test fallback: find_by_id successfully retrieves document and repopulates memory
    let fetched = col.find_by_id(sample_id).unwrap();
    assert_eq!(fetched.id.as_str(), sample_id.as_str());
    assert_eq!(fetched.get("name").unwrap().as_str(), Some("Customer_0"));
}

#[test]
fn test_bounded_memory_with_out_of_core_disk_streaming() {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        data_dir: temp_dir.path().to_path_buf(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    let storage = Arc::new(StorageEngine::open(config).unwrap());
    let col_config = faizdb_core::document::collection::CollectionConfig {
        name: "bounded_orders".to_string(),
        max_memory_documents: Some(5),
        ..Default::default()
    };

    let col = Collection::with_config_and_storage(col_config, storage.clone());

    // Insert 20 documents
    for i in 0..20 {
        let mut doc = Document::new();
        doc.set("order_id", format!("ORD_{i:03}"));
        doc.set("amount", (i * 10) as i64);
        col.insert(doc).unwrap();
    }

    // Assert that in-memory count is capped at 5
    assert!(col.in_memory_count() <= 5);
    // But total count is 20
    assert_eq!(col.stats().document_count, 20);

    // And find_paginated(0, 25) successfully retrieves all 20 from LSM disk!
    let all_orders = col.find_paginated(0, 25);
    assert_eq!(all_orders.len(), 20);

    // And find with filter works across disk!
    let filter = vec![(
        "order_id".to_string(),
        faizdb_core::document::model::Value::String("ORD_015".to_string()),
    )];
    let filtered = col.find(&filter, None, None).unwrap();
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].get("amount").unwrap().as_i64(), Some(150));
}

#[test]
fn test_find_all_and_disk_fallback_operations() {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        data_dir: temp_dir.path().to_path_buf(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    let storage = Arc::new(StorageEngine::open(config).unwrap());
    let col_config = faizdb_core::document::collection::CollectionConfig {
        name: "fallback_products".to_string(),
        max_memory_documents: Some(3),
        ..Default::default()
    };

    let col = Collection::with_config_and_storage(col_config, storage.clone());

    // Insert 10 documents
    let mut ids = Vec::new();
    for i in 0..10 {
        let mut doc = Document::new();
        doc.set("sku", format!("SKU_{i:02}"));
        doc.set("stock", (i + 1) * 10);
        let id = col.insert(doc).unwrap();
        ids.push(id.as_str().to_string());
    }

    // 1. Verify RAM is strictly capped at 3
    assert!(col.in_memory_count() <= 3);
    assert_eq!(col.stats().document_count, 10);

    // 2. Verify find_all(None) retrieves ALL 10 documents from disk
    let all_docs = col.find_all(None);
    assert_eq!(all_docs.len(), 10);

    // 3. Verify duplicate key rejection against disk-resident record
    let evicted_id = &ids[0]; // first inserted was evicted
    let mut duplicate_doc = Document::new();
    duplicate_doc.id = faizdb_core::document::model::DocumentId::from_string(evicted_id.clone());
    duplicate_doc.set("sku", "SKU_DUPLICATE");
    let insert_res = col.insert(duplicate_doc);
    assert!(insert_res.is_err(), "Must reject duplicate key for disk-resident doc");

    // 4. Verify update_by_id on disk-resident record
    let updated = col.update_by_id(evicted_id, |d| {
        d.set("stock", 9999);
    }).expect("update_by_id succeeds on evicted record");
    assert_eq!(updated.get("stock").unwrap().as_i64(), Some(9999));

    // Verify update persisted to disk
    let disk_key = format!("doc:fallback_products:{evicted_id}").into_bytes();
    let disk_bytes = storage.get(&disk_key).unwrap().unwrap();
    let from_disk: Document = serde_json::from_slice(&disk_bytes).unwrap();
    assert_eq!(from_disk.get("stock").unwrap().as_i64(), Some(9999));

    // 5. Verify delete_by_id on disk-resident record
    let deleted = col.delete_by_id(evicted_id).expect("delete_by_id succeeds on evicted record");
    assert_eq!(deleted.id.as_str(), evicted_id.as_str());
    assert_eq!(col.stats().document_count, 9);
    assert!(col.find_by_id(evicted_id).is_err());
    assert!(storage.get(&disk_key).unwrap().is_none());

    // 6. Verify cache-miss repopulation respects memory cap
    for id in &ids[1..6] {
        let _ = col.find_by_id(id);
    }
    assert!(col.in_memory_count() <= 3, "RAM cache must not exceed max_memory_documents");
}

#[test]
fn test_secondary_index_and_bm25_with_evicted_records() {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        data_dir: temp_dir.path().to_path_buf(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    let storage = Arc::new(StorageEngine::open(config).unwrap());
    let col_config = faizdb_core::document::collection::CollectionConfig {
        name: "idx_fallback".to_string(),
        max_memory_documents: Some(2),
        ..Default::default()
    };

    let col = Collection::with_config_and_storage(col_config, storage);

    // Insert 6 articles
    for i in 0..6 {
        let mut doc = Document::new();
        doc.set("category", if i % 2 == 0 { "tech" } else { "news" });
        doc.set("content", format!("Deep quantum computing tutorial article {i}"));
        col.insert(doc).unwrap();
    }

    assert_eq!(col.stats().document_count, 6);
    assert!(col.in_memory_count() <= 2);

    // 1. Create secondary index AFTER documents are already evicted to disk
    col.create_secondary_index("category", false).expect("Index created");

    // 2. Query secondary index for "tech" (should return 3 items, even though resident RAM is <=2)
    let tech_docs = col
        .find_by_secondary_index("category", &faizdb_core::document::model::Value::String("tech".into()))
        .expect("Index lookup succeeds");
    assert_eq!(tech_docs.len(), 3, "Secondary index must retrieve all 3 tech articles across disk");

    // 3. BM25 Full-Text Search across disk-evicted documents
    let search_results = col.search_text("quantum", false, 10);
    assert_eq!(search_results.len(), 6, "BM25 search must find all 6 articles including disk-evicted ones");

    // 4. Count with filter across disk-evicted documents
    let filter = vec![("category".to_string(), faizdb_core::document::model::Value::String("news".into()))];
    let news_count = col.count(&filter);
    assert_eq!(news_count, 3, "Count with filter must accurately count disk records");

    // 5. Delete many with filter across disk-evicted documents
    let deleted_count = col.delete_many(&filter).expect("delete_many succeeds");
    assert_eq!(deleted_count, 3);
    assert_eq!(col.stats().document_count, 3);
}

#[test]
fn test_collection_clear_and_disk_purge() {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        data_dir: temp_dir.path().to_path_buf(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    let storage = Arc::new(StorageEngine::open(config).unwrap());
    let col = Collection::with_storage("purge_col", storage.clone());

    let mut ids = Vec::new();
    for i in 0..10 {
        let mut doc = Document::new();
        doc.set("val", i as i64);
        let id = col.insert(doc).unwrap();
        ids.push(id.as_str().to_string());
    }

    assert_eq!(col.stats().document_count, 10);
    let disk_key = format!("doc:purge_col:{}", ids[0]).into_bytes();
    assert!(storage.get(&disk_key).unwrap().is_some());

    // Clear collection
    col.clear().expect("clear succeeds");
    assert_eq!(col.stats().document_count, 0);
    assert_eq!(col.in_memory_count(), 0);
    assert_eq!(col.stats().total_size, 0);

    // Persistent storage must have been purged
    assert!(storage.get(&disk_key).unwrap().is_none());
    let prefix = b"doc:purge_col:";
    let remaining = storage.prefix_scan(prefix).unwrap();
    assert_eq!(remaining.len(), 0);
}

#[test]
fn test_load_document_memory_cap_and_idempotency() {
    let temp_dir = tempdir().unwrap();
    let config = StorageConfig {
        data_dir: temp_dir.path().to_path_buf(),
        sync_writes: true,
        enable_wal: true,
        ..Default::default()
    };
    let storage = Arc::new(StorageEngine::open(config).unwrap());

    let col_config = faizdb_core::document::collection::CollectionConfig {
        name: "load_cap_col".to_string(),
        max_memory_documents: Some(3),
        ..Default::default()
    };

    let col = Collection::with_config_and_storage(col_config, storage.clone());

    // Populate 10 documents into storage and load into collection
    let mut docs = Vec::new();
    for i in 0..10 {
        let mut doc = Document::new();
        doc.set("seq", i as i64);
        let key = format!("doc:load_cap_col:{}", doc.id.as_str()).into_bytes();
        let val = serde_json::to_vec(&doc).unwrap();
        storage.put(&key, &val).unwrap();
        docs.push(doc);
    }

    for doc in &docs {
        col.load_document(doc.clone());
    }

    // doc_count should be 10
    assert_eq!(col.stats().document_count, 10);
    // in_memory_count MUST obey the max_memory_documents cap of 3!
    assert!(col.in_memory_count() <= 3);

    // Reloading existing resident document should NOT duplicate doc_count or total_size
    let count_before = col.stats().document_count;
    let size_before = col.stats().total_size;
    col.load_document(docs[9].clone());
    assert_eq!(col.stats().document_count, count_before);
    assert_eq!(col.stats().total_size, size_before);
}

#[test]
fn test_sstable_corrupted_overflow_protection() {
    use faizdb_core::storage::sstable::SSTableReader;

    // Craft a byte buffer claiming huge key_len to trigger overflow
    let mut corrupted = Vec::new();
    corrupted.extend_from_slice(&u32::MAX.to_le_bytes()); // key_len = u32::MAX
    corrupted.extend_from_slice(&100u32.to_le_bytes());   // val_len = 100
    corrupted.push(0); // tombstone

    // Must return Err(SsTableCorrupted), NOT panic or overflow
    let result = SSTableReader::read_entry_ref(&corrupted, 0);
    assert!(result.is_err());
}

#[test]
fn test_wal_corrupted_payload_limit() {
    use faizdb_core::storage::wal::WalRecord;
    use std::io::Cursor;

    // Craft a WAL record claiming 2GB payload length
    let mut corrupted = Vec::new();
    corrupted.extend_from_slice(&(2_000_000_000u32).to_le_bytes());
    corrupted.extend_from_slice(&[0u8; 4]); // CRC

    let mut cursor = Cursor::new(corrupted);
    let result = WalRecord::from_reader(&mut cursor, 0);
    assert!(result.is_err(), "Must reject payload exceeding MAX_WAL_SIZE");
}
