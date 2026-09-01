//! gRPC Server TCP Listener (Port 50051).

use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tracing::info;

use faizdb_query::DatabaseContext;
use super::proto::FaizDbServiceServer;
use super::service::FaizDbGrpcService;

/// Run the FaizDB gRPC / Protocol Buffers Server on the given address (e.g. "0.0.0.0:50051")
pub async fn run_grpc_server(
    addr: &str,
    db: Arc<DatabaseContext>,
) -> Result<(), Box<dyn std::error::Error>> {
    let socket_addr: SocketAddr = addr.parse()?;
    info!("⚡ gRPC / Protocol Buffers Server running on grpc://{addr}");

    let svc = FaizDbGrpcService::new(db);
    let server = FaizDbServiceServer::new(svc);

    Server::builder()
        .add_service(server)
        .serve(socket_addr)
        .await?;

    Ok(())
}
