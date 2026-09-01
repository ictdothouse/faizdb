//! Transaction layer — ACID compliance for FaizDB.
//!
//! Provides Multi-Version Concurrency Control (MVCC) for:
//! - Snapshot Isolation (each transaction sees a consistent view)
//! - Non-blocking reads (readers never block writers)
//! - Conflict detection (write-write conflicts are detected and aborted)

pub mod mvcc;

pub use mvcc::Transaction;
