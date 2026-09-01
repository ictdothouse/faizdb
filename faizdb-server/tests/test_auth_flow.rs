//! Integration tests for FaizDB authentication flow.
//!
//! Tests the full JWT EdDSA lifecycle: login → token issue → whoami → expired token rejection.

use faizdb_security::auth::{AuthManager, Role};

#[test]
fn test_eddsa_full_auth_flow() {
    let auth = AuthManager::generate_ephemeral();

    // Issue tokens for each role
    let admin_token = auth.generate_token("alice", Role::Admin, 3600).unwrap();
    let rw_token = auth.generate_token("bob", Role::ReadWrite, 3600).unwrap();
    let ro_token = auth.generate_token("carol", Role::ReadOnly, 3600).unwrap();

    // Verify each token returns correct claims
    let admin_claims = auth.verify_token(&admin_token).unwrap();
    assert_eq!(admin_claims.sub, "alice");
    assert_eq!(admin_claims.role, Role::Admin);

    let rw_claims = auth.verify_token(&rw_token).unwrap();
    assert_eq!(rw_claims.sub, "bob");
    assert_eq!(rw_claims.role, Role::ReadWrite);

    let ro_claims = auth.verify_token(&ro_token).unwrap();
    assert_eq!(ro_claims.sub, "carol");
    assert_eq!(ro_claims.role, Role::ReadOnly);
}

#[test]
fn test_expired_token_is_rejected() {
    let auth = AuthManager::generate_ephemeral();
    let token = auth.generate_token("dave", Role::Admin, 0).unwrap();
    assert!(auth.verify_token(&token).is_err(), "Expired token must be rejected");
}

#[test]
fn test_tampered_token_is_rejected() {
    let auth = AuthManager::generate_ephemeral();
    let token = auth.generate_token("eve", Role::Admin, 3600).unwrap();

    // Flip the last character of the signature
    let mut tampered = token.clone();
    let last = tampered.pop().unwrap();
    tampered.push(if last == 'A' { 'B' } else { 'A' });

    assert!(auth.verify_token(&tampered).is_err(), "Tampered signature must be rejected");
}

#[test]
fn test_different_keypairs_cannot_cross_verify() {
    let auth1 = AuthManager::generate_ephemeral();
    let auth2 = AuthManager::generate_ephemeral();

    let token = auth1.generate_token("frank", Role::Admin, 3600).unwrap();
    // auth2 has a different public key — verification must fail
    assert!(auth2.verify_token(&token).is_err(), "Cross-keypair verification must be rejected");
}

#[test]
fn test_argon2_hash_uniqueness() {
    let password = "SamePassword!2026";
    let hash1 = AuthManager::hash_password(password).unwrap();
    let hash2 = AuthManager::hash_password(password).unwrap();
    // Same password must produce different hashes (random salt)
    assert_ne!(hash1, hash2, "Argon2 must use unique salts per hash");
    // Both hashes must verify correctly
    assert!(AuthManager::verify_password(password, &hash1));
    assert!(AuthManager::verify_password(password, &hash2));
}
