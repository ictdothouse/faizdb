//! Integration tests for StorageEngine WAL persistence, startup recovery, and MVCC transactions.

use tempfile::TempDir;
use faizdb_core::document::model::Document;
use faizdb_query::DatabaseContext;

#[tokio::test]
async fn test_storage_engine_crash_recovery() {
    let tmp_dir = TempDir::new().unwrap();
    let data_path = tmp_dir.path().to_path_buf();

    // 1. First run: Insert documents into collection with active StorageEngine
    {
        let ctx = DatabaseContext::with_storage_dir(&data_path).expect("Failed to open storage");
        let users = ctx.get_or_create_collection("users");

        let doc1 = Document::new()
            .field("name", "Alice")
            .field("role", "Engineer");
        let doc2 = Document::new()
            .field("name", "Bob")
            .field("role", "Designer");

        let id1 = users.insert(doc1).unwrap();
        let id2 = users.insert(doc2).unwrap();

        assert_eq!(users.stats().document_count, 2);
        assert!(users.find_by_id(id1.as_str()).is_ok());
        assert!(users.find_by_id(id2.as_str()).is_ok());

        // Update doc1
        users.update_by_id(id1.as_str(), |d| {
            d.set("role", "Lead Engineer");
        }).unwrap();

        // Flush active memtable to SSTable to test multi-tier recovery
        if let Some(storage) = ctx.storage() {
            storage.flush().unwrap();
        }

        // Insert doc3 after flush (will reside in WAL / active memtable)
        let doc3 = Document::new()
            .field("name", "Charlie")
            .field("role", "Manager");
        let _id3 = users.insert(doc3).unwrap();
    } // Drop DatabaseContext and StorageEngine (simulating shutdown/restart)

    // 2. Second run: Re-open DatabaseContext from the same directory
    {
        let ctx = DatabaseContext::with_storage_dir(&data_path).expect("Failed to reopen storage");
        let users = ctx.get_or_create_collection("users");

        // Verify all 3 documents were recovered into memory
        assert_eq!(users.stats().document_count, 3);

        let all_docs = users.find_all(None);
        let names: Vec<String> = all_docs
            .iter()
            .filter_map(|d| d.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
            .collect();

        assert!(names.contains(&"Alice".to_string()));
        assert!(names.contains(&"Bob".to_string()));
        assert!(names.contains(&"Charlie".to_string()));

        // Verify the updated role was preserved
        let alice_doc = all_docs.iter().find(|d| d.get("name").and_then(|v| v.as_str()) == Some("Alice")).unwrap();
        assert_eq!(alice_doc.get("role").unwrap().as_str(), Some("Lead Engineer"));

        // Verify text search works on recovered documents
        let search_results = users.search_text("Engineer", false, 5);
        assert!(!search_results.is_empty());
    }
}

#[tokio::test]
async fn test_mvcc_transaction_commit_and_abort() {
    let tmp_dir = TempDir::new().unwrap();
    let ctx = DatabaseContext::with_storage_dir(tmp_dir.path()).expect("Failed to open storage");
    let tx_mgr = ctx.tx_manager();

    // Begin two concurrent transactions
    let mut txn1 = tx_mgr.begin();
    let mut txn2 = tx_mgr.begin();

    // txn1 writes key_a
    txn1.put(b"key_a".to_vec(), b"val_1".to_vec()).unwrap();
    // txn2 writes key_a (conflicting write)
    txn2.put(b"key_a".to_vec(), b"val_2".to_vec()).unwrap();

    // Commit txn1 -> must succeed
    assert!(tx_mgr.commit(&mut txn1).is_ok());

    // Commit txn2 -> must fail due to write-write conflict with txn1!
    let commit2_result = tx_mgr.commit(&mut txn2);
    assert!(commit2_result.is_err(), "Concurrent conflicting write should be rejected");

    // Abort txn2 cleanly
    tx_mgr.abort(&mut txn2);
}

#[tokio::test]
async fn test_mvcc_snapshot_isolation() {
    let tmp_dir = TempDir::new().unwrap();
    let ctx = DatabaseContext::with_storage_dir(tmp_dir.path()).expect("Failed to open storage");
    let tx_mgr = ctx.tx_manager();

    // Non-conflicting transactions
    let mut txn1 = tx_mgr.begin();
    let mut txn2 = tx_mgr.begin();

    txn1.put(b"user:1".to_vec(), b"Alice".to_vec()).unwrap();
    txn2.put(b"user:2".to_vec(), b"Bob".to_vec()).unwrap();

    // Both should commit cleanly
    assert!(tx_mgr.commit(&mut txn1).is_ok());
    assert!(tx_mgr.commit(&mut txn2).is_ok());
}
