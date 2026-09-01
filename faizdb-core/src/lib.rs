//! # FaizDB Core Storage Engine
//!
//! The foundational storage layer for FaizDB — an AI-Native NoSQL database engine.
//!
//! ## Architecture
//!
//! The core engine is composed of several layers:
//!
//! 1. **Storage Layer** — Hybrid LSM-Tree + B-Tree storage engine
//!    - MemTable (in-memory write buffer using SkipList)
//!    - SSTable (sorted immutable disk files)
//!    - WAL (Write-Ahead Log for durability)
//!    - Buffer Pool (page cache for frequently accessed data)
//!
//! 2. **Document Layer** — BSON/JSON document management
//!    - Document encoding/decoding
//!    - Schema validation (optional)
//!    - Secondary indexing
//!
//! 3. **Transaction Layer** — ACID compliance
//!    - MVCC (Multi-Version Concurrency Control)
//!    - Snapshot Isolation
//!    - Lock management
//!
//! ## Design Principles
//!
//! - **Zero-copy where possible** — minimize memory allocations
//! - **Lock-free reads** — readers never block writers
//! - **Crash-safe** — WAL ensures no data loss on unexpected shutdown
//! - **Embeddable** — can be linked as a library or run as a server

pub mod cluster;
pub mod document;
pub mod error;
pub mod search;
pub mod storage;
pub mod stream;
pub mod transaction;
pub mod ttl;

// Re-export commonly used types
pub use cluster::{RaftNode, NodeRole, ShardRouter, ShardDistribution};
pub use document::{Document, DocumentId, Value};
pub use error::{FaizError, FaizResult};
pub use search::{InvertedIndex, SearchResult};
pub use storage::engine::StorageEngine;
pub use stream::{ChangeEvent, ChangeStreamBus, OperationType};
pub use ttl::{TtlManager, TtlStats};

/// The database version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default data directory name
pub const DEFAULT_DATA_DIR: &str = "faizdb_data";

/// Maximum document size (256 MB — far beyond MongoDB's 16MB limit)
pub const MAX_DOCUMENT_SIZE: usize = 256 * 1024 * 1024;

/// Default MemTable size before flush (64 MB)
pub const DEFAULT_MEMTABLE_SIZE: usize = 64 * 1024 * 1024;

/// Default buffer pool size (256 MB)
pub const DEFAULT_BUFFER_POOL_SIZE: usize = 256 * 1024 * 1024;

/// Magic bytes for FaizDB data files
pub const MAGIC_BYTES: &[u8; 8] = b"FAIZDB01";
