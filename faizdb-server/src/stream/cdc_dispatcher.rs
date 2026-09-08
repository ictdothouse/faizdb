//! # Active Outbound CDC Stream Dispatcher
//!
//! Autonomous, background stream dispatcher that ingests `CdcEnvelope` mutations
//! and pushes them to downstream Lakehouse ingestion endpoints, Apache Kafka REST proxies,
//! or HTTP webhooks with bounded batching, exponential backoff retries, and delivery telemetry.

use super::cdc::CdcEnvelope;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// Configuration for the Outbound CDC Dispatcher
#[derive(Debug, Clone)]
pub struct CdcDispatcherConfig {
    /// Destination endpoint URL (e.g. "http://127.0.0.1:8082/topics/faizdb_events")
    pub endpoint_url: String,
    /// Maximum number of events to accumulate before flushing
    pub batch_size: usize,
    /// Maximum time to wait before flushing an incomplete batch
    pub flush_interval: Duration,
    /// Maximum retries per batch on network/transport errors
    pub max_retries: usize,
    /// Initial backoff delay on retry
    pub initial_backoff: Duration,
    /// Capacity of the inbound asynchronous event channel
    pub channel_capacity: usize,
}

impl Default for CdcDispatcherConfig {
    fn default() -> Self {
        Self {
            endpoint_url: "http://127.0.0.1:8082/cdc".to_string(),
            batch_size: 50,
            flush_interval: Duration::from_millis(100),
            max_retries: 3,
            initial_backoff: Duration::from_millis(50),
            channel_capacity: 10_000,
        }
    }
}

/// Telemetry metrics for the Outbound CDC Dispatcher
#[derive(Debug, Default)]
pub struct CdcMetrics {
    pub events_emitted: AtomicU64,
    pub events_delivered: AtomicU64,
    pub events_failed: AtomicU64,
    pub batches_dispatched: AtomicU64,
}

impl CdcMetrics {
    pub fn new() -> Self {
        Self::default()
    }
}

/// In-Memory / Test Transport for deterministic testing and embedded sinks
#[derive(Default)]
pub struct InMemoryCdcTransport {
    delivered: parking_lot::RwLock<Vec<CdcEnvelope>>,
    simulate_failures: std::sync::atomic::AtomicUsize,
}

impl InMemoryCdcTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_simulated_failures(failures: usize) -> Self {
        Self {
            delivered: parking_lot::RwLock::new(Vec::new()),
            simulate_failures: std::sync::atomic::AtomicUsize::new(failures),
        }
    }

    pub async fn dispatch(&self, _endpoint: &str, batch: &[CdcEnvelope]) -> Result<(), String> {
        let remaining = self.simulate_failures.load(Ordering::SeqCst);
        if remaining > 0 {
            self.simulate_failures.fetch_sub(1, Ordering::SeqCst);
            return Err("Simulated temporary network/sink failure".to_string());
        }

        let mut del = self.delivered.write();
        del.extend_from_slice(batch);
        Ok(())
    }

    pub fn get_delivered(&self) -> Vec<CdcEnvelope> {
        self.delivered.read().clone()
    }

    pub fn clear(&self) {
        self.delivered.write().clear();
    }
}

/// Native Lightweight HTTP Webhook Transport over Tokio TCP
#[derive(Default)]
pub struct HttpWebhookTransport;

impl HttpWebhookTransport {
    pub fn new() -> Self {
        Self
    }

    pub async fn dispatch(&self, endpoint: &str, batch: &[CdcEnvelope]) -> Result<(), String> {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpStream;

        let payload_json = serde_json::to_string(batch)
            .map_err(|e| format!("Serialization error: {e}"))?;

        // Parse host, port, and path from http://host:port/path
        let url_stripped = endpoint
            .strip_prefix("http://")
            .ok_or_else(|| "Only http:// endpoints currently supported by native transport".to_string())?;

        let (host_port, path) = match url_stripped.find('/') {
            Some(idx) => (&url_stripped[..idx], &url_stripped[idx..]),
            None => (url_stripped, "/"),
        };

        let host_port_owned = if host_port.contains(':') {
            host_port.to_string()
        } else {
            format!("{host_port}:80")
        };

        let mut stream = TcpStream::connect(&host_port_owned)
            .await
            .map_err(|e| format!("Failed to connect to CDC endpoint {host_port_owned}: {e}"))?;

        let request = format!(
            "POST {path} HTTP/1.1\r\n\
             Host: {host_port}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {len}\r\n\
             Connection: close\r\n\
             User-Agent: FaizDB-CDC/0.1.0\r\n\
             \r\n\
             {payload_json}",
            len = payload_json.len()
        );

        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| format!("Failed to send CDC HTTP request: {e}"))?;

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .await
            .map_err(|e| format!("Failed to read CDC response: {e}"))?;

        let resp_str = String::from_utf8_lossy(&response);
        if resp_str.starts_with("HTTP/1.1 2") || resp_str.starts_with("HTTP/1.0 2") {
            Ok(())
        } else {
            let status_line = resp_str.lines().next().unwrap_or("Unknown status");
            Err(format!("Endpoint rejected CDC batch: {status_line}"))
        }
    }
}

/// Transport backend selection for Outbound CDC Dispatching
#[derive(Clone)]
pub enum CdcTransportBackend {
    InMemory(Arc<InMemoryCdcTransport>),
    Http(Arc<HttpWebhookTransport>),
}

impl CdcTransportBackend {
    pub async fn dispatch(&self, endpoint: &str, batch: &[CdcEnvelope]) -> Result<(), String> {
        match self {
            Self::InMemory(mem) => mem.dispatch(endpoint, batch).await,
            Self::Http(http) => http.dispatch(endpoint, batch).await,
        }
    }
}

/// Producer handle for enqueueing CDC mutations from database write operations
#[derive(Clone)]
pub struct CdcSender {
    tx: mpsc::Sender<CdcEnvelope>,
    metrics: Arc<CdcMetrics>,
}

impl CdcSender {
    /// Emit a CDC event to the outbound dispatcher queue
    pub fn emit(&self, event: CdcEnvelope) -> bool {
        self.metrics.events_emitted.fetch_add(1, Ordering::Relaxed);
        match self.tx.try_send(event) {
            Ok(_) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                warn!("Outbound CDC channel full; dropping event to protect primary WAL write throughput");
                self.metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                error!("Outbound CDC dispatcher channel is closed");
                self.metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }
}

/// The Outbound CDC Dispatcher managing batching and retry dispatch loops
pub struct CdcOutboundDispatcher {
    config: CdcDispatcherConfig,
    metrics: Arc<CdcMetrics>,
    transport: CdcTransportBackend,
    rx: mpsc::Receiver<CdcEnvelope>,
}

impl CdcOutboundDispatcher {
    /// Create a new dispatcher instance along with its producer sender
    pub fn new(
        config: CdcDispatcherConfig,
        transport: CdcTransportBackend,
    ) -> (Self, CdcSender, Arc<CdcMetrics>) {
        let (tx, rx) = mpsc::channel(config.channel_capacity);
        let metrics = Arc::new(CdcMetrics::new());
        let sender = CdcSender {
            tx,
            metrics: Arc::clone(&metrics),
        };

        let dispatcher = Self {
            config,
            metrics: Arc::clone(&metrics),
            transport,
            rx,
        };

        (dispatcher, sender, metrics)
    }

    /// Run the dispatcher loop indefinitely in the background
    pub async fn run(mut self) {
        info!(
            "Starting Outbound CDC Dispatcher targeting {}",
            self.config.endpoint_url
        );

        let mut batch = Vec::with_capacity(self.config.batch_size);
        let mut interval = tokio::time::interval(self.config.flush_interval);

        loop {
            tokio::select! {
                maybe_event = self.rx.recv() => {
                    match maybe_event {
                        Some(event) => {
                            batch.push(event);
                            if batch.len() >= self.config.batch_size {
                                self.flush_batch(&mut batch).await;
                            }
                        }
                        None => {
                            // Channel closed - flush remaining and exit
                            if !batch.is_empty() {
                                self.flush_batch(&mut batch).await;
                            }
                            info!("CDC Dispatcher channel closed. Dispatcher exiting.");
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !batch.is_empty() {
                        self.flush_batch(&mut batch).await;
                    }
                }
            }
        }
    }

    /// Flush the accumulated batch with exponential backoff retries
    async fn flush_batch(&self, batch: &mut Vec<CdcEnvelope>) {
        let count = batch.len() as u64;
        let mut attempts = 0;
        let mut backoff = self.config.initial_backoff;

        loop {
            attempts += 1;
            match self.transport.dispatch(&self.config.endpoint_url, batch).await {
                Ok(_) => {
                    self.metrics.events_delivered.fetch_add(count, Ordering::Relaxed);
                    self.metrics.batches_dispatched.fetch_add(1, Ordering::Relaxed);
                    batch.clear();
                    return;
                }
                Err(err) => {
                    if attempts > self.config.max_retries {
                        error!(
                            "Failed to deliver CDC batch of {count} events after {attempts} attempts: {err}. Routing to DLQ drop."
                        );
                        self.metrics.events_failed.fetch_add(count, Ordering::Relaxed);
                        batch.clear();
                        return;
                    }
                    warn!(
                        "CDC dispatch attempt {attempts}/{} failed: {err}. Retrying in {:?}...",
                        self.config.max_retries, backoff
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = backoff.saturating_mul(2);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_in_memory_cdc_dispatch_and_batching() {
        let transport = Arc::new(InMemoryCdcTransport::new());
        let config = CdcDispatcherConfig {
            endpoint_url: "in_memory://events".to_string(),
            batch_size: 2,
            flush_interval: Duration::from_millis(50),
            max_retries: 2,
            initial_backoff: Duration::from_millis(10),
            channel_capacity: 100,
        };

        let (dispatcher, sender, metrics) =
            CdcOutboundDispatcher::new(config, CdcTransportBackend::InMemory(Arc::clone(&transport)));

        let handle = tokio::spawn(dispatcher.run());

        let doc1 = serde_json::json!({"id": 1, "item": "Alpha"});
        let doc2 = serde_json::json!({"id": 2, "item": "Beta"});

        assert!(sender.emit(CdcEnvelope::new_create("inventory", "1", doc1, 100)));
        assert!(sender.emit(CdcEnvelope::new_create("inventory", "2", doc2, 101)));

        // Give the background worker a few milliseconds to process the batch
        tokio::time::sleep(Duration::from_millis(80)).await;

        let delivered = transport.get_delivered();
        assert_eq!(delivered.len(), 2);
        assert_eq!(metrics.events_delivered.load(Ordering::SeqCst), 2);
        assert_eq!(metrics.batches_dispatched.load(Ordering::SeqCst), 1);

        drop(sender);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_cdc_retry_with_exponential_backoff() {
        // Transport will fail the first 2 attempts, then succeed on attempt 3
        let transport = Arc::new(InMemoryCdcTransport::with_simulated_failures(2));
        let config = CdcDispatcherConfig {
            endpoint_url: "in_memory://retry_test".to_string(),
            batch_size: 1,
            flush_interval: Duration::from_millis(20),
            max_retries: 3,
            initial_backoff: Duration::from_millis(10),
            channel_capacity: 10,
        };

        let (dispatcher, sender, metrics) =
            CdcOutboundDispatcher::new(config, CdcTransportBackend::InMemory(Arc::clone(&transport)));

        let handle = tokio::spawn(dispatcher.run());

        let doc = serde_json::json!({"test": "retry"});
        assert!(sender.emit(CdcEnvelope::new_create("orders", "ord_1", doc, 500)));

        tokio::time::sleep(Duration::from_millis(120)).await;

        assert_eq!(transport.get_delivered().len(), 1);
        assert_eq!(metrics.events_delivered.load(Ordering::SeqCst), 1);

        drop(sender);
        let _ = handle.await;
    }
}
