//! Consistent Hashing Ring & Auto-Sharding Engine.

use std::collections::BTreeMap;
use crc32fast::Hasher;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Total number of virtual hash slots across the cluster
pub const TOTAL_SHARD_SLOTS: u16 = 16_384;

/// Shard allocation range for a cluster node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardRange {
    pub start_slot: u16,
    pub end_slot: u16,
    pub slot_count: u16,
    pub node_id: String,
    pub node_address: String,
}

/// Sharding status and distribution overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShardDistribution {
    pub total_slots: u16,
    pub active_shards: usize,
    pub ranges: Vec<ShardRange>,
}

/// Dynamic Consistent Hashing Ring for auto-partitioning
pub struct ShardRouter {
    nodes: RwLock<BTreeMap<String, String>>, // node_id -> node_address
}

impl Default for ShardRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl ShardRouter {
    /// Initialize a new shard router
    pub fn new() -> Self {
        Self {
            nodes: RwLock::new(BTreeMap::new()),
        }
    }

    /// Register or update a node in the hash ring
    pub fn register_node(&self, node_id: impl Into<String>, address: impl Into<String>) {
        let mut nodes = self.nodes.write();
        nodes.insert(node_id.into(), address.into());
    }

    /// Remove a node from the hash ring
    pub fn unregister_node(&self, node_id: &str) {
        let mut nodes = self.nodes.write();
        nodes.remove(node_id);
    }

    /// Compute the shard slot (0..16383) for a given document key
    pub fn calculate_slot(key: &str) -> u16 {
        let mut hasher = Hasher::new();
        hasher.update(key.as_bytes());
        (hasher.finalize() % (TOTAL_SHARD_SLOTS as u32)) as u16
    }

    /// Locate the target node responsible for a given document key
    pub fn route_key(&self, key: &str) -> Option<(String, String)> {
        let slot = Self::calculate_slot(key);
        self.route_slot(slot)
    }

    /// Locate the target node responsible for a given shard slot
    pub fn route_slot(&self, slot: u16) -> Option<(String, String)> {
        let nodes = self.nodes.read();
        if nodes.is_empty() {
            return None;
        }

        let node_list: Vec<(&String, &String)> = nodes.iter().collect();
        let slots_per_node = TOTAL_SHARD_SLOTS / (node_list.len() as u16);
        let node_idx = ((slot / slots_per_node) as usize).min(node_list.len() - 1);

        let (node_id, addr) = node_list[node_idx];
        Some((node_id.clone(), addr.clone()))
    }

    /// Generate full cluster shard distribution table
    pub fn get_distribution(&self) -> ShardDistribution {
        let nodes = self.nodes.read();
        if nodes.is_empty() {
            return ShardDistribution {
                total_slots: TOTAL_SHARD_SLOTS,
                active_shards: 0,
                ranges: vec![],
            };
        }

        let node_list: Vec<(&String, &String)> = nodes.iter().collect();
        let n = node_list.len() as u16;
        let slots_per_node = TOTAL_SHARD_SLOTS / n;

        let mut ranges = Vec::new();
        for (i, (node_id, addr)) in node_list.iter().enumerate() {
            let start = (i as u16) * slots_per_node;
            let end = if i == (n as usize) - 1 {
                TOTAL_SHARD_SLOTS - 1
            } else {
                start + slots_per_node - 1
            };

            ranges.push(ShardRange {
                start_slot: start,
                end_slot: end,
                slot_count: end - start + 1,
                node_id: (*node_id).clone(),
                node_address: (*addr).clone(),
            });
        }

        ShardDistribution {
            total_slots: TOTAL_SHARD_SLOTS,
            active_shards: ranges.len(),
            ranges,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consistent_hash_routing() {
        let router = ShardRouter::new();
        router.register_node("node_1", "127.0.0.1:27018");
        router.register_node("node_2", "127.0.0.1:27028");
        router.register_node("node_3", "127.0.0.1:27038");

        let dist = router.get_distribution();
        assert_eq!(dist.active_shards, 3);
        assert_eq!(dist.ranges[0].start_slot, 0);
        assert_eq!(dist.ranges[2].end_slot, 16383);

        let (target_node, _) = router.route_key("user_faiz_001").unwrap();
        assert!(["node_1", "node_2", "node_3"].contains(&target_node.as_str()));
    }
}
