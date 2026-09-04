//! # Prometheus & OpenTelemetry Metrics Exporter
//!
//! Provides the standard Prometheus `GET /metrics` text exposition endpoint
//! for real-time observability in Grafana, Datadog, and Kubernetes monitoring stacks,
//! as well as structured JSON profiling endpoints.

use axum::extract::State;
use axum::http::header::CONTENT_TYPE;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::api::middleware::AppState;

/// Global atomic telemetry counters and histogram buckets
pub struct MetricsCollector {
    start_time: Instant,
    pub inserts_total: AtomicU64,
    pub queries_total: AtomicU64,
    pub updates_total: AtomicU64,
    pub deletes_total: AtomicU64,
    pub vector_searches_total: AtomicU64,
    pub bytes_written_total: AtomicU64,
    pub bytes_read_total: AtomicU64,
    pub wal_syncs_total: AtomicU64,
    pub active_connections: AtomicU64,
    pub cache_hits_total: AtomicU64,
    pub cache_misses_total: AtomicU64,

    // Latency histogram buckets (microseconds)
    // <= 100µs, <= 500µs, <= 1ms, <= 5ms, <= 10ms, <= 50ms, <= 100ms, +Inf
    pub bucket_100us: AtomicU64,
    pub bucket_500us: AtomicU64,
    pub bucket_1ms: AtomicU64,
    pub bucket_5ms: AtomicU64,
    pub bucket_10ms: AtomicU64,
    pub bucket_50ms: AtomicU64,
    pub bucket_100ms: AtomicU64,
    pub bucket_inf: AtomicU64,
    pub query_duration_sum_us: AtomicU64,
    pub query_duration_count: AtomicU64,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            inserts_total: AtomicU64::new(0),
            queries_total: AtomicU64::new(0),
            updates_total: AtomicU64::new(0),
            deletes_total: AtomicU64::new(0),
            vector_searches_total: AtomicU64::new(0),
            bytes_written_total: AtomicU64::new(0),
            bytes_read_total: AtomicU64::new(0),
            wal_syncs_total: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            cache_hits_total: AtomicU64::new(0),
            cache_misses_total: AtomicU64::new(0),
            bucket_100us: AtomicU64::new(0),
            bucket_500us: AtomicU64::new(0),
            bucket_1ms: AtomicU64::new(0),
            bucket_5ms: AtomicU64::new(0),
            bucket_10ms: AtomicU64::new(0),
            bucket_50ms: AtomicU64::new(0),
            bucket_100ms: AtomicU64::new(0),
            bucket_inf: AtomicU64::new(0),
            query_duration_sum_us: AtomicU64::new(0),
            query_duration_count: AtomicU64::new(0),
        }
    }
}

impl MetricsCollector {
    /// Record latency observation in microseconds
    pub fn record_query_latency(&self, duration: Duration) {
        let us = duration.as_micros() as u64;
        self.query_duration_sum_us.fetch_add(us, Ordering::Relaxed);
        self.query_duration_count.fetch_add(1, Ordering::Relaxed);

        if us <= 100 {
            self.bucket_100us.fetch_add(1, Ordering::Relaxed);
        }
        if us <= 500 {
            self.bucket_500us.fetch_add(1, Ordering::Relaxed);
        }
        if us <= 1_000 {
            self.bucket_1ms.fetch_add(1, Ordering::Relaxed);
        }
        if us <= 5_000 {
            self.bucket_5ms.fetch_add(1, Ordering::Relaxed);
        }
        if us <= 10_000 {
            self.bucket_10ms.fetch_add(1, Ordering::Relaxed);
        }
        if us <= 50_000 {
            self.bucket_50ms.fetch_add(1, Ordering::Relaxed);
        }
        if us <= 100_000 {
            self.bucket_100ms.fetch_add(1, Ordering::Relaxed);
        }
        self.bucket_inf.fetch_add(1, Ordering::Relaxed);
    }

    /// Render current metrics into Prometheus Text Format (version 0.0.4)
    pub fn render_prometheus(&self) -> String {
        let uptime_sec = self.start_time.elapsed().as_secs();
        let inserts = self.inserts_total.load(Ordering::Relaxed);
        let queries = self.queries_total.load(Ordering::Relaxed);
        let updates = self.updates_total.load(Ordering::Relaxed);
        let deletes = self.deletes_total.load(Ordering::Relaxed);
        let vector_searches = self.vector_searches_total.load(Ordering::Relaxed);
        let bytes_written = self.bytes_written_total.load(Ordering::Relaxed);
        let bytes_read = self.bytes_read_total.load(Ordering::Relaxed);
        let wal_syncs = self.wal_syncs_total.load(Ordering::Relaxed);
        let conns = self.active_connections.load(Ordering::Relaxed);
        let hits = self.cache_hits_total.load(Ordering::Relaxed);
        let misses = self.cache_misses_total.load(Ordering::Relaxed);

        let hit_ratio = if hits + misses == 0 {
            1.0
        } else {
            hits as f64 / (hits + misses) as f64
        };

        // Histogram cumulative counts
        let b100 = self.bucket_100us.load(Ordering::Relaxed);
        let b500 = self.bucket_500us.load(Ordering::Relaxed);
        let b1m = self.bucket_1ms.load(Ordering::Relaxed);
        let b5m = self.bucket_5ms.load(Ordering::Relaxed);
        let b10m = self.bucket_10ms.load(Ordering::Relaxed);
        let b50m = self.bucket_50ms.load(Ordering::Relaxed);
        let b100m = self.bucket_100ms.load(Ordering::Relaxed);
        let b_inf = self.bucket_inf.load(Ordering::Relaxed);
        let sum_sec = (self.query_duration_sum_us.load(Ordering::Relaxed) as f64) / 1_000_000.0;
        let count = self.query_duration_count.load(Ordering::Relaxed);

        format!(
            "# HELP faizdb_uptime_seconds FaizDB process uptime in seconds\n\
             # TYPE faizdb_uptime_seconds gauge\n\
             faizdb_uptime_seconds {}\n\n\
             # HELP faizdb_operations_total Total database operations processed by type\n\
             # TYPE faizdb_operations_total counter\n\
             faizdb_operations_total{{op=\"insert\"}} {}\n\
             faizdb_operations_total{{op=\"query\"}} {}\n\
             faizdb_operations_total{{op=\"update\"}} {}\n\
             faizdb_operations_total{{op=\"delete\"}} {}\n\
             faizdb_operations_total{{op=\"vector_search\"}} {}\n\n\
             # HELP faizdb_io_bytes_total Total disk and network bytes processed\n\
             # TYPE faizdb_io_bytes_total counter\n\
             faizdb_io_bytes_total{{direction=\"write\"}} {}\n\
             faizdb_io_bytes_total{{direction=\"read\"}} {}\n\n\
             # HELP faizdb_wal_syncs_total Total write-ahead log fsync flushes\n\
             # TYPE faizdb_wal_syncs_total counter\n\
             faizdb_wal_syncs_total {}\n\n\
             # HELP faizdb_active_connections Current active client connections\n\
             # TYPE faizdb_active_connections gauge\n\
             faizdb_active_connections {}\n\n\
             # HELP faizdb_cache_hit_ratio Storage cache hit ratio\n\
             # TYPE faizdb_cache_hit_ratio gauge\n\
             faizdb_cache_hit_ratio {:.4}\n\n\
             # HELP faizdb_query_duration_seconds Query latency histogram in seconds\n\
             # TYPE faizdb_query_duration_seconds histogram\n\
             faizdb_query_duration_seconds_bucket{{le=\"0.0001\"}} {}\n\
             faizdb_query_duration_seconds_bucket{{le=\"0.0005\"}} {}\n\
             faizdb_query_duration_seconds_bucket{{le=\"0.001\"}} {}\n\
             faizdb_query_duration_seconds_bucket{{le=\"0.005\"}} {}\n\
             faizdb_query_duration_seconds_bucket{{le=\"0.01\"}} {}\n\
             faizdb_query_duration_seconds_bucket{{le=\"0.05\"}} {}\n\
             faizdb_query_duration_seconds_bucket{{le=\"0.1\"}} {}\n\
             faizdb_query_duration_seconds_bucket{{le=\"+Inf\"}} {}\n\
             faizdb_query_duration_seconds_sum {:.6}\n\
             faizdb_query_duration_seconds_count {}\n",
            uptime_sec,
            inserts,
            queries,
            updates,
            deletes,
            vector_searches,
            bytes_written,
            bytes_read,
            wal_syncs,
            conns,
            hit_ratio,
            b100,
            b500,
            b1m,
            b5m,
            b10m,
            b50m,
            b100m,
            b_inf,
            sum_sec,
            count
        )
    }
}

/// Global shared collector instance
pub type SharedMetrics = Arc<MetricsCollector>;

/// GET /metrics and /v1/metrics handler wired to live server state
pub async fn metrics_handler(State(state): State<Arc<AppState>>) -> Response {
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        state.metrics.render_prometheus(),
    )
        .into_response()
}

/// GET /v1/system/profile handler returning runtime observability JSON
pub async fn system_profile_handler(State(state): State<Arc<AppState>>) -> Response {
    let raft_info = state.db.raft().get_info();
    let col_count = state.db.list_collections().len();
    let uptime_sec = state.metrics.start_time.elapsed().as_secs();

    let profile = json!({
        "status": "online",
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_seconds": uptime_sec,
        "collections_count": col_count,
        "cluster": {
            "node_id": raft_info.node_id,
            "role": format!("{:?}", raft_info.role).to_lowercase(),
            "term": raft_info.term,
            "is_leader": raft_info.is_leader,
            "commit_index": raft_info.commit_index,
            "peers": raft_info.peer_count,
            "quorum_size": raft_info.quorum_size,
        },
        "telemetry": {
            "active_connections": state.metrics.active_connections.load(Ordering::Relaxed),
            "inserts_total": state.metrics.inserts_total.load(Ordering::Relaxed),
            "queries_total": state.metrics.queries_total.load(Ordering::Relaxed),
            "vector_searches_total": state.metrics.vector_searches_total.load(Ordering::Relaxed),
            "bytes_written": state.metrics.bytes_written_total.load(Ordering::Relaxed),
            "bytes_read": state.metrics.bytes_read_total.load(Ordering::Relaxed),
            "wal_syncs": state.metrics.wal_syncs_total.load(Ordering::Relaxed),
            "cache_hit_ratio": {
                "hits": state.metrics.cache_hits_total.load(Ordering::Relaxed),
                "misses": state.metrics.cache_misses_total.load(Ordering::Relaxed),
            },
            "query_latency": {
                "total_queries_measured": state.metrics.query_duration_count.load(Ordering::Relaxed),
                "total_duration_us": state.metrics.query_duration_sum_us.load(Ordering::Relaxed),
                "avg_duration_us": if state.metrics.query_duration_count.load(Ordering::Relaxed) > 0 {
                    state.metrics.query_duration_sum_us.load(Ordering::Relaxed) / state.metrics.query_duration_count.load(Ordering::Relaxed)
                } else {
                    0
                }
            }
        }
    });

    ([(CONTENT_TYPE, "application/json")], profile.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prometheus_rendering_and_histograms() {
        let collector = MetricsCollector::default();
        collector.inserts_total.fetch_add(50, Ordering::Relaxed);
        collector.queries_total.fetch_add(150, Ordering::Relaxed);
        collector
            .vector_searches_total
            .fetch_add(30, Ordering::Relaxed);
        collector.cache_hits_total.fetch_add(90, Ordering::Relaxed);
        collector
            .cache_misses_total
            .fetch_add(10, Ordering::Relaxed);

        collector.record_query_latency(Duration::from_micros(250));
        collector.record_query_latency(Duration::from_micros(2500));

        let rendered = collector.render_prometheus();
        assert!(rendered.contains("faizdb_uptime_seconds"));
        assert!(rendered.contains("faizdb_operations_total{op=\"insert\"} 50"));
        assert!(rendered.contains("faizdb_operations_total{op=\"query\"} 150"));
        assert!(rendered.contains("faizdb_cache_hit_ratio 0.9000"));
        assert!(rendered.contains("faizdb_query_duration_seconds_bucket{le=\"0.0005\"} 1"));
        assert!(rendered.contains("faizdb_query_duration_seconds_bucket{le=\"0.005\"} 2"));
        assert!(rendered.contains("faizdb_query_duration_seconds_count 2"));
    }
}
