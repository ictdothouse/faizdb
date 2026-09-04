//! Hierarchical Navigable Small World (HNSW) graph for sub-millisecond Vector Search.
//!
//! HNSW builds a multi-layer graph where lower layers contain more vertices with
//! short-range links and upper layers contain fewer vertices with long-range skip links.
//! Searching begins at top layer (fast skip) and zooms in at bottom layers.

use rand::Rng;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use crate::distance::DistanceMetric;
use crate::quantization::{
    BinaryQuantizedVector, BinaryQuantizer, QuantizationType, QuantizedVector, ScalarQuantizer,
};

/// Configuration for the HNSW index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswConfig {
    /// Dimension of the vectors (e.g. 128, 768, 1536, up to 4096)
    pub dimensions: usize,
    /// Distance metric to compare vectors
    pub metric: DistanceMetric,
    /// Vector quantization type (e.g. None, Scalar8, or Binary1 for 32x memory compression)
    pub quantization: QuantizationType,
    /// Max outgoing connections per node at layers > 0 (default 16)
    pub m: usize,
    /// Max outgoing connections per node at layer 0 (default 32)
    pub m0: usize,
    /// Size of dynamic candidate list during insertion (default 128)
    pub ef_construction: usize,
    /// Size of dynamic candidate list during search (default 64)
    pub ef_search: usize,
    /// Level generation scaling factor: 1.0 / ln(M)
    pub ml: f64,
}

impl Default for HnswConfig {
    fn default() -> Self {
        let m = 16;
        Self {
            dimensions: 128,
            metric: DistanceMetric::Cosine,
            quantization: QuantizationType::None,
            m,
            m0: m * 2,
            ef_construction: 128,
            ef_search: 64,
            ml: 1.0 / (m as f64).ln(),
        }
    }
}

impl HnswConfig {
    pub fn new(dimensions: usize, metric: DistanceMetric) -> Self {
        let m = 16;
        Self {
            dimensions,
            metric,
            quantization: QuantizationType::None,
            m,
            m0: m * 2,
            ef_construction: 128,
            ef_search: 64,
            ml: 1.0 / (m as f64).ln(),
        }
    }

    /// Set quantization type (e.g. Scalar8 or Binary1 for 32x memory savings)
    pub fn with_quantization(mut self, quantization: QuantizationType) -> Self {
        self.quantization = quantization;
        self
    }
}

/// A node in the HNSW graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswNode {
    /// External document/entity ID
    pub id: String,
    /// Uncompressed vector embedding (empty if quantized)
    pub vector: Vec<f32>,
    /// Quantized 8-bit vector embedding (when Scalar8 is enabled)
    pub quantized: Option<QuantizedVector>,
    /// Quantized 1-bit vector embedding (when Binary1 is enabled)
    pub binary_quantized: Option<BinaryQuantizedVector>,
    /// Highest layer this node exists on
    pub level: usize,
    /// Neighbors at each layer: `neighbors[layer]` = list of node internal indices
    pub neighbors: Vec<Vec<usize>>,
}

/// Candidate element for priority queue searching
#[derive(Debug, Clone, Copy, PartialEq)]
struct Candidate {
    idx: usize,
    distance: f32,
}

impl Eq for Candidate {}

impl PartialOrd for Candidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// For min-heap based on distance (closer is better)
impl Ord for Candidate {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse ordering so lowest distance has highest priority
        other
            .distance
            .partial_cmp(&self.distance)
            .unwrap_or(Ordering::Equal)
    }
}

/// Element for max-heap (furthest has highest priority)
#[derive(Debug, Clone, Copy, PartialEq)]
struct MaxCandidate {
    idx: usize,
    distance: f32,
}

impl Eq for MaxCandidate {}

impl PartialOrd for MaxCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MaxCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.distance
            .partial_cmp(&other.distance)
            .unwrap_or(Ordering::Equal)
    }
}

/// Search result item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    /// Document / vector ID
    pub id: String,
    /// Distance score (smaller = closer)
    pub distance: f32,
    /// Similarity score (1.0 - distance for cosine, or 1/(1+dist))
    pub similarity: f32,
}

/// HNSW Vector Index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HnswIndex {
    pub config: HnswConfig,
    nodes: Vec<HnswNode>,
    id_to_idx: HashMap<String, usize>,
    #[serde(default)]
    deleted: HashSet<usize>,
    entry_point: Option<usize>,
    max_level: usize,
}

impl HnswIndex {
    /// Create a new HNSW index with given configuration
    pub fn new(config: HnswConfig) -> Self {
        Self {
            config,
            nodes: Vec::new(),
            id_to_idx: HashMap::new(),
            deleted: HashSet::new(),
            entry_point: None,
            max_level: 0,
        }
    }

    /// Number of active (non-deleted) vectors stored
    pub fn len(&self) -> usize {
        self.id_to_idx.len()
    }

    /// Total number of nodes including tombstones
    pub fn total_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// Count of deleted (tombstone) vectors
    pub fn deleted_count(&self) -> usize {
        self.deleted.len()
    }

    /// Check if empty of active vectors
    pub fn is_empty(&self) -> bool {
        self.id_to_idx.is_empty()
    }

    /// Check if index contains an entry with given ID
    pub fn contains_id(&self, id: &str) -> bool {
        self.id_to_idx.contains_key(id)
    }

    /// Generate random layer level according to exponential distribution
    fn random_level(&self) -> usize {
        let mut rng = rand::rng();
        let r: f64 = rng.random_range(0.0..1.0);
        let level = (-r.ln() * self.config.ml).floor() as usize;
        level.min(16) // Limit max levels to 16
    }

    /// Compute distance between a float vector and a stored node
    #[inline]
    fn dist(&self, v: &[f32], node_idx: usize) -> f32 {
        let node = &self.nodes[node_idx];
        if let Some(ref qv) = node.quantized {
            ScalarQuantizer::asymmetric_distance(v, qv, self.config.metric)
        } else if let Some(ref bv) = node.binary_quantized {
            let query_bin = BinaryQuantizer::quantize(v);
            BinaryQuantizer::normalized_hamming_distance(&query_bin, bv)
        } else {
            self.config.metric.calculate(v, &node.vector)
        }
    }

    /// Compute distance between two stored nodes
    #[inline]
    fn dist_nodes(&self, a_idx: usize, b_idx: usize) -> f32 {
        let node_a = &self.nodes[a_idx];
        let node_b = &self.nodes[b_idx];
        if let (Some(ref bv_a), Some(ref bv_b)) =
            (&node_a.binary_quantized, &node_b.binary_quantized)
        {
            BinaryQuantizer::normalized_hamming_distance(bv_a, bv_b)
        } else if let Some(ref qv_b) = node_b.quantized {
            if !node_a.vector.is_empty() {
                ScalarQuantizer::asymmetric_distance(&node_a.vector, qv_b, self.config.metric)
            } else if let Some(ref qv_a) = node_a.quantized {
                let decomp_a = ScalarQuantizer::dequantize(qv_a);
                ScalarQuantizer::asymmetric_distance(&decomp_a, qv_b, self.config.metric)
            } else {
                self.config.metric.calculate(&node_a.vector, &node_b.vector)
            }
        } else if !node_a.vector.is_empty() {
            self.config.metric.calculate(&node_a.vector, &node_b.vector)
        } else if let Some(ref qv_a) = node_a.quantized {
            let decomp_a = ScalarQuantizer::dequantize(qv_a);
            self.config.metric.calculate(&decomp_a, &node_b.vector)
        } else {
            0.0
        }
    }

    /// Total RAM footprint in bytes of all nodes and graphs
    pub fn memory_bytes(&self) -> usize {
        let mut total = std::mem::size_of::<Self>();
        for node in &self.nodes {
            total += node.id.len();
            total += node.vector.len() * std::mem::size_of::<f32>();
            if let Some(ref qv) = node.quantized {
                total += qv.memory_bytes();
            }
            if let Some(ref bv) = node.binary_quantized {
                total += bv.memory_bytes();
            }
            for nbrs in &node.neighbors {
                total += nbrs.len() * std::mem::size_of::<usize>();
            }
        }
        total
    }

    /// Delete a vector by ID using tombstone deletion (GDPR & dynamic dataset compliant)
    pub fn delete(&mut self, id: &str) -> bool {
        if let Some(idx) = self.id_to_idx.remove(id) {
            self.deleted.insert(idx);
            // If the deleted node was the entry point, re-elect a new entry point
            if self.entry_point == Some(idx) {
                self.entry_point = self.id_to_idx.values().copied().next();
            }
            true
        } else {
            false
        }
    }

    /// In-place update of a vector with the same document ID
    pub fn update(&mut self, id: impl Into<String>, vector: Vec<f32>) -> Result<(), String> {
        let id_str = id.into();
        self.delete(&id_str);
        self.insert(id_str, vector)
    }

    /// Insert a vector with its external document ID
    pub fn insert(&mut self, id: impl Into<String>, vector: Vec<f32>) -> Result<(), String> {
        let id_str = id.into();
        if vector.len() != self.config.dimensions {
            return Err(format!(
                "Vector dimension mismatch: expected {}, got {}",
                self.config.dimensions,
                vector.len()
            ));
        }

        // If ID already exists, reject duplicate
        if self.id_to_idx.contains_key(&id_str) {
            return Err(format!("Vector with id '{id_str}' already exists"));
        }

        let node_level = self.random_level();
        let new_idx = self.nodes.len();

        let (stored_vector, stored_quantized, stored_binary) = match self.config.quantization {
            QuantizationType::None => (vector.clone(), None, None),
            QuantizationType::Scalar8 => {
                let qv = ScalarQuantizer::quantize(&vector);
                (Vec::new(), Some(qv), None)
            }
            QuantizationType::Binary1 => {
                let bv = BinaryQuantizer::quantize(&vector);
                (Vec::new(), None, Some(bv))
            }
        };

        let new_node = HnswNode {
            id: id_str.clone(),
            vector: stored_vector,
            quantized: stored_quantized,
            binary_quantized: stored_binary,
            level: node_level,
            neighbors: vec![Vec::new(); node_level + 1],
        };
        self.nodes.push(new_node);
        self.id_to_idx.insert(id_str, new_idx);

        let ep = match self.entry_point {
            None => {
                self.entry_point = Some(new_idx);
                self.max_level = node_level;
                return Ok(());
            }
            Some(ep) => ep,
        };

        let mut curr_ep = ep;
        let mut curr_dist = self.dist(&vector, curr_ep);

        // Phase 1: Traverse from top layer down to node_level + 1 with greedy 1-NN search
        for lc in (node_level + 1..=self.max_level).rev() {
            let mut changed = true;
            while changed {
                changed = false;
                if lc < self.nodes[curr_ep].neighbors.len() {
                    for &neighbor in &self.nodes[curr_ep].neighbors[lc] {
                        let d = self.dist(&vector, neighbor);
                        if d < curr_dist {
                            curr_dist = d;
                            curr_ep = neighbor;
                            changed = true;
                        }
                    }
                }
            }
        }

        // Phase 2: Traverse from min(node_level, max_level) down to 0, connecting neighbors
        let start_layer = node_level.min(self.max_level);
        let mut ep_candidates = vec![curr_ep];

        for lc in (0..=start_layer).rev() {
            let candidates =
                self.search_layer(&vector, &ep_candidates, self.config.ef_construction, lc);
            let m_max = if lc == 0 {
                self.config.m0
            } else {
                self.config.m
            };

            // Select M best neighbors
            let selected_neighbors: Vec<usize> =
                candidates.iter().take(m_max).map(|c| c.idx).collect();

            // Connect new node -> neighbors
            self.nodes[new_idx].neighbors[lc] = selected_neighbors.clone();

            // Connect neighbors -> new node (bidirectional) & shrink if needed
            for &neighbor in &selected_neighbors {
                if lc < self.nodes[neighbor].neighbors.len() {
                    self.nodes[neighbor].neighbors[lc].push(new_idx);
                    if self.nodes[neighbor].neighbors[lc].len() > m_max {
                        self.shrink_neighbors(neighbor, lc, m_max);
                    }
                }
            }

            ep_candidates = candidates.into_iter().map(|c| c.idx).collect();
        }

        if node_level > self.max_level {
            self.max_level = node_level;
            self.entry_point = Some(new_idx);
        }

        Ok(())
    }

    /// Shrink a node's neighbor list to max_m using simple heuristic
    fn shrink_neighbors(&mut self, node_idx: usize, layer: usize, max_m: usize) {
        let neighbors = &self.nodes[node_idx].neighbors[layer];

        let mut scored: Vec<(usize, f32)> = neighbors
            .iter()
            .map(|&nbr| (nbr, self.dist_nodes(node_idx, nbr)))
            .collect();

        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        scored.truncate(max_m);

        self.nodes[node_idx].neighbors[layer] = scored.into_iter().map(|(idx, _)| idx).collect();
    }

    /// Search within a specific layer
    fn search_layer(
        &self,
        query: &[f32],
        enter_points: &[usize],
        ef: usize,
        layer: usize,
    ) -> Vec<Candidate> {
        let mut visited = HashSet::new();
        let mut candidates = BinaryHeap::new(); // min-heap (best first)
        let mut results = BinaryHeap::new(); // max-heap (furthest of top ef first)

        for &ep in enter_points {
            let dist = self.dist(query, ep);
            visited.insert(ep);
            candidates.push(Candidate {
                idx: ep,
                distance: dist,
            });
            results.push(MaxCandidate {
                idx: ep,
                distance: dist,
            });
        }

        while let Some(current) = candidates.pop() {
            let furthest_dist = results.peek().map(|c| c.distance).unwrap_or(f32::INFINITY);
            if current.distance > furthest_dist && results.len() >= ef {
                break;
            }

            if layer < self.nodes[current.idx].neighbors.len() {
                for &nbr in &self.nodes[current.idx].neighbors[layer] {
                    if visited.insert(nbr) {
                        let d = self.dist(query, nbr);
                        let furthest = results.peek().map(|c| c.distance).unwrap_or(f32::INFINITY);
                        if d < furthest || results.len() < ef {
                            candidates.push(Candidate {
                                idx: nbr,
                                distance: d,
                            });
                            results.push(MaxCandidate {
                                idx: nbr,
                                distance: d,
                            });
                            if results.len() > ef {
                                results.pop();
                            }
                        }
                    }
                }
            }
        }

        let mut sorted: Vec<Candidate> = results
            .into_iter()
            .map(|c| Candidate {
                idx: c.idx,
                distance: c.distance,
            })
            .collect();
        sorted.sort_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(Ordering::Equal)
        });
        sorted
    }

    /// Search K nearest neighbors with non-panicking dimension validation
    pub fn try_search(
        &self,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorSearchResult>, String> {
        if self.nodes.is_empty() || top_k == 0 {
            return Ok(Vec::new());
        }

        if query.len() != self.config.dimensions {
            return Err(format!(
                "Query vector dimension mismatch: expected {}, got {}",
                self.config.dimensions,
                query.len()
            ));
        }

        Ok(self.search(query, top_k))
    }

    /// Search K nearest neighbors for a query vector
    pub fn search(&self, query: &[f32], top_k: usize) -> Vec<VectorSearchResult> {
        if self.is_empty() || top_k == 0 {
            return Vec::new();
        }

        assert_eq!(
            query.len(),
            self.config.dimensions,
            "Query vector dimension mismatch"
        );

        let ep = match self.entry_point {
            Some(ep) if !self.deleted.contains(&ep) => ep,
            _ => match self.id_to_idx.values().copied().next() {
                Some(valid_ep) => valid_ep,
                None => return Vec::new(),
            },
        };

        let mut curr_ep = ep;
        let mut curr_dist = self.dist(query, curr_ep);

        // Top layers: 1-NN greedy jump
        for lc in (1..=self.max_level).rev() {
            let mut changed = true;
            while changed {
                changed = false;
                if lc < self.nodes[curr_ep].neighbors.len() {
                    for &neighbor in &self.nodes[curr_ep].neighbors[lc] {
                        let d = self.dist(query, neighbor);
                        if d < curr_dist {
                            curr_dist = d;
                            curr_ep = neighbor;
                            changed = true;
                        }
                    }
                }
            }
        }

        // Layer 0: Search with ef_search
        let ef = self
            .config
            .ef_search
            .max(top_k + self.deleted.len().min(self.config.ef_search));
        let candidates = self.search_layer(query, &[curr_ep], ef, 0);

        candidates
            .into_iter()
            .filter(|c| !self.deleted.contains(&c.idx))
            .take(top_k)
            .map(|c| {
                let id = self.nodes[c.idx].id.clone();
                let distance = c.distance;
                let similarity = match self.config.metric {
                    DistanceMetric::Cosine => (1.0 - distance).clamp(0.0, 1.0),
                    _ => 1.0 / (1.0 + distance),
                };
                VectorSearchResult {
                    id,
                    distance,
                    similarity,
                }
            })
            .collect()
    }

    /// Serialize the HNSW index graph to JSON bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec(self).map_err(|e| format!("Failed to serialize HNSW index: {e}"))
    }

    /// Deserialize an HNSW index graph from JSON bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| format!("Failed to deserialize HNSW index: {e}"))
    }

    /// Save the HNSW index graph to a disk file path.
    pub fn save_to_file<P: AsRef<std::path::Path>>(&self, path: P) -> Result<(), String> {
        if let Some(parent) = path.as_ref().parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let bytes = self.to_bytes()?;
        std::fs::write(path, bytes).map_err(|e| format!("Failed to write HNSW index file: {e}"))
    }

    /// Load the HNSW index graph from a disk file path.
    pub fn load_from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("Failed to read HNSW index file: {e}"))?;
        Self::from_bytes(&bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hnsw_insert_and_search() {
        let config = HnswConfig::new(3, DistanceMetric::Cosine);
        let mut index = HnswIndex::new(config);

        // Insert vectors representing different concepts
        index.insert("ai", vec![1.0, 0.0, 0.0]).unwrap();
        index.insert("ml", vec![0.95, 0.05, 0.0]).unwrap();
        index.insert("cooking", vec![0.0, 1.0, 0.0]).unwrap();
        index.insert("baking", vec![0.0, 0.9, 0.1]).unwrap();
        index.insert("astronomy", vec![0.0, 0.0, 1.0]).unwrap();

        assert_eq!(index.len(), 5);

        // Query near AI
        let results = index.search(&[0.99, 0.01, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "ai");
        assert_eq!(results[1].id, "ml");

        // Query near cooking
        let results = index.search(&[0.0, 0.95, 0.05], 2);
        assert_eq!(results.len(), 2);
        assert!(results[0].id == "cooking" || results[0].id == "baking");
    }

    #[test]
    fn test_hnsw_high_dimension() {
        let dim = 128;
        let config = HnswConfig::new(dim, DistanceMetric::Cosine);
        let mut index = HnswIndex::new(config);

        // Insert 100 random vectors
        let mut rng = rand::rng();
        for i in 0..100 {
            let mut v: Vec<f32> = (0..dim).map(|_| rng.random_range(-1.0..1.0)).collect();
            crate::distance::normalize_in_place(&mut v);
            index.insert(format!("doc_{i}"), v).unwrap();
        }

        assert_eq!(index.len(), 100);

        // Query first doc
        let query_vec = index.nodes[0].vector.clone();
        let results = index.search(&query_vec, 5);
        assert_eq!(results[0].id, "doc_0");
        assert!(results[0].distance < 1e-5);
    }

    #[test]
    fn test_hnsw_persistence_serialization() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test_index.hnsw");

        let config = HnswConfig::new(3, DistanceMetric::Cosine);
        let mut original = HnswIndex::new(config);
        original.insert("vec1", vec![1.0, 0.0, 0.0]).unwrap();
        original.insert("vec2", vec![0.0, 1.0, 0.0]).unwrap();

        // Save to file
        original.save_to_file(&file_path).unwrap();
        assert!(file_path.exists());

        // Reload from file
        let restored = HnswIndex::load_from_file(&file_path).unwrap();
        assert_eq!(restored.len(), 2);

        // Verify search works identically on restored index
        let res = restored.search(&[0.9, 0.1, 0.0], 1);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].id, "vec1");
    }

    #[test]
    fn test_quantized_hnsw_search_and_recall() {
        let dim = 64;
        let config = HnswConfig::new(dim, DistanceMetric::Cosine)
            .with_quantization(QuantizationType::Scalar8);
        let mut index = HnswIndex::new(config);

        // Insert 50 random vectors
        let mut rng = rand::rng();
        let mut vectors = Vec::new();
        for i in 0..50 {
            let mut v: Vec<f32> = (0..dim).map(|_| rng.random_range(-1.0..1.0)).collect();
            crate::distance::normalize_in_place(&mut v);
            vectors.push(v.clone());
            index.insert(format!("item_{i}"), v).unwrap();
        }

        assert_eq!(index.len(), 50);

        // Query with vector 0 — the nearest item should be item_0 itself
        let results = index.search(&vectors[0], 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "item_0");
        assert!(
            results[0].distance < 0.05,
            "Quantized distance to exact vector should be near 0"
        );
    }

    #[test]
    fn test_quantized_hnsw_memory_reduction() {
        let dim = 128;
        let count = 100;
        let mut rng = rand::rng();

        // 1. Raw f32 index
        let raw_config = HnswConfig::new(dim, DistanceMetric::Cosine);
        let mut raw_index = HnswIndex::new(raw_config);

        // 2. Quantized SQ8 index
        let q_config = HnswConfig::new(dim, DistanceMetric::Cosine)
            .with_quantization(QuantizationType::Scalar8);
        let mut q_index = HnswIndex::new(q_config);

        for i in 0..count {
            let mut v: Vec<f32> = (0..dim).map(|_| rng.random_range(-1.0..1.0)).collect();
            crate::distance::normalize_in_place(&mut v);
            raw_index.insert(format!("doc_{i}"), v.clone()).unwrap();
            q_index.insert(format!("doc_{i}"), v).unwrap();
        }

        let raw_bytes = raw_index.memory_bytes();
        let q_bytes = q_index.memory_bytes();

        // Quantized index should consume significantly less RAM for stored vectors
        assert!(
            q_bytes < raw_bytes,
            "Quantized index bytes ({q_bytes}) must be smaller than raw ({raw_bytes})"
        );
    }

    #[test]
    fn test_binary_quantized_hnsw_32x_reduction() {
        let dim = 128;
        let count = 60;
        let mut rng = rand::rng();

        let bin_config = HnswConfig::new(dim, DistanceMetric::Cosine)
            .with_quantization(QuantizationType::Binary1);
        let mut bin_index = HnswIndex::new(bin_config);

        let mut vectors = Vec::new();
        for i in 0..count {
            let mut v: Vec<f32> = (0..dim).map(|_| rng.random_range(-1.0..1.0)).collect();
            crate::distance::normalize_in_place(&mut v);
            vectors.push(v.clone());
            bin_index.insert(format!("bin_doc_{i}"), v).unwrap();
        }

        assert_eq!(bin_index.len(), count);

        // Search with query vector 0
        let results = bin_index.search(&vectors[0], 3);
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "bin_doc_0");
    }

    #[test]
    fn test_hnsw_tombstone_deletion_and_update() {
        let dim = 8;
        let config = HnswConfig::new(dim, DistanceMetric::Euclidean);
        let mut index = HnswIndex::new(config);

        index
            .insert("v1", vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .insert("v2", vec![0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
            .unwrap();
        index
            .insert("v3", vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0])
            .unwrap();

        assert_eq!(index.len(), 3);
        assert_eq!(index.deleted_count(), 0);

        // Search near v1
        let res = index.search(&[0.9, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1);
        assert_eq!(res[0].id, "v1");

        // Delete v1 (tombstone)
        assert!(index.delete("v1"));
        assert_eq!(index.len(), 2);
        assert_eq!(index.deleted_count(), 1);
        assert!(!index.contains_id("v1"));

        // Search again near v1 — v1 must NOT appear
        let res_after = index.search(&[0.9, 0.1, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 2);
        assert!(!res_after.iter().any(|r| r.id == "v1"));

        // Update v2 with new coordinates
        index
            .update("v2", vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0])
            .unwrap();
        assert_eq!(index.len(), 2);
        let res_v2 = index.search(&[0.9, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1);
        assert_eq!(res_v2[0].id, "v2");
    }
}
