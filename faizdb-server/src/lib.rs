//! # FaizDB Server — High-Performance Dual-Protocol Server
//!
//! Provides:
//! 1. **MongoDB Wire Protocol** (Port 27017) — drop-in replacement for MongoDB apps & tools.
//! 2. **REST / HTTP & WebSocket Change Streams** (Port 27018) — for web clients, microservices, and reactive subscriptions.

pub mod api;
pub mod wire;
pub mod stream;

pub use api::{create_router, AppState};
pub use wire::run_wire_server;

/// Run the unified FaizDB server (MongoDB Wire Protocol on `wire_addr` + HTTP & WebSocket on `http_addr`)
pub async fn run_dual_server(
    wire_addr: &str,
    http_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = std::sync::Arc::new(faizdb_query::DatabaseContext::new());

    let state = std::sync::Arc::new(AppState {
        db: db.clone(),
    });

    let http_router = create_router(state);
    let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
    tracing::info!("🔥 FaizDB REST/HTTP & WebSocket Change Streams running on http://{http_addr}");

    let wire_addr_str = wire_addr.to_string();
    let db_for_wire = db.clone();

    // Spawn MongoDB Wire Protocol server in background task
    let wire_handle = tokio::spawn(async move {
        if let Err(e) = run_wire_server(&wire_addr_str, db_for_wire).await {
            tracing::error!("Wire server error: {e}");
        }
    });

    // Run HTTP & WebSocket API
    let http_handle = tokio::spawn(async move {
        if let Err(e) = axum::serve(http_listener, http_router).await {
            tracing::error!("HTTP/WS server error: {e}");
        }
    });

    let _ = tokio::try_join!(wire_handle, http_handle)?;
    Ok(())
}

/// Run only the HTTP & WebSocket server
pub async fn run_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let db = std::sync::Arc::new(faizdb_query::DatabaseContext::new());
    let state = std::sync::Arc::new(AppState {
        db,
    });

    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("🔥 FaizDB Server running on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
