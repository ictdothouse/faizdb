//! Enterprise Security Audit Logging.
//!
//! Provides structured SIEM-ready security audit logging for compliance
//! (SOC2, HIPAA, ISO27001), tracking authentication, RBAC checks, and cryptographic events.

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Categories of security audit actions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    LoginSuccess,
    LoginFailure,
    TokenGenerated,
    TokenRevoked,
    AccessGranted,
    AccessDenied,
    UserCreated,
    UserDeleted,
    PasswordChanged,
    KeyRotated,
    ConfigurationChanged,
}

/// A structured security audit event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub actor: String,
    pub action: AuditAction,
    pub resource: String,
    pub client_ip: Option<String>,
    pub success: bool,
    pub details: Option<serde_json::Value>,
}

impl AuditEvent {
    pub fn new(
        actor: impl Into<String>,
        action: AuditAction,
        resource: impl Into<String>,
        success: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            actor: actor.into(),
            action,
            resource: resource.into(),
            client_ip: None,
            success,
            details: None,
        }
    }

    pub fn with_ip(mut self, ip: impl Into<String>) -> Self {
        self.client_ip = Some(ip.into());
        self
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Thread-safe Security Audit Logger with ring buffer and optional file append
#[derive(Debug, Clone)]
pub struct AuditLogger {
    ring_buffer: Arc<RwLock<VecDeque<AuditEvent>>>,
    max_memory_events: usize,
    log_file: Option<PathBuf>,
}

impl AuditLogger {
    /// Create a new in-memory audit logger with specified ring buffer capacity
    pub fn new(max_memory_events: usize) -> Self {
        Self {
            ring_buffer: Arc::new(RwLock::new(VecDeque::with_capacity(max_memory_events))),
            max_memory_events,
            log_file: None,
        }
    }

    /// Create an audit logger that writes events to disk in append-only JSON Lines format
    pub fn with_file(max_memory_events: usize, path: impl AsRef<Path>) -> std::io::Result<Self> {
        let p = path.as_ref().to_path_buf();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }

        Ok(Self {
            ring_buffer: Arc::new(RwLock::new(VecDeque::with_capacity(max_memory_events))),
            max_memory_events,
            log_file: Some(p),
        })
    }

    /// Record a security event
    pub fn log(&self, event: AuditEvent) {
        // Write to file if enabled
        if let Some(ref path) = self.log_file {
            if let Ok(line) = serde_json::to_string(&event) {
                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(f, "{line}");
                }
            }
        }

        // Store in in-memory ring buffer
        let mut buf = self.ring_buffer.write();
        if buf.len() >= self.max_memory_events {
            buf.pop_front();
        }
        buf.push_back(event);
    }

    /// Convenience record helper
    pub fn record(
        &self,
        actor: impl Into<String>,
        action: AuditAction,
        resource: impl Into<String>,
        success: bool,
        client_ip: Option<String>,
    ) {
        let mut event = AuditEvent::new(actor, action, resource, success);
        if let Some(ip) = client_ip {
            event = event.with_ip(ip);
        }
        self.log(event);
    }

    /// Fetch up to N recent events (most recent first)
    pub fn recent_events(&self, count: usize) -> Vec<AuditEvent> {
        let buf = self.ring_buffer.read();
        buf.iter().rev().take(count).cloned().collect()
    }

    /// Count total events in memory
    pub fn len(&self) -> usize {
        self.ring_buffer.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.ring_buffer.read().is_empty()
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new(1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_logger_ring_buffer_and_file() {
        let temp_file = std::env::temp_dir().join(format!("audit_test_{}.jsonl", Uuid::new_v4()));
        let logger = AuditLogger::with_file(3, &temp_file).unwrap();

        logger.record("admin", AuditAction::LoginSuccess, "auth_portal", true, Some("127.0.0.1".into()));
        logger.record("hacker", AuditAction::LoginFailure, "auth_portal", false, Some("192.168.1.50".into()));
        logger.record("admin", AuditAction::KeyRotated, "aes_master_key", true, None);
        logger.record("user1", AuditAction::AccessDenied, "users_collection", false, None);

        assert_eq!(logger.len(), 3, "Ring buffer should cap at 3");
        let recent = logger.recent_events(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].action, AuditAction::AccessDenied);
        assert_eq!(recent[1].action, AuditAction::KeyRotated);

        // Verify file content exists
        let content = fs::read_to_string(&temp_file).unwrap();
        assert!(content.contains("admin"));
        assert!(content.contains("hacker"));

        let _ = fs::remove_file(temp_file);
    }
}
