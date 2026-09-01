//! # FaizDB Security Engine — Zero-Trust Built-In Security
//!
//! Features:
//! - **AES-256-GCM** hardware-accelerated encryption at rest
//! - **Argon2id** password hashing with cryptographically secure salt
//! - **JWT** token generation & RBAC verification
//! - Tamper-evident architecture

pub mod auth;
pub mod encryption;

pub use auth::{AuthManager, Claims, Role};
pub use encryption::{Cipher, EncryptedData};

/// Security engine version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
