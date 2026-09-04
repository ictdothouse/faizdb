//! In-memory and persistent User Store for Role-Based Access Control and authentication.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::auth::{AuthManager, Role};

/// Stored user record with hashed password
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub username: String,
    pub password_hash: String,
    pub role: Role,
    pub created_at: u64,
}

/// Safe public user representation without sensitive fields
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub username: String,
    pub role: Role,
    pub created_at: u64,
}

/// Central User Store managing accounts, password verification, and roles
#[derive(Clone)]
pub struct UserStore {
    users: Arc<RwLock<HashMap<String, UserRecord>>>,
}

impl Default for UserStore {
    fn default() -> Self {
        Self::new()
    }
}

impl UserStore {
    /// Create a new user store and initialize default accounts from environment
    pub fn new() -> Self {
        let store = Self {
            users: Arc::new(RwLock::new(HashMap::new())),
        };

        // Initialize default administrator from environment
        let admin_user = std::env::var("FAIZDB_ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
        let admin_pass =
            std::env::var("FAIZDB_ADMIN_PASS").unwrap_or_else(|_| "faizdb-admin-2026".to_string());

        let _ = store.create_user(&admin_user, &admin_pass, Role::Admin);

        // Optional default ReadWrite and ReadOnly users
        if let (Ok(rw_user), Ok(rw_pass)) = (
            std::env::var("FAIZDB_RW_USER"),
            std::env::var("FAIZDB_RW_PASS"),
        ) {
            if !rw_user.is_empty() && !rw_pass.is_empty() {
                let _ = store.create_user(&rw_user, &rw_pass, Role::ReadWrite);
            }
        }
        if let (Ok(ro_user), Ok(ro_pass)) = (
            std::env::var("FAIZDB_RO_USER"),
            std::env::var("FAIZDB_RO_PASS"),
        ) {
            if !ro_user.is_empty() && !ro_pass.is_empty() {
                let _ = store.create_user(&ro_user, &ro_pass, Role::ReadOnly);
            }
        }

        store
    }

    /// Create a new user with an Argon2id hashed password
    pub fn create_user(&self, username: &str, password: &str, role: Role) -> Result<(), String> {
        let username_clean = username.trim();
        if username_clean.is_empty() {
            return Err("Username cannot be empty".to_string());
        }
        if password.len() < 4 {
            return Err("Password must be at least 4 characters".to_string());
        }

        let mut users = self.users.write().unwrap();
        if users.contains_key(username_clean) {
            return Err(format!("User '{username_clean}' already exists"));
        }

        let hash = AuthManager::hash_password(password)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        users.insert(
            username_clean.to_string(),
            UserRecord {
                username: username_clean.to_string(),
                password_hash: hash,
                role,
                created_at: now,
            },
        );

        Ok(())
    }

    /// Verify credentials and return the user's role on success
    pub fn authenticate(&self, username: &str, password: &str) -> Option<Role> {
        let users = self.users.read().unwrap();
        let record = users.get(username.trim())?;
        if AuthManager::verify_password(password, &record.password_hash) {
            Some(record.role)
        } else {
            None
        }
    }

    /// List all registered users
    pub fn list_users(&self) -> Vec<UserInfo> {
        let users = self.users.read().unwrap();
        users
            .values()
            .map(|u| UserInfo {
                username: u.username.clone(),
                role: u.role,
                created_at: u.created_at,
            })
            .collect()
    }

    /// Delete a user (preventing deletion of the last admin)
    pub fn delete_user(&self, username: &str) -> Result<bool, String> {
        let mut users = self.users.write().unwrap();
        let record = match users.get(username.trim()) {
            Some(r) => r,
            None => return Ok(false),
        };

        if record.role == Role::Admin {
            let admin_count = users.values().filter(|u| u.role == Role::Admin).count();
            if admin_count <= 1 {
                return Err("Cannot delete the last administrator user".to_string());
            }
        }

        users.remove(username.trim());
        Ok(true)
    }

    /// Update a user's password
    pub fn update_password(&self, username: &str, new_password: &str) -> Result<(), String> {
        if new_password.len() < 4 {
            return Err("Password must be at least 4 characters".to_string());
        }
        let mut users = self.users.write().unwrap();
        let record = users
            .get_mut(username.trim())
            .ok_or_else(|| format!("User '{username}' not found"))?;

        let new_hash = AuthManager::hash_password(new_password)?;
        record.password_hash = new_hash;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_store_create_and_authenticate() {
        let store = UserStore::new();
        // Default admin should authenticate
        assert_eq!(
            store.authenticate("admin", "faizdb-admin-2026"),
            Some(Role::Admin)
        );
        assert_eq!(store.authenticate("admin", "wrong_pass"), None);

        // Create new ReadWrite user
        store
            .create_user("analyst", "secret123", Role::ReadWrite)
            .unwrap();
        assert_eq!(
            store.authenticate("analyst", "secret123"),
            Some(Role::ReadWrite)
        );

        // Duplicate user rejected
        assert!(store
            .create_user("analyst", "another_pass", Role::ReadOnly)
            .is_err());

        // Update password
        store.update_password("analyst", "new_secret_456").unwrap();
        assert_eq!(
            store.authenticate("analyst", "new_secret_456"),
            Some(Role::ReadWrite)
        );
        assert_eq!(store.authenticate("analyst", "secret123"), None);

        // Cannot delete last admin
        assert!(store.delete_user("admin").is_err());

        // Can delete normal user
        assert!(store.delete_user("analyst").unwrap());
        assert_eq!(store.authenticate("analyst", "new_secret_456"), None);
    }
}
