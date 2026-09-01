//! Reactive Change Stream Broadcast Bus.

use std::sync::Arc;
use tokio::sync::broadcast::{self, Receiver, Sender};
use tracing::debug;

use super::event::ChangeEvent;

/// Default buffer capacity for change events
const DEFAULT_CHANNEL_CAPACITY: usize = 65_536;

/// High-throughput pub/sub event bus for database mutations
#[derive(Clone)]
pub struct ChangeStreamBus {
    sender: Arc<Sender<ChangeEvent>>,
}

impl ChangeStreamBus {
    /// Create a new ChangeStreamBus with default buffer capacity
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(DEFAULT_CHANNEL_CAPACITY);
        Self {
            sender: Arc::new(sender),
        }
    }

    /// Publish a change event to all active subscribers
    pub fn publish(&self, event: ChangeEvent) {
        debug!(
            "Emitting change stream event: {:?} on collection '{}'",
            event.operation_type, event.collection
        );
        let _ = self.sender.send(event);
    }

    /// Subscribe to all database mutation events
    pub fn subscribe(&self) -> Receiver<ChangeEvent> {
        self.sender.subscribe()
    }

    /// Total number of active listener subscriptions
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl Default for ChangeStreamBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::Document;

    #[tokio::test]
    async fn test_bus_publish_and_subscribe() {
        let bus = ChangeStreamBus::new();
        let mut rx = bus.subscribe();

        let mut doc = Document::new();
        doc.set("name", "Ahmad Faiz");

        let event = ChangeEvent::insert("users", doc.clone());
        bus.publish(event);

        let received = rx.recv().await.unwrap();
        assert_eq!(received.collection, "users");
        assert_eq!(received.operation_type, super::super::event::OperationType::Insert);
        assert!(received.full_document.is_some());
    }
}
