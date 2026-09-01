//! Time-To-Live (TTL) & Auto-Expiry Caching Engine.

pub mod manager;

pub use manager::{TtlManager, TtlStats, current_time_ms};
