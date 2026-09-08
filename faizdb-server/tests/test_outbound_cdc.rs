//! Integration tests for Active Outbound CDC Stream Dispatcher (Phase 4)

use faizdb_server::stream::cdc::{CdcEnvelope, CdcOp};
use faizdb_server::stream::{
    CdcDispatcherConfig, CdcOutboundDispatcher, CdcTransportBackend, InMemoryCdcTransport,
};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn test_cdc_full_mutation_lifecycle_and_serialization() {
    // 1. Create mutation
    let doc_initial = serde_json::json!({
        "sku": "ITEM_99",
        "name": "Widget Alpha",
        "price": 49.99
    });
    let cdc_create = CdcEnvelope::new_create("products", "p99", doc_initial.clone(), 1001);
    assert_eq!(cdc_create.payload.op, CdcOp::Create);
    assert!(cdc_create.payload.before.is_none());
    assert_eq!(cdc_create.payload.after.as_ref().unwrap()["price"], 49.99);

    let kafka_create_json = cdc_create.to_kafka_message().expect("Serialize create");
    assert!(kafka_create_json.contains("\"connector\":\"faizdb-cdc\""));
    assert!(kafka_create_json.contains("\"op\":\"create\""));

    // 2. Update mutation
    let doc_updated = serde_json::json!({
        "sku": "ITEM_99",
        "name": "Widget Alpha Pro",
        "price": 59.99
    });
    let cdc_update = CdcEnvelope::new_update("products", "p99", Some(doc_initial.clone()), doc_updated, 1002);
    assert_eq!(cdc_update.payload.op, CdcOp::Update);
    assert_eq!(cdc_update.payload.before.as_ref().unwrap()["price"], 49.99);
    assert_eq!(cdc_update.payload.after.as_ref().unwrap()["price"], 59.99);

    let kafka_update_json = cdc_update.to_kafka_message().expect("Serialize update");
    assert!(kafka_update_json.contains("\"op\":\"update\""));
    assert!(kafka_update_json.contains("Widget Alpha Pro"));

    // 3. Delete mutation
    let cdc_delete = CdcEnvelope::new_delete("products", "p99", Some(doc_initial), 1003);
    assert_eq!(cdc_delete.payload.op, CdcOp::Delete);
    assert!(cdc_delete.payload.before.is_some());
    assert!(cdc_delete.payload.after.is_none());

    let kafka_delete_json = cdc_delete.to_kafka_message().expect("Serialize delete");
    assert!(kafka_delete_json.contains("\"op\":\"delete\""));
}

#[tokio::test]
async fn test_outbound_cdc_dispatcher_batch_delivery() {
    let transport = Arc::new(InMemoryCdcTransport::new());
    let config = CdcDispatcherConfig {
        endpoint_url: "in_memory://lakehouse_events".to_string(),
        batch_size: 3,
        flush_interval: Duration::from_millis(50),
        max_retries: 2,
        initial_backoff: Duration::from_millis(10),
        channel_capacity: 50,
    };

    let (dispatcher, sender, metrics) =
        CdcOutboundDispatcher::new(config, CdcTransportBackend::InMemory(Arc::clone(&transport)));

    let worker = tokio::spawn(dispatcher.run());

    // Emit 3 events to trigger instant batch flush
    for i in 1..=3 {
        let doc = serde_json::json!({"id": i, "status": "active"});
        let env = CdcEnvelope::new_create("accounts", &format!("acc_{i}"), doc, i as u64);
        assert!(sender.emit(env));
    }

    // Wait for batch processing
    tokio::time::sleep(Duration::from_millis(80)).await;

    let delivered = transport.get_delivered();
    assert_eq!(delivered.len(), 3);
    assert_eq!(metrics.events_delivered.load(Ordering::SeqCst), 3);
    assert_eq!(metrics.batches_dispatched.load(Ordering::SeqCst), 1);

    drop(sender);
    let _ = worker.await;
}
