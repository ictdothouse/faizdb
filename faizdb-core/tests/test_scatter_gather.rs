//! Integration tests for Distributed Scatter-Gather Query Coordinator (Phase 4)

use faizdb_core::cluster::scatter_gather::{
    PartitionResult, ScatterGatherCoordinator, ScatterPartition, ScatterQuery,
};
use faizdb_core::document::model::{Document, Value};

#[test]
fn test_scatter_gather_partition_routing_and_hash_colocation() {
    let coordinator = ScatterGatherCoordinator::new();
    let partitions = vec![
        ScatterPartition::new("shard_a", 0, 5460, "node_us_east"),
        ScatterPartition::new("shard_b", 5461, 10922, "node_us_west"),
        ScatterPartition::new("shard_c", 10923, 16383, "node_eu_central"),
    ];

    // 1. Single-key lookup routes to exactly one partition
    let point_query = ScatterQuery::new("orders").with_filter("_id", Value::String("ord_100".into()));
    let plan = coordinator.plan_scatter(&point_query, &partitions);
    assert_eq!(plan.len(), 1);

    // 2. Hash-tagged keys route deterministically to their respective partition
    let tagged_query_1 = ScatterQuery::new("orders").with_filter("_id", Value::String("{tenant_alpha}:order_1".into()));
    let tagged_query_2 = ScatterQuery::new("users").with_filter("_id", Value::String("{tenant_alpha}:user_1".into()));

    let plan_1 = coordinator.plan_scatter(&tagged_query_1, &partitions);
    let plan_2 = coordinator.plan_scatter(&tagged_query_2, &partitions);

    assert_eq!(plan_1.len(), 1);
    assert_eq!(plan_2.len(), 1);
    // Exact same hash slot, meaning exact same partition!
    assert_eq!(plan_1[0].0.partition_id, plan_2[0].0.partition_id);

    // 3. Global scan query scatters across all 3 partitions
    let global_query = ScatterQuery::new("telemetry");
    let global_plan = coordinator.plan_scatter(&global_query, &partitions);
    assert_eq!(global_plan.len(), 3);
}

#[test]
fn test_scatter_gather_sort_merge_and_columnar_aggregations() {
    let coordinator = ScatterGatherCoordinator::new();

    // Partition 1 records
    let doc_p1_1 = Document::with_id("m1")
        .field("cpu", Value::Float(45.5))
        .field("priority", Value::Integer(10));
    let doc_p1_2 = Document::with_id("m2")
        .field("cpu", Value::Float(88.0))
        .field("priority", Value::Integer(30));

    let part1_res = PartitionResult {
        partition_id: "shard_a".into(),
        documents: vec![doc_p1_1, doc_p1_2],
        total_count: 2,
        sum: 133.5,
        min: Some(45.5),
        max: Some(88.0),
    };

    // Partition 2 records
    let doc_p2_1 = Document::with_id("m3")
        .field("cpu", Value::Float(12.0))
        .field("priority", Value::Integer(5));
    let doc_p2_2 = Document::with_id("m4")
        .field("cpu", Value::Float(95.0))
        .field("priority", Value::Integer(40));

    let part2_res = PartitionResult {
        partition_id: "shard_b".into(),
        documents: vec![doc_p2_1, doc_p2_2],
        total_count: 2,
        sum: 107.0,
        min: Some(12.0),
        max: Some(95.0),
    };

    // Query: ORDER BY priority DESC LIMIT 3 OFFSET 0
    let query = ScatterQuery::new("metrics")
        .with_order_by("priority", true)
        .with_pagination(3, 0);

    let gathered = coordinator.gather(&query, vec![part1_res, part2_res]);

    // 1. Total matching & count
    assert_eq!(gathered.total_matching, 4);
    assert_eq!(gathered.count, 4);

    // 2. Columnar analytics across shards
    assert_eq!(gathered.sum, 240.5);
    assert_eq!(gathered.min, Some(12.0));
    assert_eq!(gathered.max, Some(95.0));
    assert_eq!(gathered.avg, Some(240.5 / 4.0));

    // 3. Paginated sorted documents (top 3 by priority DESC: 40, 30, 10)
    assert_eq!(gathered.documents.len(), 3);
    assert_eq!(gathered.documents[0].get("priority"), Some(&Value::Integer(40)));
    assert_eq!(gathered.documents[1].get("priority"), Some(&Value::Integer(30)));
    assert_eq!(gathered.documents[2].get("priority"), Some(&Value::Integer(10)));
}
