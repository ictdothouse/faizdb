//! Multi-Region Geo-Replication Engine for Active-Active Datacenters.
//!
//! Enables asynchronous, strong eventual consistency cross-datacenter replication
//! using Conflict-Free Replicated Data Types (CRDTs) and Version Vectors.

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::info;

use super::crdt::{CrdtDocument, VersionVector};

/// Peer Region Configuration in the Global Mesh
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionConfig {
    pub region_id: String,
    pub endpoint: String,
    pub is_active: bool,
    pub last_synced_at: DateTime<Utc>,
    pub latency_ms: u64,
}

/// A replicated mutation delta shipped across datacenters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationDelta {
    pub source_region: String,
    pub collection: String,
    pub document_id: String,
    pub field_updates: BTreeMap<String, (serde_json::Value, u64, String)>, // (value, timestamp, region)
    pub version_vector: VersionVector,
    pub timestamp: u64,
}

/// The Geo-Replication orchestrator running inside FaizDB core
#[derive(Clone)]
pub struct GeoReplicationEngine {
    pub local_region: String,
    pub peers: Arc<DashMap<String, RegionConfig>>,
    pub crdt_documents: Arc<DashMap<String, CrdtDocument>>,
    pub version_vector: Arc<RwLock<VersionVector>>,
    pub outgoing_queue: Arc<RwLock<Vec<ReplicationDelta>>>,
}

impl GeoReplicationEngine {
    /// Initialize Geo-Replication with local region identifier (e.g. "ap-southeast-1")
    pub fn new(local_region: impl Into<String>) -> Self {
        let region = local_region.into();
        info!("🌍 Initializing Multi-Region Geo-Replication Engine for region '{region}'");

        Self {
            local_region: region,
            peers: Arc::new(DashMap::new()),
            crdt_documents: Arc::new(DashMap::new()),
            version_vector: Arc::new(RwLock::new(VersionVector::new())),
            outgoing_queue: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register a remote datacenter peer in the replication mesh
    pub fn register_peer(&self, region_id: &str, endpoint: &str) {
        let config = RegionConfig {
            region_id: region_id.to_string(),
            endpoint: endpoint.to_string(),
            is_active: true,
            last_synced_at: Utc::now(),
            latency_ms: 0,
        };
        self.peers.insert(region_id.to_string(), config);
        info!("Registered geo-replication peer '{region_id}' at {endpoint}");
    }

    /// List all registered peer regions and their sync statuses
    pub fn list_peers(&self) -> Vec<RegionConfig> {
        self.peers
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Record a local document mutation and queue a replication delta
    pub fn record_local_mutation(
        &self,
        collection: &str,
        document_id: &str,
        fields: BTreeMap<String, serde_json::Value>,
    ) {
        let now_ms = Utc::now().timestamp_millis() as u64;
        let mut vv = self.version_vector.write();
        vv.increment(&self.local_region);

        let key = format!("{collection}:{document_id}");
        let mut crdt_doc = self.crdt_documents.entry(key.clone()).or_default();

        let mut field_updates = BTreeMap::new();
        for (k, v) in fields {
            crdt_doc.set_field(&k, v.clone(), now_ms, &self.local_region);
            field_updates.insert(k, (v, now_ms, self.local_region.clone()));
        }

        let delta = ReplicationDelta {
            source_region: self.local_region.clone(),
            collection: collection.to_string(),
            document_id: document_id.to_string(),
            field_updates,
            version_vector: vv.clone(),
            timestamp: now_ms,
        };

        self.outgoing_queue.write().push(delta);
    }

    /// Apply an incoming replication delta from a remote region
    pub fn apply_remote_delta(&self, delta: ReplicationDelta) -> bool {
        let key = format!("{}:{}", delta.collection, delta.document_id);
        let mut crdt_doc = self.crdt_documents.entry(key).or_default();

        for (field, (val, ts, region)) in delta.field_updates {
            crdt_doc.set_field(&field, val, ts, &region);
        }

        // Update local version vector
        let mut vv = self.version_vector.write();
        vv.merge(&delta.version_vector);

        // Update peer last synced timestamp
        if let Some(mut peer) = self.peers.get_mut(&delta.source_region) {
            peer.last_synced_at = Utc::now();
        }

        true
    }

    /// Drain outgoing replication deltas for transmission to peers
    pub fn drain_outgoing_deltas(&self) -> Vec<ReplicationDelta> {
        let mut queue = self.outgoing_queue.write();
        std::mem::take(&mut *queue)
    }

    /// Get current state of a document across all regional merges
    pub fn get_document_state(
        &self,
        collection: &str,
        document_id: &str,
    ) -> Option<BTreeMap<String, serde_json::Value>> {
        let key = format!("{collection}:{document_id}");
        self.crdt_documents.get(&key).map(|doc| doc.to_json_map())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geo_replication_active_active_sync() {
        let sg_engine = GeoReplicationEngine::new("ap-southeast-1");
        let us_engine = GeoReplicationEngine::new("us-east-1");

        sg_engine.register_peer("us-east-1", "http://us.faizdb.io:27018");
        us_engine.register_peer("ap-southeast-1", "http://sg.faizdb.io:27018");

        // 1. Concurrent writes in SG and US to the same user profile document
        let mut sg_fields = BTreeMap::new();
        sg_fields.insert("name".to_string(), serde_json::json!("Ahmad Faiz"));
        sg_fields.insert("city".to_string(), serde_json::json!("Kuala Lumpur"));
        sg_engine.record_local_mutation("users", "usr_100", sg_fields);

        let mut us_fields = BTreeMap::new();
        us_fields.insert("plan".to_string(), serde_json::json!("Enterprise"));
        us_fields.insert("credits".to_string(), serde_json::json!(50000));
        us_engine.record_local_mutation("users", "usr_100", us_fields);

        // 2. Cross-replicate deltas
        let sg_deltas = sg_engine.drain_outgoing_deltas();
        let us_deltas = us_engine.drain_outgoing_deltas();

        for delta in sg_deltas {
            us_engine.apply_remote_delta(delta);
        }
        for delta in us_deltas {
            sg_engine.apply_remote_delta(delta);
        }

        // 3. Verify identical convergence in both regions
        let sg_final = sg_engine.get_document_state("users", "usr_100").unwrap();
        let us_final = us_engine.get_document_state("users", "usr_100").unwrap();

        assert_eq!(sg_final, us_final);
        assert_eq!(
            sg_final.get("name").unwrap(),
            &serde_json::json!("Ahmad Faiz")
        );
        assert_eq!(
            sg_final.get("city").unwrap(),
            &serde_json::json!("Kuala Lumpur")
        );
        assert_eq!(
            sg_final.get("plan").unwrap(),
            &serde_json::json!("Enterprise")
        );
        assert_eq!(sg_final.get("credits").unwrap(), &serde_json::json!(50000));
    }
}
