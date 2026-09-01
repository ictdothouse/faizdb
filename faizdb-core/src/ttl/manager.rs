//! Time-To-Live (TTL) & High-Speed Cache Expiration Manager.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Current timestamp in epoch milliseconds
pub fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Statistics for TTL Cache operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtlStats {
    pub active_tracked_keys: usize,
    pub total_purged_count: u64,
    pub next_expiry_ms: Option<u64>,
}

/// High-Performance TTL Expiry Scheduler
pub struct TtlManager {
    /// Ordered index: expiry_timestamp_ms -> Set<doc_id>
    expirations: RwLock<BTreeMap<u64, HashSet<String>>>,
    /// Reverse lookup: doc_id -> expiry_timestamp_ms
    doc_to_expiry: DashMap<String, u64>,
    /// Total count of automatically purged documents
    total_purged: AtomicU64,
}

impl Default for TtlManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TtlManager {
    /// Initialize a new TTL Manager
    pub fn new() -> Self {
        Self {
            expirations: RwLock::new(BTreeMap::new()),
            doc_to_expiry: DashMap::new(),
            total_purged: AtomicU64::new(0),
        }
    }

    /// Register document with relative TTL in seconds
    pub fn set_expiry(&self, doc_id: &str, ttl_seconds: u64) {
        let expire_at = current_time_ms() + (ttl_seconds * 1000);
        self.set_expiry_at(doc_id, expire_at);
    }

    /// Register document with absolute expiration epoch timestamp in milliseconds
    pub fn set_expiry_at(&self, doc_id: &str, expire_at_ms: u64) {
        // If doc previously had an expiry, remove it first
        self.remove(doc_id);

        self.doc_to_expiry.insert(doc_id.to_string(), expire_at_ms);
        let mut exp = self.expirations.write();
        exp.entry(expire_at_ms).or_default().insert(doc_id.to_string());
    }

    /// Remove a document from TTL tracking (e.g. upon manual deletion)
    pub fn remove(&self, doc_id: &str) {
        if let Some((_, old_expiry)) = self.doc_to_expiry.remove(doc_id) {
            let mut exp = self.expirations.write();
            if let Some(set) = exp.get_mut(&old_expiry) {
                set.remove(doc_id);
                if set.is_empty() {
                    exp.remove(&old_expiry);
                }
            }
        }
    }

    /// Check if a document is expired at a given timestamp
    pub fn is_expired(&self, doc_id: &str, now_ms: u64) -> bool {
        if let Some(expire_at) = self.doc_to_expiry.get(doc_id) {
            *expire_at <= now_ms
        } else {
            false
        }
    }

    /// Purge and return all document IDs that have expired up to `now_ms`
    pub fn purge_expired(&self, now_ms: u64) -> Vec<String> {
        let mut expired_ids = Vec::new();

        {
            let mut exp = self.expirations.write();
            // Extract all timestamps <= now_ms
            let to_remove: Vec<u64> = exp
                .range(..=now_ms)
                .map(|(&timestamp, _)| timestamp)
                .collect();

            for ts in to_remove {
                if let Some(set) = exp.remove(&ts) {
                    for id in set {
                        self.doc_to_expiry.remove(&id);
                        expired_ids.push(id);
                    }
                }
            }
        }

        if !expired_ids.is_empty() {
            self.total_purged.fetch_add(expired_ids.len() as u64, Ordering::Relaxed);
        }

        expired_ids
    }

    /// Get current TTL manager statistics
    pub fn get_stats(&self) -> TtlStats {
        let exp = self.expirations.read();
        let next_expiry = exp.keys().next().copied();

        TtlStats {
            active_tracked_keys: self.doc_to_expiry.len(),
            total_purged_count: self.total_purged.load(Ordering::Relaxed),
            next_expiry_ms: next_expiry,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ttl_expiry_and_purge() {
        let manager = TtlManager::new();
        let now = current_time_ms();

        manager.set_expiry_at("session_1", now + 1000);
        manager.set_expiry_at("session_2", now + 3000);

        assert!(!manager.is_expired("session_1", now + 500));
        assert!(manager.is_expired("session_1", now + 1500));

        let purged = manager.purge_expired(now + 2000);
        assert_eq!(purged, vec!["session_1"]);
        assert_eq!(manager.get_stats().active_tracked_keys, 1);
    }
}
