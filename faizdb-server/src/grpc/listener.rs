//! gRPC Server TCP Listener (Port 50051).

use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::info;

use super::proto::FaizDbServiceServer;
use super::service::FaizDbGrpcService;
use faizdb_query::DatabaseContext;

/// Run the FaizDB gRPC / Protocol Buffers Server with optional graceful shutdown future
pub async fn run_grpc_server_with_shutdown<F>(
    addr: &str,
    db: Arc<DatabaseContext>,
    auth: Arc<faizdb_security::auth::AuthManager>,
    user_store: Arc<faizdb_security::UserStore>,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let socket_addr: SocketAddr = addr.parse()?;
    info!("⚡ gRPC / Protocol Buffers Server running on grpc://{addr}");

    let svc = FaizDbGrpcService::new(db, auth, user_store);
    let server = FaizDbServiceServer::new(svc);

    Server::builder()
        .add_service(server)
        .serve_with_shutdown(socket_addr, shutdown)
        .await?;

    Ok(())
}

/// Run the FaizDB gRPC / Protocol Buffers Server on the given address (e.g. "0.0.0.0:50051")
pub async fn run_grpc_server(
    addr: &str,
    db: Arc<DatabaseContext>,
    auth: Arc<faizdb_security::auth::AuthManager>,
    user_store: Arc<faizdb_security::UserStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    run_grpc_server_with_shutdown(addr, db, auth, user_store, std::future::pending()).await
}
