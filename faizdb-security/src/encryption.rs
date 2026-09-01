//! Zero-Trust Encryption Engine (AES-256-GCM) for data-at-rest and field-level security.

use ring::aead::{Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};

/// Custom single-nonce sequence for AEAD operations
struct OneNonce(Option<[u8; NONCE_LEN]>);

impl NonceSequence for OneNonce {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        let bytes = self.0.take().ok_or(ring::error::Unspecified)?;
        Nonce::try_assume_unique_for_key(&bytes)
    }
}

/// Encrypted payload with random 96-bit nonce prepended
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedData {
    /// Nonce (12 bytes) + Ciphertext + Auth Tag (16 bytes)
    pub ciphertext: Vec<u8>,
}

/// AES-256-GCM cipher manager
pub struct Cipher {
    key_bytes: [u8; 32],
    rng: SystemRandom,
}

impl Cipher {
    /// Create a cipher from a 32-byte (256-bit) secret key
    pub fn new(key_bytes: [u8; 32]) -> Self {
        Self {
            key_bytes,
            rng: SystemRandom::new(),
        }
    }

    /// Create a cipher by deriving a 256-bit key from a user passphrase using SHA-256
    pub fn from_passphrase(passphrase: &str) -> Self {
        let hash = ring::digest::digest(&ring::digest::SHA256, passphrase.as_bytes());
        let mut key = [0u8; 32];
        key.copy_from_slice(hash.as_ref());
        Self::new(key)
    }

    /// Generate a cryptographically secure random 256-bit key
    pub fn generate_key() -> Result<[u8; 32], String> {
        let rng = SystemRandom::new();
        let mut key = [0u8; 32];
        rng.fill(&mut key).map_err(|_| "Failed to generate random key".to_string())?;
        Ok(key)
    }

    /// Encrypt plaintext bytes using AES-256-GCM with a fresh unique nonce
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<EncryptedData, String> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        self.rng.fill(&mut nonce_bytes).map_err(|_| "Failed to generate nonce")?;

        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.key_bytes)
            .map_err(|_| "Failed to create AEAD key")?;
        let mut sealing_key = SealingKey::new(unbound_key, OneNonce(Some(nonce_bytes)));

        let mut in_out = plaintext.to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut in_out)
            .map_err(|_| "Encryption failed")?;

        // Output: [12 bytes nonce] + [ciphertext + tag]
        let mut full_output = Vec::with_capacity(NONCE_LEN + in_out.len());
        full_output.extend_from_slice(&nonce_bytes);
        full_output.extend_from_slice(&in_out);

        Ok(EncryptedData {
            ciphertext: full_output,
        })
    }

    /// Decrypt ciphertext produced by `encrypt`
    pub fn decrypt(&self, encrypted: &EncryptedData) -> Result<Vec<u8>, String> {
        if encrypted.ciphertext.len() < NONCE_LEN + 16 {
            return Err("Ciphertext is too short".to_string());
        }

        let (nonce_bytes, ciphertext_with_tag) = encrypted.ciphertext.split_at(NONCE_LEN);
        let mut nonce_arr = [0u8; NONCE_LEN];
        nonce_arr.copy_from_slice(nonce_bytes);

        let unbound_key = UnboundKey::new(&AES_256_GCM, &self.key_bytes)
            .map_err(|_| "Failed to create AEAD key")?;
        let mut opening_key = OpeningKey::new(unbound_key, OneNonce(Some(nonce_arr)));

        let mut in_out = ciphertext_with_tag.to_vec();
        let decrypted_slice = opening_key
            .open_in_place(Aad::empty(), &mut in_out)
            .map_err(|_| "Decryption failed or invalid key/tampered data")?;

        Ok(decrypted_slice.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aes256_encrypt_decrypt() {
        let key = Cipher::generate_key().unwrap();
        let cipher = Cipher::new(key);

        let original = b"FaizDB: World's fastest AI-native database";
        let encrypted = cipher.encrypt(original).unwrap();
        assert_ne!(encrypted.ciphertext, original);

        let decrypted = cipher.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, original);
    }

    #[test]
    fn test_tamper_detection() {
        let key = Cipher::generate_key().unwrap();
        let cipher = Cipher::new(key);

        let original = b"Secret payload";
        let mut encrypted = cipher.encrypt(original).unwrap();

        // Tamper with a byte
        let len = encrypted.ciphertext.len();
        encrypted.ciphertext[len - 1] ^= 0x55;

        // Decryption must fail due to auth tag verification
        assert!(cipher.decrypt(&encrypted).is_err());
    }
}
