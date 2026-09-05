//! MySQL / MariaDB Wire Protocol (Port 3306) Compatibility Engine.

pub mod codec;
pub mod handler;
pub mod listener;

pub use listener::{run_mysql_server, run_mysql_server_with_shutdown};
