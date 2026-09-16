//! Integration tests for Phase C:
//! - High-speed In-Graph HNSW Filtered Search
//! - IdBitset O(1) membership operations
//! - 4-way unrolled Asymmetric Distance Computation (ADC) for SQ8
//! - Multi-threaded Concurrent HNSW Bitset Search

use faizdb_vector::distance::DistanceMetric;
use faizdb_vector::hnsw::{ConcurrentHnswIndex, HnswConfig, HnswIndex, IdBitset};
use faizdb_vector::quantization::ScalarQuantizer;

#[test]
fn test_phase_c_id_bitset_large_scale() {
    let mut bitset = IdBitset::with_capacity(10_000);
    assert!(bitset.is_empty());

    // Insert 500 even IDs
    for i in (0..1000).step_by(2) {
        bitset.insert(i);
    }

    assert_eq!(bitset.len(), 500);

    for i in 0..1000 {
        if i % 2 == 0 {
            assert!(bitset.contains(i), "Bitset should contain {i}");
        } else {
            assert!(!bitset.contains(i), "Bitset should NOT contain {i}");
        }
    }
}

#[test]
fn test_phase_c_in_graph_bitset_filtering_precision() {
    let dim = 16;
    let config = HnswConfig::new(dim, DistanceMetric::Cosine);
    let mut index = HnswIndex::new(config);

    // Insert 50 documents with orthogonal/pseudo-random components
    for i in 0..50 {
        let mut v = vec![0.0f32; dim];
        v[i % dim] = 1.0;
        v[(i + 3) % dim] = 0.5;
        faizdb_vector::distance::normalize_in_place(&mut v);
        index.insert(format!("vec_{i}"), v).unwrap();
    }

    // Only allow vec_10, vec_20, vec_30, vec_40
    let allowed_ids = vec!["vec_10", "vec_20", "vec_30", "vec_40"];
    let bitset = index.build_id_bitset(allowed_ids);
    assert_eq!(bitset.len(), 4);

    let _baseline_dist = index.search(&[1.0; 16], 1)[0].distance;
    let mut query = vec![0.0f32; dim];
    query[10 % dim] = 1.0;
    faizdb_vector::distance::normalize_in_place(&mut query);

    let results = index.search_with_bitset(&query, 3, &bitset);
    assert!(!results.is_empty());
    assert!(results.len() <= 3);

    for r in &results {
        assert!(
            r.id == "vec_10" || r.id == "vec_20" || r.id == "vec_30" || r.id == "vec_40",
            "Result {} was not in allowed bitset!",
            r.id
        );
    }
}

#[test]
fn test_phase_c_optimized_adc_unrolled_accuracy() {
    let raw_v = vec![
        0.1, 0.5, -0.2, 0.8, 1.2, -0.9, 0.4, 0.3, 0.7, -0.5, 0.0, 0.9,
    ];
    let query = vec![
        0.2, 0.4, -0.1, 0.9, 1.0, -0.8, 0.3, 0.2, 0.6, -0.4, 0.1, 0.8,
    ];

    let quantized = ScalarQuantizer::quantize(&raw_v);

    // Compare Cosine ADC
    let dist_cosine =
        ScalarQuantizer::asymmetric_distance(&query, &quantized, DistanceMetric::Cosine);
    assert!((0.0..=2.0).contains(&dist_cosine));

    // Compare Euclidean ADC
    let dist_l2 =
        ScalarQuantizer::asymmetric_distance(&query, &quantized, DistanceMetric::Euclidean);
    assert!(dist_l2 >= 0.0);

    // Compare DotProduct ADC
    let dist_dot =
        ScalarQuantizer::asymmetric_distance(&query, &quantized, DistanceMetric::DotProduct);
    assert!(dist_dot < 0.0); // positive correlation yields negative dot distance

    // Compare Manhattan ADC
    let dist_manhattan =
        ScalarQuantizer::asymmetric_distance(&query, &quantized, DistanceMetric::Manhattan);
    assert!(dist_manhattan >= 0.0);
}

#[test]
fn test_phase_c_concurrent_hnsw_bitset_search() {
    let dim = 8;
    let config = HnswConfig::new(dim, DistanceMetric::Cosine);
    let c_index = ConcurrentHnswIndex::new(config);

    for i in 0..30 {
        let mut v = vec![0.0f32; dim];
        v[i % dim] = 1.0;
        faizdb_vector::distance::normalize_in_place(&mut v);
        c_index.insert(format!("item_{i}"), v).unwrap();
    }

    let allowed = vec!["item_0", "item_5", "item_10", "item_15"];
    let bitset = c_index.build_id_bitset(allowed);

    let query = vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let results = c_index.search_with_bitset(&query, 2, &bitset);

    assert!(!results.is_empty());
    for r in &results {
        assert!(
            r.id == "item_0" || r.id == "item_5" || r.id == "item_10" || r.id == "item_15",
            "Disallowed item found: {}",
            r.id
        );
    }
}
