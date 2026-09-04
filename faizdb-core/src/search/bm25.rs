//! Inverted Index & Okapi BM25 Full-Text Search Engine.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::tokenizer::{levenshtein_distance, tokenize};

/// Parameters for the Okapi BM25 scoring algorithm
pub const BM25_K1: f64 = 1.2;
pub const BM25_B: f64 = 0.75;

/// Metadata for a term match inside a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermMeta {
    pub frequency: usize,
    pub positions: Vec<usize>,
}

/// A ranked full-text search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub doc_id: String,
    pub score: f64,
    pub matched_terms: Vec<String>,
    pub snippet: Option<String>,
}

/// Inverted Index with fast concurrent BM25 scoring
pub struct InvertedIndex {
    /// term -> (doc_id -> TermMeta)
    postings: DashMap<String, HashMap<String, TermMeta>>,
    /// doc_id -> total tokens
    doc_lengths: DashMap<String, usize>,
    /// Total number of indexed documents
    total_docs: AtomicUsize,
    /// Total number of tokens indexed across all documents
    total_tokens: AtomicUsize,
}

impl Default for InvertedIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl InvertedIndex {
    /// Create a new inverted search index
    pub fn new() -> Self {
        Self {
            postings: DashMap::new(),
            doc_lengths: DashMap::new(),
            total_docs: AtomicUsize::new(0),
            total_tokens: AtomicUsize::new(0),
        }
    }

    /// Index or re-index a document text
    pub fn index_document(&self, doc_id: &str, text: &str) {
        // Remove previous index for this doc if it exists
        self.remove_document(doc_id);

        let tokens = tokenize(text);
        let doc_len = tokens.len();
        if doc_len == 0 {
            return;
        }

        self.doc_lengths.insert(doc_id.to_string(), doc_len);
        self.total_docs.fetch_add(1, Ordering::SeqCst);
        self.total_tokens.fetch_add(doc_len, Ordering::SeqCst);

        let mut term_positions: HashMap<String, Vec<usize>> = HashMap::new();
        for (pos, token) in tokens.iter().enumerate() {
            term_positions.entry(token.clone()).or_default().push(pos);
        }

        for (term, positions) in term_positions {
            let meta = TermMeta {
                frequency: positions.len(),
                positions,
            };
            self.postings
                .entry(term)
                .or_default()
                .insert(doc_id.to_string(), meta);
        }
    }

    /// Remove a document from the inverted index
    pub fn remove_document(&self, doc_id: &str) {
        if let Some((_, old_len)) = self.doc_lengths.remove(doc_id) {
            self.total_docs.fetch_sub(1, Ordering::SeqCst);
            self.total_tokens.fetch_sub(old_len, Ordering::SeqCst);

            for mut entry in self.postings.iter_mut() {
                entry.value_mut().remove(doc_id);
            }
        }
    }

    /// Average document length across index
    pub fn avg_doc_length(&self) -> f64 {
        let n = self.total_docs.load(Ordering::Relaxed);
        if n == 0 {
            return 1.0;
        }
        let total_tok = self.total_tokens.load(Ordering::Relaxed) as f64;
        (total_tok / (n as f64)).max(1.0)
    }

    /// Search for matching documents using Okapi BM25 ranking
    pub fn search(&self, query: &str, fuzzy: bool, top_k: usize) -> Vec<SearchResult> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return vec![];
        }

        let n = self.total_docs.load(Ordering::Relaxed) as f64;
        if n == 0.0 {
            return vec![];
        }

        let avg_dl = self.avg_doc_length();
        let mut doc_scores: HashMap<String, (f64, HashSet<String>)> = HashMap::new();

        for q_term in &query_tokens {
            let mut matching_terms: Vec<(String, f64)> = Vec::new();

            // 1. Exact match
            if self.postings.contains_key(q_term) {
                matching_terms.push((q_term.clone(), 1.0));
            }

            // 2. Fuzzy match if requested (Levenshtein distance <= 1 or 2)
            if fuzzy {
                for entry in self.postings.iter() {
                    let index_term = entry.key();
                    if index_term != q_term {
                        let dist = levenshtein_distance(q_term, index_term);
                        if dist == 1 {
                            matching_terms.push((index_term.clone(), 0.75));
                        } else if dist == 2 && q_term.len() >= 5 {
                            matching_terms.push((index_term.clone(), 0.50));
                        }
                    }
                }
            }

            for (term, term_weight) in matching_terms {
                if let Some(postings_map) = self.postings.get(&term) {
                    let df = postings_map.len() as f64;
                    // Standard BM25 Inverse Document Frequency (IDF)
                    let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln().max(0.1);

                    for (doc_id, meta) in postings_map.iter() {
                        let tf = meta.frequency as f64;
                        let dl = self
                            .doc_lengths
                            .get(doc_id)
                            .map(|r| *r.value())
                            .unwrap_or(1) as f64;

                        // Okapi BM25 formula
                        let numerator = tf * (BM25_K1 + 1.0);
                        let denominator = tf + BM25_K1 * (1.0 - BM25_B + BM25_B * (dl / avg_dl));
                        let bm25_score = idf * (numerator / denominator) * term_weight;

                        let entry = doc_scores
                            .entry(doc_id.clone())
                            .or_insert_with(|| (0.0, HashSet::new()));
                        entry.0 += bm25_score;
                        entry.1.insert(term.clone());
                    }
                }
            }
        }

        let mut results: Vec<SearchResult> = doc_scores
            .into_iter()
            .map(|(doc_id, (score, matched_set))| SearchResult {
                doc_id,
                score: (score * 100.0).round() / 100.0,
                matched_terms: matched_set.into_iter().collect(),
                snippet: None,
            })
            .collect();

        // Sort descending by relevance score
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_ranking_and_fuzzy() {
        let index = InvertedIndex::new();
        index.index_document(
            "doc_1",
            "FaizDB is a blazing fast NoSQL database written in Rust",
        );
        index.index_document("doc_2", "Rust is a modern systems programming language");
        index.index_document(
            "doc_3",
            "Database design principles and distributed Raft consensus",
        );

        // Exact BM25 query
        let res = index.search("database rust", false, 5);
        assert!(!res.is_empty());
        assert_eq!(res[0].doc_id, "doc_1"); // Contains both "database" and "rust"

        // Fuzzy query (with typo: "databse")
        let fuzzy_res = index.search("databse", true, 5);
        assert!(!fuzzy_res.is_empty());
        assert!(["doc_1", "doc_3"].contains(&fuzzy_res[0].doc_id.as_str()));
    }
}
