//! # FaizDB Server — High-Performance Multi-Protocol Server
//!
//! Provides 4 concurrent entry gateways:
//! 1. **MongoDB Wire Protocol** (Port 27017) — drop-in replacement for MongoDB apps & tools.
//! 2. **PostgreSQL Wire Protocol** (Port 5432) — drop-in compatibility for psql, DBeaver, TablePlus, Grafana, SQL ORMs.
//! 3. **gRPC & Protocol Buffers** (Port 50051) — ultra-low latency IPC for microservices & streaming AI vectors.
//! 4. **REST / HTTP & WebSocket Change Streams** (Port 27018) — for web clients, microservices, and reactive subscriptions.

pub mod api;
pub mod wire;
pub mod grpc;
pub mod stream;

pub use api::{create_router, middleware::AppState, BackupScheduleConfig};
pub use wire::{run_wire_server, run_postgres_server};
pub use grpc::run_grpc_server;

/// Run the 4-way Multi-Protocol FaizDB server (MongoDB Wire + PostgreSQL Wire + gRPC + HTTP/WS)
pub async fn run_multi_protocol_server(
    wire_addr: &str,
    pg_addr: &str,
    grpc_addr: &str,
    http_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = std::env::var("FAIZDB_DATA_DIR").unwrap_or_else(|_| faizdb_core::DEFAULT_DATA_DIR.to_string());
    let db = std::sync::Arc::new(
        faizdb_query::DatabaseContext::with_storage_dir(&data_dir)
            .unwrap_or_else(|e| {
                tracing::warn!("Persistent storage not initialized at '{data_dir}' ({e}); running in-memory");
                faizdb_query::DatabaseContext::new()
            })
    );

    // Initialise AuthManager with JWT secret from env
    let jwt_secret = std::env::var("FAIZDB_JWT_SECRET")
        .unwrap_or_else(|_| "faizdb-jwt-secret-change-in-production".to_string());
    let auth = std::sync::Arc::new(faizdb_security::auth::AuthManager::new(jwt_secret.as_bytes()));
    let user_store = std::sync::Arc::new(faizdb_security::UserStore::new());

    let local_region = std::env::var("FAIZDB_REGION").unwrap_or_else(|_| "default-region".to_string());
    let geo_replication = std::sync::Arc::new(faizdb_core::cluster::GeoReplicationEngine::new(local_region));

    let state = std::sync::Arc::new(AppState {
        db: db.clone(),
        auth,
        user_store: user_store.clone(),
        backup_schedule: std::sync::Arc::new(parking_lot::RwLock::new(api::BackupScheduleConfig::default())),
        geo_replication,
        metrics: std::sync::Arc::new(api::metrics::MetricsCollector::default()),
    });

    let http_router = create_router(state.clone());
    let http_addr_str = http_addr.to_string();
    let tls_config_opt = get_server_tls_config().await;

    // 1. Run HTTP & WebSocket API with graceful shutdown on CTRL+C / SIGTERM
    // Determine TLS mode before binding to prevent EADDRINUSE port collision
    let http_handle = if let Some(tls_config) = tls_config_opt {
        let socket_addr: std::net::SocketAddr = http_addr_str
            .parse()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("Invalid HTTP socket address '{http_addr_str}': {e}")))?;
        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
        });
        tracing::info!("🔒 FaizDB REST/HTTP & WebSocket Change Streams running with TLS on https://{http_addr_str}");
        tokio::spawn(async move {
            if let Err(e) = axum_server::bind_rustls(socket_addr, tls_config)
                .handle(handle)
                .serve(http_router.into_make_service_with_connect_info::<std::net::SocketAddr>())
                .await
            {
                tracing::error!("TLS HTTP/WS server error: {e}");
            }
        })
    } else {
        let http_listener = tokio::net::TcpListener::bind(http_addr).await?;
        tracing::info!("🔥 FaizDB REST/HTTP & WebSocket Change Streams running on http://{http_addr}");
        let service = http_router.into_make_service_with_connect_info::<std::net::SocketAddr>();
        tokio::spawn(async move {
            axum::serve(http_listener, service)
                .with_graceful_shutdown(shutdown_signal())
                .await
                .unwrap_or_else(|e| tracing::error!("HTTP/WS server error: {e}"));
        })
    };

    let wire_addr_str = wire_addr.to_string();
    let db_for_mongo = db.clone();
    let user_store_for_mongo = user_store.clone();

    // 2. Spawn MongoDB Wire Protocol server (Port 27017)
    let mongo_handle = tokio::spawn(async move {
        if let Err(e) = run_wire_server(&wire_addr_str, db_for_mongo, user_store_for_mongo).await {
            tracing::error!("MongoDB Wire server error: {e}");
        }
    });

    let pg_addr_str = pg_addr.to_string();
    let db_for_pg = db.clone();
    let user_store_for_pg = user_store.clone();

    // 3. Spawn PostgreSQL Wire Protocol server (Port 5432)
    let pg_handle = tokio::spawn(async move {
        if let Err(e) = run_postgres_server(&pg_addr_str, db_for_pg, user_store_for_pg).await {
            tracing::error!("PostgreSQL Wire server error: {e}");
        }
    });

    let grpc_addr_str = grpc_addr.to_string();
    let db_for_grpc = db.clone();
    let auth_for_grpc = state.auth.clone();
    let user_store_for_grpc = user_store.clone();

    // 4. Spawn gRPC / Protocol Buffers server (Port 50051)
    let grpc_handle = tokio::spawn(async move {
        if let Err(e) = run_grpc_server(&grpc_addr_str, db_for_grpc, auth_for_grpc, user_store_for_grpc).await {
            tracing::error!("gRPC server error: {e}");
        }
    });

    // 5. Background TTL Sweeper: automatically purge expired documents every 30s
    let db_for_ttl = db.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            for (col_name, col) in db_for_ttl.all_collections() {
                let purged = col.purge_expired();
                if !purged.is_empty() {
                    tracing::info!("[TTL Sweeper] Purged {} expired document(s) from collection '{}'", purged.len(), col_name);
                }
            }
        }
    });

    let _ = tokio::try_join!(mongo_handle, pg_handle, grpc_handle, http_handle)?;
    Ok(())
}

/// Helper function to load TLS configuration from environment variables
pub async fn get_server_tls_config() -> Option<axum_server::tls_rustls::RustlsConfig> {
    let _ = rustls::crypto::ring::default_provider().install_default();

    if let (Ok(cert_path), Ok(key_path)) = (std::env::var("FAIZDB_TLS_CERT"), std::env::var("FAIZDB_TLS_KEY")) {
        match axum_server::tls_rustls::RustlsConfig::from_pem_file(&cert_path, &key_path).await {
            Ok(config) => {
                tracing::info!("🔒 TLS enabled with certificates from '{cert_path}'");
                return Some(config);
            }
            Err(e) => tracing::error!("Failed to load TLS certificates from '{cert_path}': {e}"),
        }
    } else if std::env::var("FAIZDB_ENABLE_TLS").map(|v| v == "true" || v == "1").unwrap_or(false) {
        match faizdb_security::generate_self_signed_cert(&["localhost".into(), "127.0.0.1".into()]) {
            Ok((certs, key)) => {
                let certs_der: Vec<Vec<u8>> = certs.into_iter().map(|c| c.to_vec()).collect();
                let key_der = match key {
                    rustls_pki_types::PrivateKeyDer::Pkcs8(p) => p.secret_pkcs8_der().to_vec(),
                    rustls_pki_types::PrivateKeyDer::Pkcs1(p) => p.secret_pkcs1_der().to_vec(),
                    rustls_pki_types::PrivateKeyDer::Sec1(p) => p.secret_sec1_der().to_vec(),
                    _ => Vec::new(),
                };
                match axum_server::tls_rustls::RustlsConfig::from_der(certs_der, key_der).await {
                    Ok(config) => {
                        tracing::info!("🔒 TLS enabled with auto-generated self-signed certificate (HTTPS/WSS ready)");
                        return Some(config);
                    }
                    Err(e) => tracing::error!("Failed to initialize TLS config from DER: {e}"),
                }
            }
            Err(e) => tracing::error!("Failed to generate self-signed TLS cert: {e}"),
        }
    }
    None
}

/// Run the dual FaizDB server (MongoDB Wire Protocol on `wire_addr` + HTTP & WebSocket on `http_addr`)
pub async fn run_dual_server(
    wire_addr: &str,
    http_addr: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let host = wire_addr.split(':').next().unwrap_or("0.0.0.0");
    let pg_addr = format!("{host}:5432");
    let grpc_addr = format!("{host}:50051");
    run_multi_protocol_server(wire_addr, &pg_addr, &grpc_addr, http_addr).await
}

/// Run only the HTTP & WebSocket server
pub async fn run_server(addr: &str) -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = std::env::var("FAIZDB_DATA_DIR").unwrap_or_else(|_| faizdb_core::DEFAULT_DATA_DIR.to_string());
    let db = std::sync::Arc::new(
        faizdb_query::DatabaseContext::with_storage_dir(&data_dir)
            .unwrap_or_else(|e| {
                tracing::warn!("Persistent storage not initialized at '{data_dir}' ({e}); running in-memory");
                faizdb_query::DatabaseContext::new()
            })
    );

    let jwt_secret = std::env::var("FAIZDB_JWT_SECRET")
        .unwrap_or_else(|_| "faizdb-jwt-secret-change-in-production".to_string());
    let auth = std::sync::Arc::new(faizdb_security::auth::AuthManager::new(jwt_secret.as_bytes()));
    let user_store = std::sync::Arc::new(faizdb_security::UserStore::new());

    let local_region = std::env::var("FAIZDB_REGION").unwrap_or_else(|_| "default-region".to_string());
    let geo_replication = std::sync::Arc::new(faizdb_core::cluster::GeoReplicationEngine::new(local_region));

    let state = std::sync::Arc::new(AppState {
        db,
        auth,
        user_store,
        backup_schedule: std::sync::Arc::new(parking_lot::RwLock::new(api::BackupScheduleConfig::default())),
        geo_replication,
        metrics: std::sync::Arc::new(api::metrics::MetricsCollector::default()),
    });
    let app = create_router(state);

    if let Some(tls_config) = get_server_tls_config().await {
        let handle = axum_server::Handle::new();
        let shutdown_handle = handle.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            shutdown_handle.graceful_shutdown(Some(std::time::Duration::from_secs(5)));
        });

        tracing::info!("🔒 FaizDB Server running with TLS on https://{addr}");
        let socket_addr: std::net::SocketAddr = addr.parse().map_err(|e| format!("Invalid address '{addr}': {e}"))?;
        axum_server::bind_rustls(socket_addr, tls_config)
            .handle(handle)
            .serve(app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .await?;
    } else {
        tracing::info!("🔥 FaizDB Server running on http://{addr}");
        let listener = tokio::net::TcpListener::bind(addr).await?;
        axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
            .with_graceful_shutdown(shutdown_signal())
            .await?;
    }

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
