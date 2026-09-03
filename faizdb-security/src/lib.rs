//! # FaizDB Security Engine — Zero-Trust Built-In Security
//!
//! Features:
//! - **AES-256-GCM** hardware-accelerated encryption at rest
//! - **Argon2id** password hashing with cryptographically secure salt
//! - **JWT** token generation & RBAC verification
//! - Tamper-evident architecture

pub mod auth;
pub mod encryption;
pub mod tls;

pub use auth::{AuthManager, Claims, Role};
pub use encryption::{Cipher, EncryptedData};
pub use tls::{create_rustls_server_config, generate_self_signed_cert, load_pem_cert_and_key};

/// Security engine version
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
