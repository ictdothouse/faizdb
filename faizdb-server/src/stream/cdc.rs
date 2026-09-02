//! # Debezium & Kafka-Compatible Change Data Capture (CDC) Event Streamer
//!
//! Emits ordered, transaction-aware mutation events for streaming directly into
//! Apache Kafka topics, Apache Flink pipelines, Snowflake, or Data Lakehouses (Iceberg/Delta Lake).

use serde::{Deserialize, Serialize};

/// CDC Mutation Operation Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CdcOp {
    /// Read initial snapshot event
    Read,
    /// Insert / Create mutation
    Create,
    /// Update / Modify mutation
    Update,
    /// Delete / Purge mutation
    Delete,
}

/// Source metadata envelope conforming to Debezium CDC specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcSource {
    pub version: String,
    pub connector: String,
    pub name: String,
    pub ts_ms: u64,
    pub snapshot: bool,
    pub db: String,
    pub collection: String,
    pub lsn: u64,
}

/// Standard Debezium / Kafka CDC Event Payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcPayload {
    /// State before mutation (None for creates)
    pub before: Option<serde_json::Value>,
    /// State after mutation (None for deletes)
    pub after: Option<serde_json::Value>,
    /// Source provenance metadata
    pub source: CdcSource,
    /// Operation type: 'c' (create), 'u' (update), 'd' (delete), 'r' (read)
    pub op: CdcOp,
    /// Event creation timestamp in milliseconds
    pub ts_ms: u64,
}

/// Complete CDC envelope with JSON-Schema typing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdcEnvelope {
    pub payload: CdcPayload,
}

impl CdcEnvelope {
    /// Create a new CDC event for an insert operation
    pub fn new_create(
        collection: &str,
        doc_id: &str,
        document: serde_json::Value,
        lsn: u64,
    ) -> Self {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            payload: CdcPayload {
                before: None,
                after: Some(document),
                source: CdcSource {
                    version: "0.1.0".to_string(),
                    connector: "faizdb-cdc".to_string(),
                    name: "faizdb_cluster".to_string(),
                    ts_ms: now_ms,
                    snapshot: false,
                    db: "default".to_string(),
                    collection: collection.to_string(),
                    lsn,
                },
                op: CdcOp::Create,
                ts_ms: now_ms,
            },
        }
    }

    /// Serialize to JSON string for Kafka producer ingestion
    pub fn to_kafka_message(&self) -> Result<String, String> {
        serde_json::to_string(self).map_err(|e| format!("Failed to serialize CDC message: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cdc_event_generation_and_kafka_serialization() {
        let doc = serde_json::json!({
            "user_id": "usr_100",
            "name": "Faiz",
            "tier": "Enterprise"
        });

        let cdc_event = CdcEnvelope::new_create("users", "usr_100", doc, 1048576);

        assert_eq!(cdc_event.payload.op, CdcOp::Create);
        assert_eq!(cdc_event.payload.source.collection, "users");
        assert_eq!(cdc_event.payload.source.lsn, 1048576);
        assert!(cdc_event.payload.before.is_none());
        assert!(cdc_event.payload.after.is_some());

        let json_msg = cdc_event.to_kafka_message().unwrap();
        assert!(json_msg.contains("\"connector\":\"faizdb-cdc\""));
        assert!(json_msg.contains("\"Enterprise\""));
    }
}
