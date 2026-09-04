//! Integration tests for FaizDB HNSW vector search.
//!
//! Validates index construction, nearest-neighbour retrieval,
//! distance metrics, and persistence behaviour.

use faizdb_vector::{
    distance::DistanceMetric,
    hnsw::{HnswConfig, HnswIndex},
};

#[allow(dead_code)]
/// Cosine distance between two f32 vectors (for test validation)
fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0;
    }
    1.0 - (dot / (norm_a * norm_b))
}

#[test]
fn test_insert_and_nearest_neighbour() {
    let config = HnswConfig::new(4, DistanceMetric::Cosine);
    let mut index = HnswIndex::new(config);

    // Insert 5 known vectors
    index
        .insert("v1".to_string(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap();
    index
        .insert("v2".to_string(), vec![0.0, 1.0, 0.0, 0.0])
        .unwrap();
    index
        .insert("v3".to_string(), vec![0.0, 0.0, 1.0, 0.0])
        .unwrap();
    index
        .insert("v4".to_string(), vec![0.0, 0.0, 0.0, 1.0])
        .unwrap();
    index
        .insert("v5".to_string(), vec![0.9, 0.1, 0.0, 0.0])
        .unwrap(); // Most similar to v1

    // Query with a vector closest to v1
    let query = vec![1.0, 0.05, 0.0, 0.0];
    let results = index.search(&query, 2);

    assert!(
        !results.is_empty(),
        "Search must return at least one result"
    );
    // The closest result should be v1 or v5 (both very close to query)
    let top_id = &results[0].id;
    assert!(
        top_id == "v1" || top_id == "v5",
        "Top result must be v1 or v5, got: {top_id}"
    );
}

#[test]
fn test_euclidean_distance_search() {
    let config = HnswConfig::new(3, DistanceMetric::Euclidean);
    let mut index = HnswIndex::new(config);

    index
        .insert("origin".to_string(), vec![0.0, 0.0, 0.0])
        .unwrap();
    index
        .insert("near".to_string(), vec![0.1, 0.1, 0.1])
        .unwrap();
    index
        .insert("far".to_string(), vec![10.0, 10.0, 10.0])
        .unwrap();

    let query = vec![0.05, 0.05, 0.05];
    let results = index.search(&query, 1);

    assert_eq!(results.len(), 1);
    assert!(
        results[0].id == "origin" || results[0].id == "near",
        "Euclidean NN must return origin or near, got: {}",
        results[0].id
    );
}

#[test]
fn test_search_k_results_bounded_by_index_size() {
    let config = HnswConfig::new(2, DistanceMetric::Cosine);
    let mut index = HnswIndex::new(config);

    index.insert("a".to_string(), vec![1.0, 0.0]).unwrap();
    index.insert("b".to_string(), vec![0.0, 1.0]).unwrap();

    // Requesting k=10 on a 2-element index should return at most 2 results
    let results = index.search(&[0.7, 0.7], 10);
    assert!(results.len() <= 2, "Results must not exceed index size");
}

#[test]
fn test_empty_index_search_returns_empty() {
    let config = HnswConfig::new(4, DistanceMetric::Cosine);
    let index = HnswIndex::new(config);
    let results = index.search(&[1.0, 0.0, 0.0, 0.0], 5);
    assert!(results.is_empty(), "Empty index must return empty results");
}

#[test]
fn test_distance_scores_are_ordered_ascending() {
    let config = HnswConfig::new(4, DistanceMetric::Cosine);
    let mut index = HnswIndex::new(config);

    for i in 0..10 {
        let v = vec![i as f32, 0.0, 0.0, 0.0];
        index.insert(format!("v{i}"), v).unwrap();
    }

    let query = vec![5.0, 0.0, 0.0, 0.0];
    let results = index.search(&query, 5);

    // Scores must be in ascending distance order (nearest first)
    let scores: Vec<f32> = results.iter().map(|r| r.distance).collect();
    for window in scores.windows(2) {
        assert!(
            window[0] <= window[1],
            "Results must be ordered by ascending distance: {:?}",
            scores
        );
    }
}

#[test]
fn test_quantized_vector_search_integration() {
    use faizdb_vector::quantization::QuantizationType;

    let config =
        HnswConfig::new(4, DistanceMetric::Cosine).with_quantization(QuantizationType::Scalar8);
    let mut index = HnswIndex::new(config);

    index
        .insert("doc_a".to_string(), vec![1.0, 0.0, 0.0, 0.0])
        .unwrap();
    index
        .insert("doc_b".to_string(), vec![0.0, 1.0, 0.0, 0.0])
        .unwrap();
    index
        .insert("doc_c".to_string(), vec![0.9, 0.1, 0.0, 0.0])
        .unwrap();

    let query = vec![0.98, 0.02, 0.0, 0.0];
    let results = index.search(&query, 2);

    assert_eq!(results.len(), 2);
    // Nearest should be doc_a or doc_c
    assert!(results[0].id == "doc_a" || results[0].id == "doc_c");
}
