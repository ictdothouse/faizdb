//! gRPC and Protocol Buffers Gateway (Port 50051) for FaizDB.

pub mod listener;
pub mod proto;
pub mod service;

pub use listener::{run_grpc_server, run_grpc_server_with_shutdown};
pub use proto::*;
pub use service::FaizDbGrpcService;
