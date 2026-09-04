//! Conflict-Free Replicated Data Types (CRDTs) for Multi-Region Geo-Replication.
//!
//! Provides mathematically proven convergent data structures for active-active multi-datacenter clusters:
//! - [`VersionVector`]: Causality and concurrent update tracker.
//! - [`LwwRegister`]: Last-Write-Wins conflict resolution register.
//! - [`OrSet`]: Observed-Remove Set supporting concurrent adds/removes.
//! - [`PnCounter`]: Positive-Negative distributed counter.
//! - [`CrdtDocument`]: Field-level CRDT document merger.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Version Vector tracking causal history across cluster regions.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct VersionVector {
    pub versions: BTreeMap<String, u64>,
}

impl VersionVector {
    pub fn new() -> Self {
        Self {
            versions: BTreeMap::new(),
        }
    }

    /// Increment version counter for a given region
    pub fn increment(&mut self, region: &str) -> u64 {
        let entry = self.versions.entry(region.to_string()).or_insert(0);
        *entry += 1;
        *entry
    }

    /// Get version for a region
    pub fn get(&self, region: &str) -> u64 {
        self.versions.get(region).copied().unwrap_or(0)
    }

    /// Merge with another version vector (takes pairwise maximum)
    pub fn merge(&mut self, other: &VersionVector) {
        for (region, &v2) in &other.versions {
            let v1 = self.versions.entry(region.clone()).or_insert(0);
            if v2 > *v1 {
                *v1 = v2;
            }
        }
    }

    /// Check if self causally dominates other (self >= other)
    pub fn dominates(&self, other: &VersionVector) -> bool {
        for (region, &v2) in &other.versions {
            if self.get(region) < v2 {
                return false;
            }
        }
        true
    }

    /// Check if self strictly dominates other (self > other)
    pub fn strictly_dominates(&self, other: &VersionVector) -> bool {
        self.dominates(other) && self != other
    }

    /// Check if two version vectors represent concurrent updates (split-brain)
    pub fn is_concurrent(&self, other: &VersionVector) -> bool {
        !self.dominates(other) && !other.dominates(self)
    }
}

/// Last-Write-Wins Register (LWW-Register).
/// Resolves concurrent writes using (Timestamp, RegionID) as strict total ordering tie-breaker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LwwRegister<T> {
    pub value: T,
    pub timestamp: u64,
    pub region_id: String,
}

impl<T: Clone + PartialEq> LwwRegister<T> {
    pub fn new(value: T, timestamp: u64, region_id: impl Into<String>) -> Self {
        Self {
            value,
            timestamp,
            region_id: region_id.into(),
        }
    }

    /// Update value if new timestamp is higher or region_id is lexicographically greater
    pub fn set(&mut self, value: T, timestamp: u64, region_id: &str) -> bool {
        if timestamp > self.timestamp
            || (timestamp == self.timestamp && region_id > self.region_id.as_str())
        {
            self.value = value;
            self.timestamp = timestamp;
            self.region_id = region_id.to_string();
            true
        } else {
            false
        }
    }

    /// Alias for set() — updates value if timestamp/region is newer
    pub fn update(&mut self, value: T, timestamp: u64, region_id: &str) -> bool {
        self.set(value, timestamp, region_id)
    }

    /// Merge two LWW registers deterministically
    pub fn merge(&mut self, other: &LwwRegister<T>) {
        if other.timestamp > self.timestamp
            || (other.timestamp == self.timestamp && other.region_id > self.region_id)
        {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
            self.region_id = other.region_id.clone();
        }
    }
}

/// Observed-Remove Set (OR-Set).
/// Allows concurrent additions and deletions across regions without lost updates.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OrSet<T: Ord + Clone> {
    pub adds: BTreeMap<T, BTreeSet<(u64, String)>>, // item -> set of (timestamp, tag)
    pub removes: BTreeMap<T, u64>,                  // item -> highest remove timestamp
}

impl<T: Ord + Clone> OrSet<T> {
    pub fn new() -> Self {
        Self {
            adds: BTreeMap::new(),
            removes: BTreeMap::new(),
        }
    }

    /// Add an element with a unique tag
    pub fn add(&mut self, item: T, timestamp: u64, tag: &str) {
        self.adds
            .entry(item)
            .or_default()
            .insert((timestamp, tag.to_string()));
    }

    /// Remove an element
    pub fn remove(&mut self, item: T, timestamp: u64) {
        let entry = self.removes.entry(item).or_insert(0);
        if timestamp > *entry {
            *entry = timestamp;
        }
    }

    /// Read all active elements in the set
    pub fn read(&self) -> Vec<T> {
        let mut active = Vec::new();
        for (item, tags) in &self.adds {
            let remove_ts = self.removes.get(item).copied().unwrap_or(0);
            // Element is present if any add tag has timestamp > remove timestamp
            if tags.iter().any(|(ts, _)| *ts > remove_ts) {
                active.push(item.clone());
            }
        }
        active
    }

    /// Merge another OR-Set (commutative and idempotent)
    pub fn merge(&mut self, other: &OrSet<T>) {
        for (item, tags) in &other.adds {
            self.adds
                .entry(item.clone())
                .or_default()
                .extend(tags.clone());
        }
        for (item, &ts) in &other.removes {
            let entry = self.removes.entry(item.clone()).or_insert(0);
            if ts > *entry {
                *entry = ts;
            }
        }
    }
}

/// Positive-Negative Distributed Counter (PN-Counter).
/// Allows lock-free concurrent increments and decrements.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PnCounter {
    pub p: BTreeMap<String, i64>,
    pub n: BTreeMap<String, i64>,
}

impl PnCounter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn increment(&mut self, region: &str, delta: i64) {
        let val = self.p.entry(region.to_string()).or_insert(0);
        *val += delta.abs();
    }

    pub fn decrement(&mut self, region: &str, delta: i64) {
        let val = self.n.entry(region.to_string()).or_insert(0);
        *val += delta.abs();
    }

    pub fn value(&self) -> i64 {
        let pos: i64 = self.p.values().sum();
        let neg: i64 = self.n.values().sum();
        pos - neg
    }

    pub fn merge(&mut self, other: &PnCounter) {
        for (r, &v) in &other.p {
            let cur = self.p.entry(r.clone()).or_insert(0);
            if v > *cur {
                *cur = v;
            }
        }
        for (r, &v) in &other.n {
            let cur = self.n.entry(r.clone()).or_insert(0);
            if v > *cur {
                *cur = v;
            }
        }
    }
}

/// Alias for LwwRegister for compatibility
pub type CrdtLwwRegister<T> = LwwRegister<T>;

/// Node-scoped distributed PN-Counter wrapper
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CrdtPnCounter {
    pub node_id: String,
    pub counter: PnCounter,
}

impl CrdtPnCounter {
    pub fn new(node_id: impl Into<String>) -> Self {
        Self {
            node_id: node_id.into(),
            counter: PnCounter::new(),
        }
    }

    pub fn increment(&mut self) {
        self.counter.increment(&self.node_id, 1);
    }

    pub fn decrement(&mut self) {
        self.counter.decrement(&self.node_id, 1);
    }

    pub fn value(&self) -> i64 {
        self.counter.value()
    }

    pub fn merge(&mut self, other: &CrdtPnCounter) {
        self.counter.merge(&other.counter);
    }
}

/// Field-level CRDT Document representation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CrdtDocument {
    pub fields: BTreeMap<String, LwwRegister<serde_json::Value>>,
}

impl CrdtDocument {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_field(
        &mut self,
        field: &str,
        value: serde_json::Value,
        timestamp: u64,
        region: &str,
    ) {
        if let Some(reg) = self.fields.get_mut(field) {
            reg.set(value, timestamp, region);
        } else {
            self.fields.insert(
                field.to_string(),
                LwwRegister::new(value, timestamp, region),
            );
        }
    }

    pub fn merge(&mut self, other: &CrdtDocument) {
        for (k, other_reg) in &other.fields {
            if let Some(my_reg) = self.fields.get_mut(k) {
                my_reg.merge(other_reg);
            } else {
                self.fields.insert(k.clone(), other_reg.clone());
            }
        }
    }

    pub fn to_json_map(&self) -> BTreeMap<String, serde_json::Value> {
        self.fields
            .iter()
            .map(|(k, reg)| (k.clone(), reg.value.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_vector_causality() {
        let mut v1 = VersionVector::new();
        let mut v2 = VersionVector::new();

        v1.increment("ap-southeast");
        v1.increment("ap-southeast");
        assert!(v1.dominates(&v2));

        v2.increment("us-east");
        assert!(v1.is_concurrent(&v2));

        v1.merge(&v2);
        assert_eq!(v1.get("ap-southeast"), 2);
        assert_eq!(v1.get("us-east"), 1);
        assert!(v1.dominates(&v2));
    }

    #[test]
    fn test_lww_register_convergence() {
        let mut reg_sg = LwwRegister::new("SG-Value", 100, "ap-southeast-1");
        let reg_us = LwwRegister::new("US-Value", 105, "us-east-1");

        reg_sg.merge(&reg_us);
        assert_eq!(reg_sg.value, "US-Value");

        // Equal timestamp tie-breaker
        let mut reg_a = LwwRegister::new("Value-A", 200, "region-a");
        let reg_b = LwwRegister::new("Value-B", 200, "region-b");
        reg_a.merge(&reg_b);
        assert_eq!(reg_a.value, "Value-B");
    }

    #[test]
    fn test_pn_counter_convergence() {
        let mut cnt_sg = PnCounter::new();
        let mut cnt_us = PnCounter::new();

        cnt_sg.increment("ap-southeast", 10);
        cnt_sg.decrement("ap-southeast", 2);

        cnt_us.increment("us-east", 50);
        cnt_us.decrement("us-east", 10);

        cnt_sg.merge(&cnt_us);
        assert_eq!(cnt_sg.value(), (10 - 2) + (50 - 10));
    }

    #[test]
    fn test_crdt_document_field_level_merge() {
        let mut doc_sg = CrdtDocument::new();
        doc_sg.set_field("name", serde_json::json!("Faiz SG"), 100, "ap-southeast-1");
        doc_sg.set_field(
            "city",
            serde_json::json!("Kuala Lumpur"),
            100,
            "ap-southeast-1",
        );

        let mut doc_us = CrdtDocument::new();
        doc_us.set_field(
            "title",
            serde_json::json!("Chief Architect"),
            110,
            "us-east-1",
        );
        doc_us.set_field("name", serde_json::json!("Ahmad Faiz"), 120, "us-east-1");

        doc_sg.merge(&doc_us);
        let map = doc_sg.to_json_map();

        assert_eq!(map.get("name").unwrap(), &serde_json::json!("Ahmad Faiz"));
        assert_eq!(map.get("city").unwrap(), &serde_json::json!("Kuala Lumpur"));
        assert_eq!(
            map.get("title").unwrap(),
            &serde_json::json!("Chief Architect")
        );
    }
}
