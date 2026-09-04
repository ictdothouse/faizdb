//! Multi-Protocol Wire Engine for FaizDB (MongoDB Port 27017 & PostgreSQL Port 5432).

pub mod handler;
pub mod header;
pub mod listener;
pub mod op_msg;
pub mod op_query;

pub mod postgres;

pub use listener::{run_wire_server, run_wire_server_with_shutdown};
pub use postgres::{run_postgres_server, run_postgres_server_with_shutdown};
