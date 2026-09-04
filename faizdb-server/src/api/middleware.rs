//! Security middleware: CORS, Auth, RBAC, Rate Limiting, Payload Size, Audit Logging.

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use tracing::{info, warn};
use uuid::Uuid;

use faizdb_core::cluster::GeoReplicationEngine;
use faizdb_query::DatabaseContext;
use faizdb_security::auth::{AuthManager, Role};

use super::{AuthenticatedUser, BackupScheduleConfig};

/// Shared server state injected into every handler via Axum's `State` extractor.
pub struct AppState {
    pub db: Arc<DatabaseContext>,
    pub auth: Arc<AuthManager>,
    pub user_store: Arc<faizdb_security::UserStore>,
    pub backup_schedule: Arc<parking_lot::RwLock<BackupScheduleConfig>>,
    pub geo_replication: Arc<GeoReplicationEngine>,
    pub metrics: Arc<super::metrics::MetricsCollector>,
}

// ── OpenTelemetry & Correlation ID Tracing Middleware ────────────────────────

pub async fn trace_middleware(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let start = Instant::now();

    // 1. Extract or generate Correlation ID
    let correlation_id = req
        .headers()
        .get("x-correlation-id")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::now_v7().to_string());

    // 2. Extract or generate W3C traceparent (format: 00-{trace_id}-{span_id}-01)
    let traceparent = req
        .headers()
        .get("traceparent")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let trace_id = Uuid::new_v4().to_string().replace('-', "");
            let span_id_raw = Uuid::now_v7().to_string().replace('-', "");
            let span_id = &span_id_raw[0..16];
            format!("00-{trace_id}-{span_id}-01")
        });

    let method = req.method().clone();
    let uri = req.uri().clone();

    // Track active connections
    state
        .metrics
        .active_connections
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

    let mut res = next.run(req).await;

    state
        .metrics
        .active_connections
        .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    let elapsed = start.elapsed();
    state.metrics.record_query_latency(elapsed);

    if method == Method::GET {
        state
            .metrics
            .queries_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    } else if method == Method::POST {
        state
            .metrics
            .inserts_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    } else if method == Method::DELETE {
        state
            .metrics
            .deletes_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    } else if method == Method::PUT || method == Method::PATCH {
        state
            .metrics
            .updates_total
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    // Set correlation headers on response
    if let Ok(val) = HeaderValue::from_str(&correlation_id) {
        res.headers_mut().insert("x-correlation-id", val);
    }
    if let Ok(val) = HeaderValue::from_str(&traceparent) {
        res.headers_mut().insert("traceparent", val);
    }

    info!(
        correlation_id = %correlation_id,
        traceparent = %traceparent,
        method = %method,
        uri = %uri,
        latency_ms = %elapsed.as_millis(),
        status = %res.status().as_u16(),
        "HTTP request processed"
    );

    res
}

// ── CORS ────────────────────────────────────────────────────────────────────

pub async fn cors_middleware(req: Request<Body>, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();

    let allowed_origin_env = std::env::var("FAIZDB_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "http://localhost:27020".to_string());
    let allowed_origins: Vec<&str> = allowed_origin_env.split(',').collect();

    let is_options = req.method() == Method::OPTIONS;
    let mut response = if is_options {
        Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap()
    } else {
        next.run(req).await
    };

    let headers = response.headers_mut();
    if allowed_origins.contains(&"*") || allowed_origins.contains(&origin.as_str()) {
        if origin.is_empty() {
            headers.insert(
                header::ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static("*"),
            );
        } else if let Ok(val) = HeaderValue::from_str(&origin) {
            headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
        }
    }
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS, PATCH"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("Authorization, Content-Type, Accept"),
    );
    headers.insert(
        header::ACCESS_CONTROL_MAX_AGE,
        HeaderValue::from_static("86400"),
    );
    response
}

// ── AUTH MIDDLEWARE ─────────────────────────────────────────────────────────

pub async fn client_auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let raw_token = extract_bearer_token(&req).or_else(|| extract_query_token(&req));
    let token = match raw_token {
        Some(t) => t,
        None => {
            warn!("[Auth] Missing token — 401");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };
    match state.auth.verify_token(&token) {
        Ok(claims) => {
            req.extensions_mut().insert(AuthenticatedUser {
                username: claims.sub.clone(),
                role: claims.role,
            });
            info!("[Auth] Authenticated: {} ({:?})", claims.sub, claims.role);
            Ok(next.run(req).await)
        }
        Err(e) => {
            warn!("[Auth] JWT validation failed: {}", e);
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

pub async fn rbac_write_middleware(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    if let Some(user) = req.extensions().get::<AuthenticatedUser>() {
        match user.role {
            Role::Admin | Role::ReadWrite => return Ok(next.run(req).await),
            Role::ReadOnly => {
                warn!(
                    "[RBAC] ReadOnly user '{}' attempted write — 403",
                    user.username
                );
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

pub async fn rbac_admin_middleware(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    if let Some(user) = req.extensions().get::<AuthenticatedUser>() {
        if user.role == Role::Admin {
            return Ok(next.run(req).await);
        }
        warn!(
            "[RBAC] Non-admin '{}' attempted admin op — 403",
            user.username
        );
        return Err(StatusCode::FORBIDDEN);
    }
    Err(StatusCode::UNAUTHORIZED)
}

pub async fn cluster_auth_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let expected = std::env::var("FAIZDB_CLUSTER_TOKEN")
        .unwrap_or_else(|_| "faizdb-cluster-secret".to_string());
    if let Some(auth_value) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_value.to_str() {
            if auth_str.starts_with("Bearer ") && auth_str.trim_start_matches("Bearer ") == expected
            {
                return Ok(next.run(req).await);
            }
        }
    }
    Err(StatusCode::UNAUTHORIZED)
}

pub fn extract_bearer_token(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get(header::AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
        .map(|t| t.trim().to_string())
}

pub fn extract_query_token(req: &Request<Body>) -> Option<String> {
    let query = req.uri().query()?;
    for pair in query.split('&') {
        if let Some(val) = pair.strip_prefix("token=") {
            return Some(val.to_string());
        }
    }
    None
}

// ── RATE LIMITER & BLOCKLIST ────────────────────────────────────────────────

static BLOCKLIST: std::sync::OnceLock<DashMap<String, u32>> = std::sync::OnceLock::new();
static RATE_LIMITER: std::sync::OnceLock<DashMap<String, (u64, Instant)>> =
    std::sync::OnceLock::new();

pub fn get_blocklist() -> &'static DashMap<String, u32> {
    BLOCKLIST.get_or_init(DashMap::new)
}
pub fn get_rate_limiter() -> &'static DashMap<String, (u64, Instant)> {
    RATE_LIMITER.get_or_init(DashMap::new)
}

fn is_internal_ip(ip: &str) -> bool {
    if ip == "unknown" {
        return false;
    }
    if ip.starts_with("127.") || ip == "::1" {
        return true;
    }
    if ip.starts_with("10.") || ip.starts_with("192.168.") || ip.starts_with("100.64.") {
        return true;
    }
    if let Some(second_octet) = ip.strip_prefix("172.").and_then(|s| s.split('.').next()) {
        if let Ok(n) = second_octet.parse::<u8>() {
            if (16..=31).contains(&n) {
                return true;
            }
        }
    }
    false
}

pub async fn rate_limit_middleware(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let mut ip = "unknown".to_string();
    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            ip = s.split(',').next().unwrap_or("unknown").trim().to_string();
        }
    }
    if ip == "unknown" {
        if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<std::net::SocketAddr>>()
        {
            ip = addr.ip().to_string();
        }
    }
    if is_internal_ip(&ip) {
        return Ok(next.run(req).await);
    }

    let expected_token =
        std::env::var("FAIZDB_API_KEY").unwrap_or_else(|_| "faizdb-secret-key".to_string());
    if let Some(auth_val) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_val.to_str() {
            if auth_str.trim_start_matches("Bearer ").trim() == expected_token {
                return Ok(next.run(req).await);
            }
        }
    }

    if get_blocklist().contains_key(&ip) {
        warn!("[Blocklist] Blocked IP {} attempted access", ip);
        return Err(StatusCode::FORBIDDEN);
    }

    {
        let limiter = get_rate_limiter();
        let mut entry = limiter.entry(ip.clone()).or_insert((0, Instant::now()));
        let now = Instant::now();
        let window = Duration::from_secs(10);
        let limit: u64 = std::env::var("FAIZDB_RATE_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        let strike_threshold: u32 = std::env::var("FAIZDB_BAN_STRIKES")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);

        if now.duration_since(entry.1) > window {
            entry.0 = 1;
            entry.1 = now;
        } else {
            entry.0 += 1;
            if entry.0 > limit {
                let mut strikes = get_blocklist().entry(ip.clone()).or_insert(0);
                *strikes += 1;
                let current_strikes = *strikes;
                drop(strikes);
                if current_strikes >= strike_threshold {
                    warn!(
                        "[Blocklist] IP {} permanently banned after {} strikes",
                        ip, current_strikes
                    );
                    return Err(StatusCode::FORBIDDEN);
                }
                warn!(
                    "[RateLimit] IP {} strike {}/{} (429)",
                    ip, current_strikes, strike_threshold
                );
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
        }
    }
    Ok(next.run(req).await)
}

// ── PAYLOAD SIZE ────────────────────────────────────────────────────────────

pub async fn payload_size_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let max_mb: u64 = std::env::var("FAIZDB_MAX_PAYLOAD_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let max_bytes = max_mb * 1024 * 1024;
    if let Some(content_length) = req.headers().get(header::CONTENT_LENGTH) {
        if let Ok(length_str) = content_length.to_str() {
            if let Ok(length) = length_str.parse::<u64>() {
                if length > max_bytes {
                    warn!(
                        "[PayloadLimit] Request {} bytes exceeds {}MB",
                        length, max_mb
                    );
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
            }
        }
    }
    Ok(next.run(req).await)
}

// ── REQUEST ID ──────────────────────────────────────────────────────────────

pub async fn request_id_middleware(mut req: Request<Body>, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();
    req.extensions_mut().insert(request_id.clone());
    let mut response = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-faizdb-request-id", val);
    }
    response
}

// ── AUDIT LOG ───────────────────────────────────────────────────────────────

pub fn audit_log(event: &str, ip: &str, path: &str, status: u16, request_id: Option<&str>) {
    let log_entry = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "event": event, "ip": ip, "path": path,
        "status": status,
        "request_id": request_id.unwrap_or("-"),
        "engine": "FaizDB"
    });
    let line = format!("{}\n", log_entry);
    tokio::spawn(async move {
        let log_path =
            std::env::var("FAIZDB_AUDIT_LOG").unwrap_or_else(|_| "./logs/audit.jsonl".to_string());
        let log_dir = std::path::Path::new(&log_path)
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let _ = tokio::fs::create_dir_all(log_dir).await;
        use tokio::io::AsyncWriteExt;
        if let Ok(mut file) = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
        {
            let _ = file.write_all(line.as_bytes()).await;
        }
    });
}

pub async fn audit_middleware(req: Request<Body>, next: Next) -> Response {
    let ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("-").trim().to_string())
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip().to_string())
        })
        .unwrap_or_else(|| "-".to_string());

    let path = req.uri().path().to_string();
    let request_id = req
        .extensions()
        .get::<String>()
        .cloned()
        .unwrap_or_else(|| "-".to_string());
    let method = req.method().clone();
    let response = next.run(req).await;
    let status = response.status().as_u16();

    let is_write = matches!(
        method,
        Method::POST | Method::DELETE | Method::PUT | Method::PATCH
    );
    let is_error = status >= 400;
    if is_write || is_error {
        let event = if status == 401 {
            "auth_failure"
        } else if status == 403 {
            "access_denied"
        } else if status == 429 {
            "rate_limited"
        } else if status == 413 {
            "payload_too_large"
        } else if is_write {
            "write_operation"
        } else {
            "error"
        };
        audit_log(event, &ip, &path, status, Some(&request_id));
    }
    response
}
