//! # FaizDB Server — High-Performance Multi-Protocol Database Server

pub mod api;

pub use api::{create_router, AppState};

/// Run the FaizDB HTTP server on a specified address
pub async fn run_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let state = std::sync::Arc::new(AppState {
        db: faizdb_query::DatabaseContext::new(),
    });

    let app = create_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("🔥 FaizDB Server running on http://{addr}");

    axum::serve(listener, app).await?;
    Ok(())
}
