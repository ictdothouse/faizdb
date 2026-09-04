//! Time-To-Live (TTL) & Auto-Expiry Caching Engine.

pub mod manager;

pub use manager::{current_time_ms, TtlManager, TtlStats};
