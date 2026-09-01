//! Authentication, Password Hashing, JWT Tokens, and Role-Based Access Control (RBAC).

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
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
    pub sub: String,       // Username
    pub role: Role,        // User role
    pub exp: usize,        // Expiration timestamp
}

/// Authentication manager
pub struct AuthManager {
    jwt_secret: Vec<u8>,
}

impl AuthManager {
    pub fn new(jwt_secret: impl Into<Vec<u8>>) -> Self {
        Self {
            jwt_secret: jwt_secret.into(),
        }
    }

    /// Hash a plaintext password with Argon2id and a random salt
    pub fn hash_password(password: &str) -> Result<String, String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        let hash = argon2
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| format!("Password hashing failed: {e}"))?;
        Ok(hash.to_string())
    }

    /// Verify a plaintext password against an Argon2 hash
    pub fn verify_password(password: &str, hash: &str) -> bool {
        let parsed_hash = match PasswordHash::new(hash) {
            Ok(h) => h,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    }

    /// Generate a JWT token for a user with role and duration
    pub fn generate_token(&self, username: &str, role: Role, valid_seconds: u64) -> Result<String, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let claims = Claims {
            sub: username.to_string(),
            role,
            exp: (now + valid_seconds) as usize,
        };

        encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(&self.jwt_secret),
        )
        .map_err(|e| format!("JWT generation failed: {e}"))
    }

    /// Verify a JWT token and extract claims
    pub fn verify_token(&self, token: &str) -> Result<Claims, String> {
        let token_data = decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.jwt_secret),
            &Validation::default(),
        )
        .map_err(|e| format!("Invalid or expired token: {e}"))?;

        Ok(token_data.claims)
    }
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
    fn test_jwt_token_flow() {
        let auth = AuthManager::new(b"test_jwt_secret_key_123456789012");
        let token = auth.generate_token("faiz", Role::Admin, 3600).unwrap();
        let claims = auth.verify_token(&token).unwrap();

        assert_eq!(claims.sub, "faiz");
        assert_eq!(claims.role, Role::Admin);
    }
}
