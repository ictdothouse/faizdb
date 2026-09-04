//! PostgreSQL Wire Protocol (Port 5432) Compatibility Engine.

pub mod codec;
pub mod handler;
pub mod listener;

pub use listener::{run_postgres_server, run_postgres_server_with_shutdown};
