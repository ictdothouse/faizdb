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
    let mut col_config = faizdb_core::document::collection::CollectionConfig::default();
    col_config.name = "bounded_orders".to_string();
    col_config.max_memory_documents = Some(5); // Cap RAM to only 5 documents!

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
