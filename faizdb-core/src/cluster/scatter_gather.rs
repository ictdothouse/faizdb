//! # Distributed Scatter-Gather Query Coordinator
//!
//! Provides partition-aware scatter-gather execution across virtual hash slots.
//! Automatically splits global queries into sub-partition scans, executes them
//! concurrently, and merges sorted result sets, offsets/limits, and columnar aggregations.

use crate::document::model::{Document, Value};
use serde::{Deserialize, Serialize};

/// Definition of a cluster partition slice in the consistent hash ring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterPartition {
    pub partition_id: String,
    pub slot_start: u16,
    pub slot_end: u16,
    pub node_id: String,
}

impl ScatterPartition {
    pub fn new(partition_id: impl Into<String>, slot_start: u16, slot_end: u16, node_id: impl Into<String>) -> Self {
        Self {
            partition_id: partition_id.into(),
            slot_start,
            slot_end,
            node_id: node_id.into(),
        }
    }

    /// Check if a given virtual slot falls within this partition
    pub fn contains_slot(&self, slot: u16) -> bool {
        slot >= self.slot_start && slot <= self.slot_end
    }
}

/// Query specification for Scatter-Gather execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterQuery {
    pub collection: String,
    pub filter_field: Option<String>,
    pub filter_value: Option<Value>,
    pub order_by_field: Option<String>,
    pub is_desc: bool,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub aggregate_field: Option<String>,
}

impl ScatterQuery {
    pub fn new(collection: impl Into<String>) -> Self {
        Self {
            collection: collection.into(),
            filter_field: None,
            filter_value: None,
            order_by_field: None,
            is_desc: false,
            limit: None,
            offset: None,
            aggregate_field: None,
        }
    }

    pub fn with_filter(mut self, field: impl Into<String>, value: Value) -> Self {
        self.filter_field = Some(field.into());
        self.filter_value = Some(value);
        self
    }

    pub fn with_order_by(mut self, field: impl Into<String>, is_desc: bool) -> Self {
        self.order_by_field = Some(field.into());
        self.is_desc = is_desc;
        self
    }

    pub fn with_pagination(mut self, limit: usize, offset: usize) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }

    pub fn with_aggregate(mut self, field: impl Into<String>) -> Self {
        self.aggregate_field = Some(field.into());
        self
    }
}

/// Result returned by an individual partition/shard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionResult {
    pub partition_id: String,
    pub documents: Vec<Document>,
    pub total_count: usize,
    pub sum: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl PartitionResult {
    pub fn empty(partition_id: impl Into<String>) -> Self {
        Self {
            partition_id: partition_id.into(),
            documents: Vec::new(),
            total_count: 0,
            sum: 0.0,
            min: None,
            max: None,
        }
    }
}

/// Fully gathered and merged result across all shards
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedQueryResult {
    pub documents: Vec<Document>,
    pub total_matching: usize,
    pub count: usize,
    pub sum: f64,
    pub avg: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

/// Distributed Scatter-Gather Coordinator
#[derive(Default)]
pub struct ScatterGatherCoordinator;

impl ScatterGatherCoordinator {
    pub fn new() -> Self {
        Self
    }

    /// Scatter a query across partitions. For keyed queries, short-circuits to only relevant partitions.
    pub fn plan_scatter<'a>(
        &self,
        query: &ScatterQuery,
        partitions: &'a [ScatterPartition],
    ) -> Vec<(&'a ScatterPartition, ScatterQuery)> {
        // If query targets a specific document ID, route only to the exact partition
        if let Some(ref field) = query.filter_field {
            if field == "_id" || field == "id" {
                if let Some(Value::String(ref id_str)) = query.filter_value {
                    let slot = crate::cluster::sharding::ShardRouter::calculate_slot(id_str);
                    if let Some(target) = partitions.iter().find(|p| p.contains_slot(slot)) {
                        return vec![(target, query.clone())];
                    }
                }
            }
        }

        // For general scans/aggregations, dispatch to all partitions in parallel
        partitions.iter().map(|p| (p, query.clone())).collect()
    }

    /// Gather, sort, paginate, and merge analytical aggregates from all partition results
    pub fn gather(&self, query: &ScatterQuery, mut results: Vec<PartitionResult>) -> MergedQueryResult {
        let mut all_documents = Vec::new();
        let mut total_matching = 0;
        let mut total_count = 0;
        let mut total_sum = 0.0;
        let mut global_min: Option<f64> = None;
        let mut global_max: Option<f64> = None;

        for res in results.drain(..) {
            total_matching += res.total_count;
            total_count += res.total_count;
            total_sum += res.sum;

            if let Some(part_min) = res.min {
                global_min = Some(match global_min {
                    Some(cur) => cur.min(part_min),
                    None => part_min,
                });
            }

            if let Some(part_max) = res.max {
                global_max = Some(match global_max {
                    Some(cur) => cur.max(part_max),
                    None => part_max,
                });
            }

            all_documents.extend(res.documents);
        }

        // Apply global sorting if requested
        if let Some(ref sort_field) = query.order_by_field {
            let is_desc = query.is_desc;
            all_documents.sort_by(|a, b| {
                let val_a = a.get(sort_field);
                let val_b = b.get(sort_field);
                let cmp = match (val_a, val_b) {
                    (Some(Value::Integer(ia)), Some(Value::Integer(ib))) => ia.cmp(ib),
                    (Some(Value::Float(fa)), Some(Value::Float(fb))) => {
                        fa.partial_cmp(fb).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    (Some(Value::String(sa)), Some(Value::String(sb))) => sa.cmp(sb),
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    _ => std::cmp::Ordering::Equal,
                };
                if is_desc {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }

        // Apply global pagination (offset and limit)
        let offset = query.offset.unwrap_or(0);
        let docs_after_offset: Vec<Document> = all_documents.into_iter().skip(offset).collect();

        let final_documents = if let Some(limit) = query.limit {
            docs_after_offset.into_iter().take(limit).collect()
        } else {
            docs_after_offset
        };

        let avg = if total_count > 0 {
            Some(total_sum / (total_count as f64))
        } else {
            None
        };

        MergedQueryResult {
            documents: final_documents,
            total_matching,
            count: total_count,
            sum: total_sum,
            avg,
            min: global_min,
            max: global_max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scatter_plan_routing() {
        let coordinator = ScatterGatherCoordinator::new();
        let partitions = vec![
            ScatterPartition::new("part_0", 0, 8191, "node_1"),
            ScatterPartition::new("part_1", 8192, 16383, "node_2"),
        ];

        // Keyed lookup routes to single partition
        let keyed_query = ScatterQuery::new("users").with_filter("_id", Value::String("usr_alpha".into()));
        let scatter_tasks = coordinator.plan_scatter(&keyed_query, &partitions);
        assert_eq!(scatter_tasks.len(), 1);

        // General scan routes to all partitions
        let scan_query = ScatterQuery::new("users");
        let scatter_tasks = coordinator.plan_scatter(&scan_query, &partitions);
        assert_eq!(scatter_tasks.len(), 2);
    }

    #[test]
    fn test_gather_sort_and_columnar_aggregates() {
        let coordinator = ScatterGatherCoordinator::new();

        let doc1 = Document::with_id("1").field("score", Value::Integer(90));
        let doc2 = Document::with_id("2").field("score", Value::Integer(100));
        let doc3 = Document::with_id("3").field("score", Value::Integer(80));

        let res_part1 = PartitionResult {
            partition_id: "part_0".into(),
            documents: vec![doc1, doc2],
            total_count: 2,
            sum: 190.0,
            min: Some(90.0),
            max: Some(100.0),
        };

        let res_part2 = PartitionResult {
            partition_id: "part_1".into(),
            documents: vec![doc3],
            total_count: 1,
            sum: 80.0,
            min: Some(80.0),
            max: Some(80.0),
        };

        let query = ScatterQuery::new("users")
            .with_order_by("score", true) // DESC
            .with_pagination(2, 0); // Limit 2

        let merged = coordinator.gather(&query, vec![res_part1, res_part2]);

        assert_eq!(merged.total_matching, 3);
        assert_eq!(merged.count, 3);
        assert_eq!(merged.sum, 270.0);
        assert_eq!(merged.avg, Some(90.0));
        assert_eq!(merged.min, Some(80.0));
        assert_eq!(merged.max, Some(100.0));

        // Documents should be top-2 sorted descending: 100, then 90
        assert_eq!(merged.documents.len(), 2);
        assert_eq!(merged.documents[0].get("score"), Some(&Value::Integer(100)));
        assert_eq!(merged.documents[1].get("score"), Some(&Value::Integer(90)));
    }
}
