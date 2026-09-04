//! WebSocket Change Stream handlers.

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, info};

use faizdb_query::DatabaseContext;

use super::AppState;

/// GET /v1/subscribe — global Change Stream (all collections)
pub async fn ws_global_subscribe(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = state.db.clone();
    ws.on_upgrade(move |socket| handle_change_stream_socket(socket, db, None))
}

/// GET /v1/collections/{name}/watch — per-collection Change Stream
pub async fn ws_collection_watch(
    ws: WebSocketUpgrade,
    Path(collection_name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = state.db.clone();
    ws.on_upgrade(move |socket| handle_change_stream_socket(socket, db, Some(collection_name)))
}

async fn handle_change_stream_socket(
    socket: WebSocket,
    db: Arc<DatabaseContext>,
    target_collection: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = db.change_stream_bus().subscribe();
    let col_filter = target_collection.unwrap_or_else(|| "*".to_string());

    info!(
        "WebSocket client connected to Change Stream '{}' (total subscribers: {})",
        col_filter,
        db.change_stream_bus().subscriber_count()
    );

    let welcome = serde_json::json!({
        "status": "connected",
        "stream": "faizdb-change-streams-v1",
        "collection": col_filter,
        "active_subscribers": db.change_stream_bus().subscriber_count(),
        "timestamp": chrono::Utc::now(),
    });
    if let Ok(msg_str) = serde_json::to_string(&welcome) {
        let _ = sender.send(Message::Text(msg_str.into())).await;
    }

    let filter_for_task = col_filter.clone();
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if filter_for_task == "*" || filter_for_task == event.collection {
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
            if let Message::Close(_) = msg {
                break;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };

    debug!(
        "WebSocket client disconnected from Change Stream '{}'",
        col_filter
    );
}
