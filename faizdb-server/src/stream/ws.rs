//! WebSocket Real-Time Change Stream Handlers.

use std::sync::Arc;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use faizdb_query::DatabaseContext;

/// Subscription command from client
#[derive(Debug, Deserialize)]
struct ClientCommand {
    #[allow(dead_code)]
    action: Option<String>,
    collection: Option<String>,
}

/// Server welcome & status message
#[derive(Debug, Serialize)]
struct StreamWelcomeMessage {
    status: String,
    stream: String,
    collection: String,
    message: String,
}

/// WebSocket handler for global change stream subscription: `/v1/subscribe`
pub async fn ws_global_subscribe(
    ws: WebSocketUpgrade,
    State(db): State<Arc<DatabaseContext>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, db, None))
}

/// WebSocket handler for collection-specific subscription: `/v1/collections/{name}/watch`
pub async fn ws_collection_watch(
    ws: WebSocketUpgrade,
    Path(collection_name): Path<String>,
    State(db): State<Arc<DatabaseContext>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, db, Some(collection_name)))
}

async fn handle_socket(
    socket: WebSocket,
    db: Arc<DatabaseContext>,
    initial_collection: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = db.change_stream_bus().subscribe();
    let target_collection = initial_collection.unwrap_or_else(|| "*".to_string());

    info!(
        "New WebSocket subscriber connected for collection '{}' (Total: {})",
        target_collection,
        db.change_stream_bus().subscriber_count()
    );

    // Send initial welcome frame
    let welcome = StreamWelcomeMessage {
        status: "connected".to_string(),
        stream: "faizdb-change-streams-v1".to_string(),
        collection: target_collection.clone(),
        message: format!("Subscribed to real-time events for '{}'", target_collection),
    };
    if let Ok(msg_str) = serde_json::to_string(&welcome) {
        let _ = sender.send(Message::Text(msg_str.into())).await;
    }

    // Spawn task to read messages from client (e.g. dynamic subscription changes, ping)
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // Filter by collection if not wildcard
            if target_collection == "*" || target_collection == event.collection {
                if let Ok(json) = serde_json::to_string(&event) {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    if let Ok(cmd) = serde_json::from_str::<ClientCommand>(&text) {
                        if let Some(col) = cmd.collection {
                            debug!("Subscriber updated target collection to '{}'", col);
                            // Future enhancement: dynamic filter mutation
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either send or receive task to complete
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };

    debug!("WebSocket change stream subscriber disconnected");
}
