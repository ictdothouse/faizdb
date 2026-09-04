//! Authentication, Password Hashing, JWT Tokens, and Role-Based Access Control (RBAC).
//!
//! ## JWT Algorithm
//!
//! Uses **EdDSA (Ed25519)** — the 2026 standard.
//! - Far stronger than HS256 (symmetric shared secret)
//! - 64-byte compact signatures (smaller than RS256)
//! - Immune to timing attacks (constant-time verify)
//!
//! Keys are generated fresh on startup and stored as PEM in memory.
//! For production, supply pre-generated keys via `FAIZDB_JWT_PRIVATE_KEY` /
//! `FAIZDB_JWT_PUBLIC_KEY` environment variables (PEM format).

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};

/// User roles in FaizDB
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Full administrative control (create/drop collections, manage users, cluster config)
    Admin,
    /// Read and write access to collections
    ReadWrite,
    /// Read-only access to collections
    ReadOnly,
}

/// JWT Claims payload
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // Username
    pub role: Role,  // User role
    pub exp: usize,  // Expiration timestamp (Unix seconds)
    pub iat: usize,  // Issued-at timestamp
}

/// Authentication manager with Ed25519 keypair for JWT signing.
pub struct AuthManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl AuthManager {
    /// Create an `AuthManager` from environment variables.
    ///
    /// In production, generate an Ed25519 keypair with:
    /// ```sh
    /// openssl genpkey -algorithm Ed25519 -out private.pem
    /// openssl pkey -in private.pem -pubout -out public.pem
    /// ```
    /// Then supply via `FAIZDB_JWT_PRIVATE_KEY` / `FAIZDB_JWT_PUBLIC_KEY` env vars.
    pub fn new_from_env() -> Self {
        let private_pem = std::env::var("FAIZDB_JWT_PRIVATE_KEY")
            .ok()
            .filter(|s| !s.is_empty());
        let public_pem = std::env::var("FAIZDB_JWT_PUBLIC_KEY")
            .ok()
            .filter(|s| !s.is_empty());

        match (private_pem, public_pem) {
            (Some(priv_pem), Some(pub_pem)) => Self {
                encoding_key: EncodingKey::from_ed_pem(priv_pem.as_bytes())
                    .expect("FAIZDB_JWT_PRIVATE_KEY is not a valid Ed25519 PEM private key"),
                decoding_key: DecodingKey::from_ed_pem(pub_pem.as_bytes())
                    .expect("FAIZDB_JWT_PUBLIC_KEY is not a valid Ed25519 PEM public key"),
            },
            _ => {
                tracing::warn!(
                    "No Ed25519 keys in FAIZDB_JWT_PRIVATE_KEY / FAIZDB_JWT_PUBLIC_KEY. \
                     Generating ephemeral keys — tokens are invalidated on restart."
                );
                Self::generate_ephemeral()
            }
        }
    }

    /// Generate an ephemeral Ed25519 keypair for development/testing.
    pub fn generate_ephemeral() -> Self {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8_doc =
            Ed25519KeyPair::generate_pkcs8(&rng).expect("Failed to generate Ed25519 keypair");
        let key_pair = Ed25519KeyPair::from_pkcs8(pkcs8_doc.as_ref())
            .expect("Failed to parse Ed25519 keypair");

        let private_pem = pem::encode(&pem::Pem::new("PRIVATE KEY", pkcs8_doc.as_ref().to_vec()));
        let public_der = key_pair.public_key().as_ref().to_vec();
        let spki = build_ed25519_spki(&public_der);
        let public_pem = pem::encode(&pem::Pem::new("PUBLIC KEY", spki));

        Self {
            encoding_key: EncodingKey::from_ed_pem(private_pem.as_bytes())
                .expect("Failed to create EdDSA EncodingKey"),
            decoding_key: DecodingKey::from_ed_pem(public_pem.as_bytes())
                .expect("Failed to create EdDSA DecodingKey"),
        }
    }

    /// Backwards-compatible constructor — ignored `_secret`, uses `new_from_env()` internally.
    ///
    /// # Migration
    /// Replace `AuthManager::new(secret)` with `AuthManager::new_from_env()` and set
    /// `FAIZDB_JWT_PRIVATE_KEY` / `FAIZDB_JWT_PUBLIC_KEY` in your environment.
    pub fn new(_secret: impl Into<Vec<u8>>) -> Self {
        Self::new_from_env()
    }

    /// Hash a plaintext password with Argon2id and a random salt.
    pub fn hash_password(password: &str) -> Result<String, String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("Password hashing failed: {e}"))?;
        Ok(hash.to_string())
    }

    /// Verify a plaintext password against an Argon2 hash.
    pub fn verify_password(password: &str, hash: &str) -> bool {
        let parsed_hash = match PasswordHash::new(hash) {
            Ok(h) => h,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    }

    /// Generate a signed EdDSA JWT token for a user.
    pub fn generate_token(
        &self,
        username: &str,
        role: Role,
        valid_seconds: u64,
    ) -> Result<String, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;

        let claims = Claims {
            sub: username.to_string(),
            role,
            exp: now + valid_seconds as usize,
            iat: now,
        };

        encode(&Header::new(Algorithm::EdDSA), &claims, &self.encoding_key)
            .map_err(|e| format!("JWT generation failed: {e}"))
    }

    /// Verify an EdDSA JWT and extract claims.
    pub fn verify_token(&self, token: &str) -> Result<Claims, String> {
        let mut validation = Validation::new(Algorithm::EdDSA);
        validation.validate_exp = true;
        validation.leeway = 0;
        let token_data = decode::<Claims>(token, &self.decoding_key, &validation)
            .map_err(|e| format!("Invalid or expired token: {e}"))?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;

        // RFC 7519: current date/time MUST be before expiration date/time
        if token_data.claims.exp <= now {
            return Err("Token expired".to_string());
        }

        Ok(token_data.claims)
    }
}

/// Build a minimal Ed25519 SubjectPublicKeyInfo (SPKI) DER structure.
/// Required to PEM-encode an Ed25519 raw public key for `jsonwebtoken`.
fn build_ed25519_spki(raw_public_key: &[u8]) -> Vec<u8> {
    // RFC 8410 standard 12-byte header for Ed25519 SubjectPublicKeyInfo:
    // SEQUENCE (42 bytes) -> SEQUENCE (5 bytes: AlgorithmIdentifier id-Ed25519 1.3.101.112) -> BIT STRING (33 bytes, 0 unused bits)
    const ED25519_SPKI_PREFIX: [u8; 12] = [
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    let mut spki = Vec::with_capacity(ED25519_SPKI_PREFIX.len() + raw_public_key.len());
    spki.extend_from_slice(&ED25519_SPKI_PREFIX);
    spki.extend_from_slice(raw_public_key);
    spki
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2_password_hashing() {
        let password = "SuperSecretPassword2026!";
        let hash = AuthManager::hash_password(password).unwrap();
        assert_ne!(password, hash);
        assert!(AuthManager::verify_password(password, &hash));
        assert!(!AuthManager::verify_password("WrongPassword", &hash));
    }

    #[test]
    fn test_eddsa_jwt_token_flow() {
        let auth = AuthManager::generate_ephemeral();
        let token = auth.generate_token("faiz", Role::Admin, 3600).unwrap();
        let claims = auth.verify_token(&token).unwrap();

        assert_eq!(claims.sub, "faiz");
        assert_eq!(claims.role, Role::Admin);

        // JWT must be three dot-separated Base64URL segments
        let parts: Vec<&str> = token.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT must have header.payload.signature");
    }

    #[test]
    fn test_expired_token_rejected() {
        let auth = AuthManager::generate_ephemeral();
        let token = auth.generate_token("faiz", Role::ReadOnly, 0).unwrap();
        assert!(
            auth.verify_token(&token).is_err(),
            "Expired token should be rejected"
        );
    }
}
