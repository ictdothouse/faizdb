//! # Semantic Cache for AI Workloads and GraphRAG Queries
//!
//! Stores query prompts, embeddings, retrieved graph context, and document IDs.
//! Provides fast sub-millisecond similarity matching with configurable threshold
//! and automatic TTL expiration so LLM GraphRAG pipelines avoid redundant retrieval.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};


/// A cached item in the semantic cache
#[derive(Debug, Clone)]
pub struct SemanticCacheEntry {
    /// Original user/agent query prompt
    pub prompt: String,
    /// Embedding vector of the query prompt
    pub embedding: Vec<f32>,
    /// Retrieved context (formatted knowledge graph text, document snippets, etc.)
    pub context: String,
    /// Associated document IDs
    pub document_ids: Vec<String>,
    /// Timestamp when this entry was created
    pub created_at: Instant,
    /// Time-to-live duration before this cache entry expires
    pub ttl: Duration,
}

impl SemanticCacheEntry {
    /// Check if this cache entry is expired
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.ttl
    }
}

/// A hit from the semantic cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SemanticCacheHit {
    /// Cosine similarity score (0.0 to 1.0)
    pub similarity: f32,
    /// Matched prompt
    pub prompt: String,
    /// Cached context
    pub context: String,
    /// Cached document IDs
    pub document_ids: Vec<String>,
}

/// Thread-safe in-memory Semantic Cache with cosine similarity matching
pub struct SemanticCache {
    entries: RwLock<Vec<SemanticCacheEntry>>,
    /// Minimum cosine similarity required to consider a lookup a cache hit (e.g., 0.90)
    threshold: f32,
    /// Default time-to-live for new entries
    default_ttl: Duration,
    /// Maximum number of cache entries
    max_capacity: usize,
}

impl Default for SemanticCache {
    fn default() -> Self {
        Self::new(0.90, Duration::from_secs(300), 10_000)
    }
}

impl SemanticCache {
    /// Create a new SemanticCache
    pub fn new(threshold: f32, default_ttl: Duration, max_capacity: usize) -> Self {
        Self {
            entries: RwLock::new(Vec::new()),
            threshold: threshold.clamp(0.0, 1.0),
            default_ttl,
            max_capacity: max_capacity.max(1),
        }
    }

    /// Set minimum cosine similarity threshold
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.0, 1.0);
    }

    /// Get current similarity threshold
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Put a new entry into the semantic cache with default TTL
    pub fn put(
        &self,
        prompt: impl Into<String>,
        embedding: Vec<f32>,
        context: impl Into<String>,
        document_ids: Vec<String>,
    ) {
        self.put_with_ttl(prompt, embedding, context, document_ids, self.default_ttl);
    }

    /// Put a new entry into the semantic cache with custom TTL
    pub fn put_with_ttl(
        &self,
        prompt: impl Into<String>,
        embedding: Vec<f32>,
        context: impl Into<String>,
        document_ids: Vec<String>,
        ttl: Duration,
    ) {
        let mut entries = self.entries.write();

        // Evict expired entries if capacity reached
        if entries.len() >= self.max_capacity {
            entries.retain(|e| !e.is_expired());
            // If still full, remove oldest entry (FIFO / LRU-approximate)
            if entries.len() >= self.max_capacity {
                entries.remove(0);
            }
        }

        entries.push(SemanticCacheEntry {
            prompt: prompt.into(),
            embedding,
            context: context.into(),
            document_ids,
            created_at: Instant::now(),
            ttl,
        });
    }

    /// Look up the semantic cache using a query embedding.
    /// Returns the best matching entry if its cosine similarity is >= threshold and it has not expired.
    pub fn get(&self, query_embedding: &[f32]) -> Option<SemanticCacheHit> {
        if query_embedding.is_empty() {
            return None;
        }

        let entries = self.entries.read();
        let mut best_hit: Option<SemanticCacheHit> = None;
        let mut highest_sim = self.threshold;

        for entry in entries.iter() {
            if entry.is_expired() {
                continue;
            }

            if entry.embedding.len() != query_embedding.len() {
                continue;
            }

            let sim = cosine_similarity(query_embedding, &entry.embedding);
            if sim >= highest_sim {
                highest_sim = sim;
                best_hit = Some(SemanticCacheHit {
                    similarity: sim,
                    prompt: entry.prompt.clone(),
                    context: entry.context.clone(),
                    document_ids: entry.document_ids.clone(),
                });
            }
        }

        best_hit
    }

    /// Remove all expired entries from cache
    pub fn invalidate_expired(&self) -> usize {
        let mut entries = self.entries.write();
        let initial_len = entries.len();
        entries.retain(|e| !e.is_expired());
        initial_len - entries.len()
    }

    /// Get current active entries count (ignoring expired ones)
    pub fn len(&self) -> usize {
        let entries = self.entries.read();
        entries.iter().filter(|e| !e.is_expired()).count()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all entries from the semantic cache
    pub fn clear(&self) {
        self.entries.write().clear();
    }
}

/// Compute cosine similarity between two float vectors in 100% Safe Rust
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot_product = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot_product += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    if norm_a <= f32::EPSILON || norm_b <= f32::EPSILON {
        return 0.0;
    }

    (dot_product / (norm_a.sqrt() * norm_b.sqrt())).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];
        let d = vec![0.7071, 0.7071, 0.0];

        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-4);
        assert!((cosine_similarity(&a, &c) - 0.0).abs() < 1e-4);
        assert!((cosine_similarity(&a, &d) - 0.7071).abs() < 1e-3);
    }

    #[test]
    fn test_semantic_cache_hit_and_miss() {
        let cache = SemanticCache::new(0.90, Duration::from_secs(60), 100);

        // Put an entry for "What is FaizDB?"
        cache.put(
            "What is FaizDB?",
            vec![1.0, 0.0, 0.0],
            "FaizDB is an AI-native database engine.",
            vec!["doc_overview".to_string()],
        );

        // Query with identical or near-identical vector (similarity > 0.90)
        let hit = cache.get(&[0.99, 0.05, 0.0]);
        assert!(hit.is_some());
        let hit_val = hit.unwrap();
        assert!(hit_val.similarity >= 0.90);
        assert_eq!(hit_val.prompt, "What is FaizDB?");
        assert_eq!(hit_val.context, "FaizDB is an AI-native database engine.");

        // Query with orthogonal vector (similarity ~ 0.0 < 0.90)
        let miss = cache.get(&[0.0, 1.0, 0.0]);
        assert!(miss.is_none());
    }

    #[test]
    fn test_semantic_cache_expiration() {
        let cache = SemanticCache::new(0.80, Duration::from_millis(10), 100);

        cache.put_with_ttl(
            "Quick prompt",
            vec![1.0, 0.0],
            "Temporary context",
            vec![],
            Duration::from_millis(5),
        );

        // Immediate check
        assert_eq!(cache.len(), 1);

        // Sleep to let TTL expire
        std::thread::sleep(Duration::from_millis(15));

        assert!(cache.get(&[1.0, 0.0]).is_none());
        assert_eq!(cache.len(), 0);
    }
}
