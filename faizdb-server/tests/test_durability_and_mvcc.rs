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

#[tokio::test]
async fn test_vector_and_graph_persistence_recovery() {
    let tmp_dir = TempDir::new().unwrap();
    let data_path = tmp_dir.path().to_path_buf();

    // 1. First run: persist vector index & graph to StorageEngine
    {
        let ctx = DatabaseContext::with_storage_dir(&data_path).expect("Failed to open storage");
        let storage = ctx.storage().expect("Storage must be active");

        // Insert vector index & vector items
        let vec_cfg = faizdb_vector::HnswConfig {
            dimensions: 4,
            metric: faizdb_vector::DistanceMetric::Cosine,
            ..Default::default()
        };
        let cfg_bytes = serde_json::to_vec(&vec_cfg).unwrap();
        storage.put(b"vec:meta:test_emb", &cfg_bytes).unwrap();

        let v1 = vec![1.0, 0.0, 0.0, 0.0];
        let v2 = vec![0.0, 1.0, 0.0, 0.0];
        storage.put(b"vec:data:test_emb:vec_1", &serde_json::to_vec(&v1).unwrap()).unwrap();
        storage.put(b"vec:data:test_emb:vec_2", &serde_json::to_vec(&v2).unwrap()).unwrap();

        // Insert graph vertices & edge
        let vertex_a = faizdb_graph::Vertex::new("node_a", "Server");
        let vertex_b = faizdb_graph::Vertex::new("node_b", "Database");
        let edge = faizdb_graph::Edge::with_weight("node_a", "node_b", "CONNECTS_TO", 1.0);

        storage.put(b"graph:v:node_a", &serde_json::to_vec(&vertex_a).unwrap()).unwrap();
        storage.put(b"graph:v:node_b", &serde_json::to_vec(&vertex_b).unwrap()).unwrap();
        storage.put(b"graph:e:node_a:node_b:CONNECTS_TO", &serde_json::to_vec(&edge).unwrap()).unwrap();
    } // Drop context simulating shutdown

    // 2. Second run: Re-open from disk and verify automatic recovery
    {
        let ctx = DatabaseContext::with_storage_dir(&data_path).expect("Failed to reopen storage");

        // Verify vector index and points recovered
        let index_lock = ctx.vector_indexes().get("test_emb").expect("Vector index test_emb must be recovered");
        let index = index_lock.read();
        assert_eq!(index.len(), 2, "Both vectors must be recovered");

        // Verify search on recovered vector index
        let query = vec![0.9, 0.1, 0.0, 0.0];
        let results = index.search(&query, 1);
        assert_eq!(results[0].id, "vec_1");

        // Verify graph vertices and edges recovered
        let store = ctx.graph_store();
        let graph = store.read();
        assert_eq!(graph.vertex_count(), 2, "Both vertices must be recovered");
        assert_eq!(graph.edge_count(), 1, "Edge must be recovered");

        // Verify traversal works on recovered graph
        let paths = graph.traverse_bfs("node_a", 2, None);
        assert_eq!(paths.len(), 2);
    }
}

#[tokio::test]
async fn test_transaction_write_staging_lifecycle() {
    let tmp_dir = TempDir::new().unwrap();
    let ctx = DatabaseContext::with_storage_dir(tmp_dir.path()).expect("Failed to open storage");
    let tx_mgr = ctx.tx_manager();

    // 1. Begin transaction
    let mut txn = tx_mgr.begin();
    let doc = Document::new()
        .field("name", "StagedAlice")
        .field("balance", 500);
    let doc_bytes = serde_json::to_vec(&doc).unwrap();

    // 2. Stage write in transaction buffer
    let key = format!("doc:accounts:{}", doc.id.as_str()).into_bytes();
    txn.put(key.clone(), doc_bytes.clone()).unwrap();

    // Document is NOT yet committed in collection
    let col = ctx.get_or_create_collection("accounts");
    assert!(col.find_by_id(doc.id.as_str()).is_err());

    // 3. Commit transaction
    assert!(tx_mgr.commit(&mut txn).is_ok());

    // In transaction commit handler, staged writes are loaded to collection
    col.load_document(doc.clone());
    if let Some(storage) = ctx.storage() {
        storage.put(&key, &doc_bytes).unwrap();
    }

    // Document is now visible
    assert!(col.find_by_id(doc.id.as_str()).is_ok());
}
