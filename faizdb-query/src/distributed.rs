//! # Distributed Scatter-Gather Query Aggregator for Multi-Shard Clusters
//!
//! Enables parallel query execution across 16,384 virtual hash slots on $N$ cluster nodes:
//! - **Predicate Pushdown:** Pushes filter conditions (`WHERE`) down to each individual shard node.
//! - **Partial Aggregation:** Each shard computes local intermediate sums, counts, min/max in parallel.
//! - **Coordinator Reduction:** The receiving coordinator node reduces partial responses into a final unified result set.

use serde::{Deserialize, Serialize};

/// An individual shard target in the cluster
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShardTarget {
    pub node_id: String,
    pub endpoint: String,
    pub slot_range: (u16, u16),
}

/// Physical execution strategy for distributed joins across cluster shards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DistributedJoinStrategy {
    /// Zero network data transfer: Entities share the exact same hash slot tag {...}
    ColocatedHashJoin,
    /// Coordinator streams the smaller dimension table to all participating shards
    BroadcastHashJoin,
    /// Graceful degradation fallback when dimension table exceeds broadcast threshold:
    /// executes micro-batch index lookups against the target shards
    DistributedIndexNestedLoopFallback,
}

/// A distributed scatter-gather execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterGatherPlan {
    pub raw_query: String,
    pub collection: String,
    pub target_shards: Vec<ShardTarget>,
    pub pushdown_filter: Option<String>,
    pub aggregation_type: Option<AggregationOp>,
    pub broadcast_payload: Option<Vec<serde_json::Value>>,
    pub join_strategy: Option<DistributedJoinStrategy>,
}

/// Aggregation operations pushed down to shards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AggregationOp {
    Count,
    Sum,
    Avg,
    Min,
    Max,
}

/// Intermediate partial aggregation result returned by an individual shard
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShardPartialResult {
    pub node_id: String,
    pub count: u64,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub rows: Vec<serde_json::Value>,
    pub execution_time_us: u64,
}

/// Unified final result reduced by coordinator
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DistributedQueryResult {
    pub total_rows: usize,
    pub count: u64,
    pub sum: Option<f64>,
    pub avg: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub rows: Vec<serde_json::Value>,
    pub participating_shards: usize,
    pub total_execution_time_us: u64,
}

/// Default maximum size for broadcasting dimension tables (10 MB)
pub const DEFAULT_BROADCAST_THRESHOLD_BYTES: usize = 10 * 1024 * 1024;

/// Coordinator responsible for routing and reducing scatter-gather queries
#[derive(Debug)]
pub struct DistributedQueryCoordinator {
    shards: Vec<ShardTarget>,
    pub broadcast_threshold_bytes: usize,
}

impl Default for DistributedQueryCoordinator {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl DistributedQueryCoordinator {
    /// Create a new coordinator with known cluster shard topology and default 10MB broadcast threshold
    pub fn new(shards: Vec<ShardTarget>) -> Self {
        Self {
            shards,
            broadcast_threshold_bytes: DEFAULT_BROADCAST_THRESHOLD_BYTES,
        }
    }

    /// Set a custom broadcast threshold in bytes
    pub fn with_broadcast_threshold(mut self, threshold_bytes: usize) -> Self {
        self.broadcast_threshold_bytes = threshold_bytes;
        self
    }

    /// Evaluate and plan the distributed join physical execution strategy.
    ///
    /// - If `is_colocated_key` is true: selects `ColocatedHashJoin` (Zero network transfer).
    /// - If `dimension_table_bytes <= broadcast_threshold_bytes`: selects `BroadcastHashJoin`.
    /// - If `dimension_table_bytes > broadcast_threshold_bytes`: triggers circuit-breaker fallback
    ///   to `DistributedIndexNestedLoopFallback` (graceful degradation via micro-batching).
    pub fn plan_distributed_join(
        &self,
        dimension_table_bytes: usize,
        is_colocated_key: bool,
    ) -> DistributedJoinStrategy {
        if is_colocated_key {
            DistributedJoinStrategy::ColocatedHashJoin
        } else if dimension_table_bytes <= self.broadcast_threshold_bytes {
            DistributedJoinStrategy::BroadcastHashJoin
        } else {
            DistributedJoinStrategy::DistributedIndexNestedLoopFallback
        }
    }

    /// Add a shard node to the topology
    pub fn register_shard(&mut self, shard: ShardTarget) {
        self.shards.push(shard);
    }

    /// Plan a distributed scatter-gather query
    pub fn create_plan(
        &self,
        collection: &str,
        raw_query: &str,
        aggregation: Option<AggregationOp>,
    ) -> ScatterGatherPlan {
        ScatterGatherPlan {
            raw_query: raw_query.to_string(),
            collection: collection.to_string(),
            target_shards: self.shards.clone(),
            pushdown_filter: None,
            aggregation_type: aggregation,
            broadcast_payload: None,
            join_strategy: None,
        }
    }

    /// Reduce partial shard results into a unified final result
    pub fn reduce_results(
        &self,
        partial_results: Vec<ShardPartialResult>,
        aggregation: Option<AggregationOp>,
    ) -> DistributedQueryResult {
        let participating_shards = partial_results.len();
        let mut total_count = 0u64;
        let mut total_sum = 0.0f64;
        let mut global_min = f64::INFINITY;
        let mut global_max = f64::NEG_INFINITY;
        let mut combined_rows = Vec::new();
        let mut max_shard_time_us = 0u64;

        for part in partial_results {
            total_count += part.count;
            total_sum += part.sum;
            if part.min < global_min {
                global_min = part.min;
            }
            if part.max > global_max {
                global_max = part.max;
            }
            max_shard_time_us = max_shard_time_us.max(part.execution_time_us);
            combined_rows.extend(part.rows);
        }

        let total_rows = combined_rows.len();

        let (sum, avg, min, max) = match aggregation {
            Some(AggregationOp::Count) => (None, None, None, None),
            Some(AggregationOp::Sum) => (Some(total_sum), None, None, None),
            Some(AggregationOp::Avg) => {
                let calculated_avg = if total_count > 0 {
                    total_sum / (total_count as f64)
                } else {
                    0.0
                };
                (Some(total_sum), Some(calculated_avg), None, None)
            }
            Some(AggregationOp::Min) => (
                None,
                None,
                Some(if global_min.is_infinite() {
                    0.0
                } else {
                    global_min
                }),
                None,
            ),
            Some(AggregationOp::Max) => (
                None,
                None,
                None,
                Some(if global_max.is_infinite() {
                    0.0
                } else {
                    global_max
                }),
            ),
            None => (None, None, None, None),
        };

        DistributedQueryResult {
            total_rows,
            count: total_count,
            sum,
            avg,
            min,
            max,
            rows: combined_rows,
            participating_shards,
            total_execution_time_us: max_shard_time_us,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distributed_scatter_gather_reduction() {
        let shard1 = ShardTarget {
            node_id: "node-1".to_string(),
            endpoint: "http://10.0.0.1:27018".to_string(),
            slot_range: (0, 5461),
        };
        let shard2 = ShardTarget {
            node_id: "node-2".to_string(),
            endpoint: "http://10.0.0.2:27018".to_string(),
            slot_range: (5462, 10922),
        };
        let shard3 = ShardTarget {
            node_id: "node-3".to_string(),
            endpoint: "http://10.0.0.3:27018".to_string(),
            slot_range: (10923, 16383),
        };

        let coordinator = DistributedQueryCoordinator::new(vec![shard1, shard2, shard3]);

        // Simulate 3 shard worker responses for a distributed SUM/AVG query
        let partial_results = vec![
            ShardPartialResult {
                node_id: "node-1".to_string(),
                count: 100,
                sum: 5000.0,
                min: 10.0,
                max: 95.0,
                rows: vec![serde_json::json!({"id": 1, "score": 50})],
                execution_time_us: 120,
            },
            ShardPartialResult {
                node_id: "node-2".to_string(),
                count: 150,
                sum: 7500.0,
                min: 5.0,
                max: 100.0,
                rows: vec![serde_json::json!({"id": 2, "score": 80})],
                execution_time_us: 150,
            },
            ShardPartialResult {
                node_id: "node-3".to_string(),
                count: 250,
                sum: 12500.0,
                min: 1.0,
                max: 99.0,
                rows: vec![serde_json::json!({"id": 3, "score": 90})],
                execution_time_us: 110,
            },
        ];

        let final_result = coordinator.reduce_results(partial_results, Some(AggregationOp::Avg));

        assert_eq!(final_result.participating_shards, 3);
        assert_eq!(final_result.count, 500); // 100 + 150 + 250
        assert_eq!(final_result.sum, Some(25000.0)); // 5000 + 7500 + 12500
        assert_eq!(final_result.avg, Some(50.0)); // 25000 / 500
        assert_eq!(final_result.total_rows, 3);
    }

    #[test]
    fn test_cbo_distributed_join_planning_and_circuit_breaker() {
        let coordinator = DistributedQueryCoordinator::new(vec![]);

        // Case 1: Colocated key with hash tags {...} -> zero network transfer
        let colocated_plan = coordinator.plan_distributed_join(50 * 1024 * 1024, true);
        assert_eq!(colocated_plan, DistributedJoinStrategy::ColocatedHashJoin);

        // Case 2: Under 10 MB threshold (e.g., 4 MB dimension table) -> Broadcast Join
        let small_dim = 4 * 1024 * 1024;
        let broadcast_plan = coordinator.plan_distributed_join(small_dim, false);
        assert_eq!(broadcast_plan, DistributedJoinStrategy::BroadcastHashJoin);

        // Case 3: Exactly at 10 MB threshold -> Broadcast Join
        let exact_dim = 10 * 1024 * 1024;
        let exact_plan = coordinator.plan_distributed_join(exact_dim, false);
        assert_eq!(exact_plan, DistributedJoinStrategy::BroadcastHashJoin);

        // Case 4: Exceeds 10 MB threshold (e.g., 15 MB) -> Circuit Breaker trips to Distributed Index-Nested-Loop fallback
        let large_dim = 15 * 1024 * 1024;
        let fallback_plan = coordinator.plan_distributed_join(large_dim, false);
        assert_eq!(
            fallback_plan,
            DistributedJoinStrategy::DistributedIndexNestedLoopFallback
        );

        // Case 5: Custom threshold via builder pattern
        let custom_coord = coordinator.with_broadcast_threshold(2 * 1024 * 1024); // 2 MB
        assert_eq!(
            custom_coord.plan_distributed_join(3 * 1024 * 1024, false),
            DistributedJoinStrategy::DistributedIndexNestedLoopFallback
        );
        assert_eq!(
            custom_coord.plan_distributed_join(1 * 1024 * 1024, false),
            DistributedJoinStrategy::BroadcastHashJoin
        );
    }
}

