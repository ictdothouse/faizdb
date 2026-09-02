//! # Adaptive Replacement Cache (ARC) Engine
//!
//! Implements the self-tuning Adaptive Replacement Cache algorithm (Megiddo & Modha).
//! Automatically balances between **Recency** (LRU) and **Frequency** (LFU) access patterns
//! without requiring manual cache tuning.
//!
//! ## Mathematical Foundations
//! - Maintains 4 doubly-linked lists:
//!   - `T1`: Recent cache entries (size $\le p$)
//!   - `T2`: Frequent cache entries (size $\le c - p$)
//!   - `B1`: Ghost recency history (evicted from `T1`, tracks keys without data)
//!   - `B2`: Ghost frequency history (evicted from `T2`, tracks keys without data)
//! - Target adaptation parameter $p \in [0, c]$ self-tunes based on cache hit feedback:
//!   - Hit in $B_1 \implies p = \min(c, p + \max(1, |B_2| / |B_1|))$
//!   - Hit in $B_2 \implies p = \max(0, p - \max(1, |B_1| / |B_2|))$

use parking_lot::Mutex;
use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::Arc;

/// Statistics for the Adaptive Replacement Cache
#[derive(Debug, Clone, Copy, Default)]
pub struct ArcCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub ghost_hits_b1: u64,
    pub ghost_hits_b2: u64,
    pub evictions: u64,
}

impl ArcCacheStats {
    /// Hit ratio calculation
    pub fn hit_ratio(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Adaptive Replacement Cache (ARC)
pub struct ArcCache<K, V> {
    capacity: usize,
    p: usize, // Target size for T1
    t1: VecDeque<K>, // Recent entries
    t2: VecDeque<K>, // Frequent entries
    b1: VecDeque<K>, // Ghost recent keys
    b2: VecDeque<K>, // Ghost frequent keys
    store: HashMap<K, V>,
    stats: ArcCacheStats,
}

impl<K: Clone + Eq + Hash, V: Clone> ArcCache<K, V> {
    /// Create a new ARC cache with given maximum capacity $c$
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "ARC capacity must be greater than 0");
        Self {
            capacity,
            p: 0,
            t1: VecDeque::new(),
            t2: VecDeque::new(),
            b1: VecDeque::new(),
            b2: VecDeque::new(),
            store: HashMap::new(),
            stats: ArcCacheStats::default(),
        }
    }

    /// Retrieve an entry from the cache
    pub fn get(&mut self, key: &K) -> Option<V> {
        // Case 1: Hit in T1 or T2
        if let Some(val) = self.store.get(key).cloned() {
            self.stats.hits += 1;
            // Move from T1 -> T2 (it has now been accessed at least twice)
            if let Some(pos) = self.t1.iter().position(|k| k == key) {
                self.t1.remove(pos);
                self.t2.push_back(key.clone());
            } else if let Some(pos) = self.t2.iter().position(|k| k == key) {
                // Move to MRU position in T2
                self.t2.remove(pos);
                self.t2.push_back(key.clone());
            }
            return Some(val);
        }

        self.stats.misses += 1;
        None
    }

    /// Insert or update an entry in the cache
    pub fn put(&mut self, key: K, value: V) {
        // If already in store, update value and promote to T2
        if self.store.contains_key(&key) {
            self.store.insert(key.clone(), value);
            if let Some(pos) = self.t1.iter().position(|k| *k == key) {
                self.t1.remove(pos);
                self.t2.push_back(key);
            } else if let Some(pos) = self.t2.iter().position(|k| *k == key) {
                self.t2.remove(pos);
                self.t2.push_back(key);
            }
            return;
        }

        // Case A: Key is in ghost list B1 (Recency bias detected)
        if let Some(pos) = self.b1.iter().position(|k| *k == key) {
            self.stats.ghost_hits_b1 += 1;
            self.b1.remove(pos);

            // Adapt p: increase recency capacity target
            let delta = if self.b1.is_empty() {
                1
            } else {
                (self.b2.len() / self.b1.len()).max(1)
            };
            self.p = (self.p + delta).min(self.capacity);

            self.replace(&key);
            self.t2.push_back(key.clone());
            self.store.insert(key, value);
            return;
        }

        // Case B: Key is in ghost list B2 (Frequency bias detected)
        if let Some(pos) = self.b2.iter().position(|k| *k == key) {
            self.stats.ghost_hits_b2 += 1;
            self.b2.remove(pos);

            // Adapt p: decrease recency capacity target (give more room to T2)
            let delta = if self.b2.is_empty() {
                1
            } else {
                (self.b1.len() / self.b2.len()).max(1)
            };
            self.p = self.p.saturating_sub(delta);

            self.replace(&key);
            self.t2.push_back(key.clone());
            self.store.insert(key, value);
            return;
        }

        // Case C: Cache Miss (Key is entirely new)
        let l1_size = self.t1.len() + self.b1.len();
        let total_size = l1_size + self.t2.len() + self.b2.len();

        if l1_size == self.capacity {
            if self.t1.len() < self.capacity {
                self.b1.pop_front();
                self.replace(&key);
            } else if let Some(k) = self.t1.pop_front() {
                self.store.remove(&k);
            }
        } else if total_size >= self.capacity {
            if total_size == 2 * self.capacity && !self.b2.is_empty() {
                self.b2.pop_front();
            }
            self.replace(&key);
        }

        self.t1.push_back(key.clone());
        self.store.insert(key, value);
    }

    /// Subroutine to evict an item from T1 or T2 to ghost lists B1 or B2
    fn replace(&mut self, _key: &K) {
        if !self.t1.is_empty()
            && ((self.t1.len() > self.p) || (self.b2.iter().any(|k| k == _key) && self.t1.len() == self.p))
        {
            if let Some(old_k) = self.t1.pop_front() {
                self.store.remove(&old_k);
                self.b1.push_back(old_k);
                self.stats.evictions += 1;
            }
        } else if let Some(old_k) = self.t2.pop_front() {
            self.store.remove(&old_k);
            self.b2.push_back(old_k);
            self.stats.evictions += 1;
        }
    }

    /// Current statistics
    pub fn stats(&self) -> ArcCacheStats {
        self.stats
    }

    /// Current target parameter p
    pub fn target_p(&self) -> usize {
        self.p
    }

    /// Number of live items in cache
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.store.is_empty()
    }
}

/// Thread-safe shared ARC cache
pub type SharedArcCache<K, V> = Arc<Mutex<ArcCache<K, V>>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arc_basic_put_get() {
        let mut cache = ArcCache::new(3);

        cache.put("a", 1);
        cache.put("b", 2);
        cache.put("c", 3);

        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));
        assert_eq!(cache.len(), 3);
    }

    #[test]
    fn test_arc_adaptive_ghost_hit_tuning() {
        let mut cache = ArcCache::new(2);

        cache.put("k1", 100);
        cache.put("k2", 200);

        // Evicts k1 to B1
        cache.put("k3", 300);
        assert_eq!(cache.get(&"k1"), None); // Miss

        // Access k1 again -> Ghost hit in B1!
        cache.put("k1", 150);
        assert!(cache.stats().ghost_hits_b1 > 0);
        assert!(cache.target_p() > 0, "p should dynamically adapt upwards on B1 hit");
    }
}
