//! Document model — the fundamental data unit in FaizDB.
//!
//! Every piece of data in FaizDB is a [`Document`] — a flexible,
//! schema-optional JSON/BSON-compatible structure that supports:
//!
//! - Nested objects and arrays
//! - Rich types (DateTime, UUID, Binary, Decimal, Vector)
//! - Auto-generated IDs (UUID v7 — time-sortable)
//! - Optional schema validation

pub mod model;
pub mod collection;
pub mod index;

pub use model::{Document, DocumentId, Value};
pub use collection::Collection;
