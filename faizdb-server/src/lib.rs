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

    // Initialise AuthManager with JWT secret from env
    let jwt_secret = std::env::var("FAIZDB_JWT_SECRET")
        .unwrap_or_else(|_| "faizdb-jwt-secret-change-in-production".to_string());
    let auth = std::sync::Arc::new(faizdb_security::auth::AuthManager::new(jwt_secret.as_bytes()));

    let state = std::sync::Arc::new(AppState {
        db: db.clone(),
        auth,
        backup_schedule: std::sync::Arc::new(std::sync::RwLock::new(api::BackupScheduleConfig::default())),
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

    // Run HTTP & WebSocket API with graceful shutdown on CTRL+C / SIGTERM
    let http_handle = tokio::spawn(async move {
        let service = http_router.into_make_service_with_connect_info::<std::net::SocketAddr>();
        axum::serve(http_listener, service)
            .with_graceful_shutdown(shutdown_signal())
            .await
            .unwrap_or_else(|e| tracing::error!("HTTP/WS server error: {e}"));
    });

    let _ = tokio::try_join!(wire_handle, http_handle)?;
    Ok(())
}

/// Run only the HTTP & WebSocket server
pub async fn run_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let db = std::sync::Arc::new(faizdb_query::DatabaseContext::new());

    let jwt_secret = std::env::var("FAIZDB_JWT_SECRET")
        .unwrap_or_else(|_| "faizdb-jwt-secret-change-in-production".to_string());
    let auth = std::sync::Arc::new(faizdb_security::auth::AuthManager::new(jwt_secret.as_bytes()));

    let state = std::sync::Arc::new(AppState {
        db,
        auth,
        backup_schedule: std::sync::Arc::new(std::sync::RwLock::new(api::BackupScheduleConfig::default())),
    });
    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("🔥 FaizDB Server running on http://{addr}");

    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Graceful shutdown signal handler — listens for CTRL+C (cross-platform) and SIGTERM (Linux/Docker).
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("Failed to install CTRL+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { tracing::info!("🛑 FaizDB received CTRL+C — initiating graceful shutdown..."); }
        _ = terminate => { tracing::info!("🛑 FaizDB received SIGTERM — initiating graceful shutdown..."); }
    }
}
