//! Storage layer — the core persistence engine for FaizDB.
//!
//! ## Architecture
//!
//! FaizDB uses a hybrid storage approach:
//!
//! 1. **Write Path (LSM-Tree inspired)**:
//!    - Writes go to WAL first (durability guarantee)
//!    - Then to MemTable (in-memory SkipList)
//!    - When MemTable is full, flush to SSTable on disk
//!    - Background compaction merges SSTables
//!
//! 2. **Read Path**:
//!    - Check MemTable first (most recent data)
//!    - Then check SSTables (newest to oldest)
//!    - Bloom filters for fast negative lookups
//!    - Block cache for frequently accessed data
//!
//! This gives us the best of both worlds:
//! - Fast writes (sequential I/O, like MongoDB)
//! - Fast reads (cached, indexed, like PostgreSQL)

pub mod arc_cache;
pub mod columnar;
pub mod compaction;
pub mod engine;
pub mod memtable;
pub mod sstable;
pub mod tiered;
pub mod wal;
