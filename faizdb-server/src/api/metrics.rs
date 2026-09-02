//! # Prometheus & OpenTelemetry Metrics Exporter
//!
//! Provides the standard Prometheus `GET /metrics` text exposition endpoint
//! for real-time observability in Grafana, Datadog, and Kubernetes monitoring stacks.

use axum::response::{IntoResponse, Response};
use axum::http::header::CONTENT_TYPE;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Global atomic telemetry counters
pub struct MetricsCollector {
    start_time: Instant,
    pub inserts_total: AtomicU64,
    pub queries_total: AtomicU64,
    pub vector_searches_total: AtomicU64,
    pub active_connections: AtomicU64,
    pub cache_hits_total: AtomicU64,
    pub cache_misses_total: AtomicU64,
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            inserts_total: AtomicU64::new(0),
            queries_total: AtomicU64::new(0),
            vector_searches_total: AtomicU64::new(0),
            active_connections: AtomicU64::new(0),
            cache_hits_total: AtomicU64::new(0),
            cache_misses_total: AtomicU64::new(0),
        }
    }
}

impl MetricsCollector {
    /// Render current metrics into Prometheus Text Format (version 0.0.4)
    pub fn render_prometheus(&self) -> String {
        let uptime_sec = self.start_time.elapsed().as_secs();
        let inserts = self.inserts_total.load(Ordering::Relaxed);
        let queries = self.queries_total.load(Ordering::Relaxed);
        let vector_searches = self.vector_searches_total.load(Ordering::Relaxed);
        let conns = self.active_connections.load(Ordering::Relaxed);
        let hits = self.cache_hits_total.load(Ordering::Relaxed);
        let misses = self.cache_misses_total.load(Ordering::Relaxed);

        let hit_ratio = if hits + misses == 0 {
            1.0
        } else {
            hits as f64 / (hits + misses) as f64
        };

        format!(
            "# HELP faizdb_uptime_seconds FaizDB process uptime in seconds\n\
             # TYPE faizdb_uptime_seconds gauge\n\
             faizdb_uptime_seconds {}\n\n\
             # HELP faizdb_operations_total Total database operations processed by type\n\
             # TYPE faizdb_operations_total counter\n\
             faizdb_operations_total{{op=\"insert\"}} {}\n\
             faizdb_operations_total{{op=\"query\"}} {}\n\
             faizdb_operations_total{{op=\"vector_search\"}} {}\n\n\
             # HELP faizdb_active_connections Current active client connections\n\
             # TYPE faizdb_active_connections gauge\n\
             faizdb_active_connections {}\n\n\
             # HELP faizdb_cache_hit_ratio Storage cache hit ratio\n\
             # TYPE faizdb_cache_hit_ratio gauge\n\
             faizdb_cache_hit_ratio {:.4}\n",
            uptime_sec, inserts, queries, vector_searches, conns, hit_ratio
        )
    }
}

/// Global shared collector instance
pub type SharedMetrics = Arc<MetricsCollector>;

/// GET /metrics handler
pub async fn metrics_handler() -> Response {
    // In production, instantiate or read from shared server state
    let collector = MetricsCollector::default();
    (
        [(CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        collector.render_prometheus(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prometheus_rendering() {
        let collector = MetricsCollector::default();
        collector.inserts_total.fetch_add(50, Ordering::Relaxed);
        collector.queries_total.fetch_add(150, Ordering::Relaxed);
        collector.vector_searches_total.fetch_add(30, Ordering::Relaxed);
        collector.cache_hits_total.fetch_add(90, Ordering::Relaxed);
        collector.cache_misses_total.fetch_add(10, Ordering::Relaxed);

        let rendered = collector.render_prometheus();
        assert!(rendered.contains("faizdb_uptime_seconds"));
        assert!(rendered.contains("faizdb_operations_total{op=\"insert\"} 50"));
        assert!(rendered.contains("faizdb_operations_total{op=\"query\"} 150"));
        assert!(rendered.contains("faizdb_cache_hit_ratio 0.9000"));
    }
}
