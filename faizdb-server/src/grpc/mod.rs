//! gRPC and Protocol Buffers Gateway (Port 50051) for FaizDB.

pub mod proto;
pub mod service;
pub mod listener;

pub use listener::run_grpc_server;
pub use proto::*;
pub use service::FaizDbGrpcService;
