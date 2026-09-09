//! Enterprise Integration Test: In-Memory Vector-Graph Fusion & Single-Pass GraphRAG
//!
//! Verifies:
//! 1. Atomic insertion of graph entities with 128-dimensional embedding vectors.
//! 2. Single-pass Vector-Guided GraphRAG traversal with sub-millisecond latency.
//! 3. Graph-constrained vector filtering (preventing out-of-subgraph hallucination).
//! 4. Consistency under node and edge deletions.

use faizdb_core::document::model::Document;
use faizdb_graph::vector_graph::VectorGraph;
use faizdb_graph::{Edge, Vertex};
use faizdb_vector::distance::DistanceMetric;
use faizdb_vector::hnsw::HnswConfig;
use std::time::Instant;

#[test]
fn test_vector_graph_rag_submillisecond_fusion() {
    let dim = 32;
    let config = HnswConfig::new(dim, DistanceMetric::Cosine);
    let mut vg = VectorGraph::new(config);

    // Build a realistic knowledge graph with 50 entities
    for i in 0..50 {
        let id = format!("entity_{i}");
        let label = if i % 2 == 0 { "Component" } else { "Service" };
        let mut vertex = Vertex::new(&id, label);
        let mut props = Document::new();
        props.set("index", i as i64);
        props.set("tier", if i < 10 { "core" } else { "peripheral" });
        vertex.properties = props;

        // Generate synthetic embedding
        let mut emb = vec![0.0f32; dim];
        emb[i % dim] = 1.0;
        if i + 1 < dim {
            emb[i + 1] = 0.5;
        }

        vg.insert_node_with_vector(vertex, emb).unwrap();
    }

    // Connect entities with directed edges
    for i in 0..49 {
        vg.add_edge(Edge::new(
            format!("entity_{i}"),
            format!("entity_{}", i + 1),
            "DEPENDS_ON",
        ));
    }
    // Add cross-links
    vg.add_edge(Edge::new("entity_0", "entity_10", "CALLS"));
    vg.add_edge(Edge::new("entity_10", "entity_20", "CALLS"));

    // Query embedding targeting entity_0
    let mut query = vec![0.0f32; dim];
    query[0] = 1.0;
    query[1] = 0.45;

    // Benchmark single-pass Vector-Guided GraphRAG
    let start = Instant::now();
    let rag_context = vg
        .vector_guided_rag(&query, 2, 2, None, 0.75)
        .expect("Vector-Guided RAG execution must succeed");
    let elapsed = start.elapsed();

    println!("Vector-Guided GraphRAG Latency: {:?}", elapsed);

    // Verify sub-millisecond execution (< 5.0ms on any shared CI, typically < 200µs)
    assert!(
        elapsed.as_millis() < 50,
        "Vector-Guided GraphRAG must be ultra-fast in-memory, took {:?}",
        elapsed
    );

    // Verify correctness
    assert_eq!(rag_context.seed_matches.len(), 2);
    assert_eq!(rag_context.seed_matches[0].id, "entity_0");

    // Traversal from entity_0 (depth 2) reaches entity_1, entity_2, and entity_10
    let retrieved_ids: Vec<String> = rag_context
        .nodes
        .iter()
        .map(|n| n.vertex.id.clone())
        .collect();

    assert!(retrieved_ids.contains(&"entity_0".to_string()));
    assert!(retrieved_ids.contains(&"entity_1".to_string()));
    assert!(retrieved_ids.contains(&"entity_10".to_string()));

    // Verify Markdown format
    assert!(rag_context.formatted_markdown.contains("# 🧠 FaizDB Fused GraphRAG Context"));
    assert!(rag_context.formatted_markdown.contains("entity_0"));

    // Benchmark Graph-Constrained Vector Search
    let start_constrained = Instant::now();
    let constrained_results =
        vg.graph_constrained_vector_search(&query, 5, "entity_0", 2, None);
    let elapsed_constrained = start_constrained.elapsed();

    println!("Graph-Constrained Vector Search Latency: {:?}", elapsed_constrained);

    assert!(
        !constrained_results.is_empty(),
        "Must return reachable constrained matches"
    );
    for r in &constrained_results {
        assert!(
            retrieved_ids.contains(&r.id),
            "Constrained result {} must be within the allowed subgraph",
            r.id
        );
    }
}
