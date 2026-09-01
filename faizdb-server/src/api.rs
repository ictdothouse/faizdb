//! REST API handlers and Router for FaizDB Server.

use std::sync::Arc;
use std::time::{Instant, Duration};
use std::net::SocketAddr;
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, State,
    },
    http::{header, HeaderValue, Method, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use dashmap::DashMap;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use uuid::Uuid;
use tower::ServiceBuilder;
use tower_http::timeout::TimeoutLayer;

use faizdb_core::cluster::{AppendEntriesArgs, RequestVoteArgs};
use faizdb_core::document::model::Document;
use faizdb_core::stream::ChangeEvent;
use faizdb_query::{parse_query, DatabaseContext};
use faizdb_security::auth::{AuthManager, Role, Claims};

/// Injected into request extensions after JWT validation — available to all handlers.
#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub username: String,
    pub role: Role,
}

/// Shared server state
pub struct AppState {
    pub db: Arc<DatabaseContext>,
    pub auth: Arc<AuthManager>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub username: String,
    pub role: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize)]
pub struct QueryRequest {
    pub query: String,
}

#[derive(Debug, Deserialize)]
pub struct JoinClusterRequest {
    pub peer_id: String,
    pub peer_address: String,
}

#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn err(message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message.into()),
        }
    }
}

/// Restricted CORS Middleware allowing specific origins
async fn cors_middleware(req: Request<Body>, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
        
    let allowed_origin_env = std::env::var("FAIZDB_ALLOWED_ORIGINS").unwrap_or_else(|_| "http://localhost:27020".to_string());
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
    
    // Set CORS origin dynamically based on allowed origins list
    if allowed_origins.contains(&"*") || allowed_origins.contains(&origin.as_str()) {
        if origin.is_empty() {
             headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
        } else {
            if let Ok(val) = HeaderValue::from_str(&origin) {
                headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, val);
            }
        }
    }

    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, POST, PUT, DELETE, OPTIONS, PATCH"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("Authorization, Content-Type, Accept"));
    headers.insert(header::ACCESS_CONTROL_MAX_AGE, HeaderValue::from_static("86400"));
    
    response
}

/// Client Authentication Middleware — validates JWT from Bearer token or ?token= query param.
/// On success, injects `AuthenticatedUser` into request extensions for downstream handlers.
async fn client_auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. Extract raw token string (header or query param for WebSocket)
    let raw_token = extract_bearer_token(&req)
        .or_else(|| extract_query_token(&req));

    let token = match raw_token {
        Some(t) => t,
        None => {
            warn!("[Auth] Missing token — 401");
            return Err(StatusCode::UNAUTHORIZED);
        }
    };

    // 2. Validate JWT and extract claims
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

/// RBAC Write Guard — only Admin and ReadWrite roles may mutate data.
async fn rbac_write_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if let Some(user) = req.extensions().get::<AuthenticatedUser>() {
        match user.role {
            Role::Admin | Role::ReadWrite => return Ok(next.run(req).await),
            Role::ReadOnly => {
                warn!("[RBAC] ReadOnly user '{}' attempted write operation — 403", user.username);
                return Err(StatusCode::FORBIDDEN);
            }
        }
    }
    // No user extension means auth middleware was bypassed — should not happen
    Err(StatusCode::UNAUTHORIZED)
}

fn extract_bearer_token(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get(header::AUTHORIZATION)?
        .to_str().ok()?
        .strip_prefix("Bearer ")
        .map(|t| t.trim().to_string())
}

fn extract_query_token(req: &Request<Body>) -> Option<String> {
    let query = req.uri().query()?;
    for pair in query.split('&') {
        if let Some(val) = pair.strip_prefix("token=") {
            return Some(val.to_string());
        }
    }
    None
}


/// Cluster Authentication Middleware (Validates RPC tokens)
async fn cluster_auth_middleware(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let expected_token = std::env::var("FAIZDB_CLUSTER_TOKEN").unwrap_or_else(|_| "faizdb-cluster-secret".to_string());
    
    if let Some(auth_value) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_value.to_str() {
            if auth_str.starts_with("Bearer ") {
                let token = auth_str.trim_start_matches("Bearer ");
                if token == expected_token {
                    return Ok(next.run(req).await);
                }
            }
        }
    }
    
    Err(StatusCode::UNAUTHORIZED)
}

// ─────────────────────────────────────────────────────────────────────────────
// Enterprise Security Middleware Stack
// All checks happen in microseconds with zero blocking I/O on the request path.
// ─────────────────────────────────────────────────────────────────────────────

/// [1] REQUEST ID — Injects a unique trace ID into every request & response.
/// Zero cost: one UUID v4 generated per request (~100ns), no I/O.
async fn request_id_middleware(mut req: Request<Body>, next: Next) -> Response {
    let request_id = Uuid::new_v4().to_string();
    req.extensions_mut().insert(request_id.clone());
    let mut response = next.run(req).await;
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-faizdb-request-id", val);
    }
    response
}

/// [2] PAYLOAD SIZE LIMITER — Rejects oversized request bodies before parsing.
/// Zero cost: reads Content-Length header only, never touches the body bytes.
/// Default: 10MB. Override with FAIZDB_MAX_PAYLOAD_MB env var.
async fn payload_size_middleware(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let max_mb: u64 = std::env::var("FAIZDB_MAX_PAYLOAD_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);
    let max_bytes = max_mb * 1024 * 1024;

    if let Some(content_length) = req.headers().get(header::CONTENT_LENGTH) {
        if let Ok(length_str) = content_length.to_str() {
            if let Ok(length) = length_str.parse::<u64>() {
                if length > max_bytes {
                    warn!("[PayloadLimit] Request body {} bytes exceeds {}MB limit", length, max_mb);
                    return Err(StatusCode::PAYLOAD_TOO_LARGE);
                }
            }
        }
    }
    Ok(next.run(req).await)
}

// Persistent blocklist: IPs permanently banned after repeated violations.
// Stored in memory — survives for the lifetime of the server process.
static BLOCKLIST: std::sync::OnceLock<DashMap<String, u32>> = std::sync::OnceLock::new();
static RATE_LIMITER: std::sync::OnceLock<DashMap<String, (u64, Instant)>> = std::sync::OnceLock::new();

fn get_blocklist() -> &'static DashMap<String, u32> {
    BLOCKLIST.get_or_init(DashMap::new)
}

fn get_rate_limiter() -> &'static DashMap<String, (u64, Instant)> {
    RATE_LIMITER.get_or_init(DashMap::new)
}

/// Returns true if the IP belongs to an internal / trusted network (LAN, VPC, Loopback).
/// These hosts are game servers, microservices, or cluster nodes — never public clients.
fn is_internal_ip(ip: &str) -> bool {
    if ip == "unknown" {
        return false;
    }
    // Loopback
    if ip.starts_with("127.") || ip == "::1" {
        return true;
    }
    // RFC-1918 private ranges (LAN / VPC)
    if ip.starts_with("10.")
        || ip.starts_with("192.168.")
        || ip.starts_with("100.64.")  // CGNAT (Tailscale, WireGuard, AWS VPC)
    {
        return true;
    }
    // 172.16.0.0 – 172.31.255.255
    if let Some(second_octet) = ip.strip_prefix("172.").and_then(|s| s.split('.').next()) {
        if let Ok(n) = second_octet.parse::<u8>() {
            if (16..=31).contains(&n) {
                return true;
            }
        }
    }
    false
}

/// Enterprise-grade Context-Aware Rate Limiter:
/// - Internal IPs (LAN / VPC / Loopback) → Bypass. Game servers, microservices are free.
/// - Authenticated requests (valid Bearer token) → Bypass. Registered clients are trusted.
/// - Public anonymous requests → 100 requests / 10s hard limit (brute-force / DDoS shield).
async fn rate_limit_middleware(
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // --- 1. Resolve client IP ---
    let mut ip = "unknown".to_string();

    if let Some(forwarded) = req.headers().get("x-forwarded-for") {
        if let Ok(s) = forwarded.to_str() {
            ip = s.split(',').next().unwrap_or("unknown").trim().to_string();
        }
    }
    if ip == "unknown" {
        if let Some(ConnectInfo(addr)) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
            ip = addr.ip().to_string();
        }
    }

    // --- 2. VPC / LAN Whitelist: internal game servers & microservices bypass all limits ---
    if is_internal_ip(&ip) {
        return Ok(next.run(req).await);
    }

    // --- 3. Authenticated Bypass: registered clients with valid API key bypass limits ---
    let expected_token = std::env::var("FAIZDB_API_KEY")
        .unwrap_or_else(|_| "faizdb-secret-key".to_string());
    if let Some(auth_val) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(auth_str) = auth_val.to_str() {
            let token = auth_str.trim_start_matches("Bearer ").trim();
            if token == expected_token {
                return Ok(next.run(req).await);
            }
        }
    }

    // --- 3b. Persistent Blocklist: ban IPs that repeatedly violate rate limits ---
    {
        let blocklist = get_blocklist();
        if blocklist.contains_key(&ip) {
            warn!("[Blocklist] Permanently blocked IP {} attempted access", ip);
            return Err(StatusCode::FORBIDDEN);
        }
    }

    // --- 4. Public anonymous throttle: sliding window rate limit ---
    {
        let limiter = get_rate_limiter();
        let mut entry = limiter.entry(ip.clone()).or_insert((0, Instant::now()));

        let now = Instant::now();
        let window = Duration::from_secs(10);
        let limit: u64 = std::env::var("FAIZDB_RATE_LIMIT")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);
        // Strike threshold: 3 window violations → permanent ban
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
                // Increment strike counter and potentially ban
                let mut strikes = get_blocklist().entry(ip.clone()).or_insert(0);
                *strikes += 1;
                let current_strikes = *strikes;
                drop(strikes);

                if current_strikes >= strike_threshold {
                    warn!(
                        "[Blocklist] IP {} hit {} strikes — permanently banned",
                        ip, current_strikes
                    );
                    return Err(StatusCode::FORBIDDEN);
                }

                warn!(
                    "[RateLimit] IP {} exceeded limit — strike {}/{} (429)",
                    ip, current_strikes, strike_threshold
                );
                return Err(StatusCode::TOO_MANY_REQUESTS);
            }
        }
    }

    Ok(next.run(req).await)
}

/// [4] ASYNC AUDIT LOGGER — Logs security events to a dedicated audit log file.
/// Zero cost on request path: spawns a background task to write, never blocks.
/// Log format: JSON-Lines (machine-readable, compatible with log aggregators like ELK/Loki).
pub fn audit_log(event: &str, ip: &str, path: &str, status: u16, request_id: Option<&str>) {
    let log_entry = serde_json::json!({
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "event": event,
        "ip": ip,
        "path": path,
        "status": status,
        "request_id": request_id.unwrap_or("-"),
        "engine": "FaizDB"
    });
    let line = format!("{}
", log_entry);
    // Spawn async so audit write never blocks the response path
    tokio::spawn(async move {
        let log_path = std::env::var("FAIZDB_AUDIT_LOG")
            .unwrap_or_else(|_| "./logs/audit.jsonl".to_string());
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

async fn audit_middleware(req: Request<Body>, next: Next) -> Response {
    let ip = req.headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.split(',').next().unwrap_or("-").trim().to_string())
        .or_else(|| {
            req.extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0.ip().to_string())
        })
        .unwrap_or_else(|| "-".to_string());

    let path = req.uri().path().to_string();
    let request_id = req.extensions()
        .get::<String>()
        .cloned()
        .unwrap_or_else(|| "-".to_string());

    let method = req.method().clone();
    let response = next.run(req).await;
    let status = response.status().as_u16();

    // Only audit security-relevant events to keep log volume low
    let is_write = matches!(method, Method::POST | Method::DELETE | Method::PUT | Method::PATCH);
    let is_error = status >= 400;
    if is_write || is_error {
        let event = if status == 401 { "auth_failure" }
            else if status == 403 { "access_denied" }
            else if status == 429 { "rate_limited" }
            else if status == 413 { "payload_too_large" }
            else if is_write { "write_operation" }
            else { "error" };
        audit_log(event, &ip, &path, status, Some(&request_id));
    }

    response
}

/// Create the Axum HTTP router with REST, WebSocket Change Streams & Cluster RPC
pub fn create_router(state: Arc<AppState>) -> Router {
    // Public routes — no auth required
    let public_routes = Router::new()
        .route("/v1/health", get(health_check))
        .route("/v1/info", get(server_info))
        .route("/v1/auth/login", post(auth_login));

    // Read-only routes — requires any valid JWT (Admin, ReadWrite, or ReadOnly)
    let read_routes = Router::new()
        .route("/v1/collections/{name}/documents", get(get_collection_documents))
        .route("/v1/collections/{name}/stats", get(collection_stats))
        .route("/v1/collections/{name}/search", post(search_collection))
        .route("/v1/collections/{name}/ttl/stats", get(collection_ttl_stats))
        .route("/v1/subscribe", get(ws_global_subscribe))
        .route("/v1/collections/{name}/watch", get(ws_collection_watch))
        .route("/v1/backup/list", get(backup_list))
        .route("/v1/auth/whoami", get(auth_whoami))
        .layer(middleware::from_fn_with_state(state.clone(), client_auth_middleware));

    // Write routes — requires Admin or ReadWrite role
    let write_routes = Router::new()
        .route("/v1/query", post(execute_query))
        .route("/v1/collections/{name}/documents", post(insert_document))
        .route("/v1/collections/{name}/documents/{id}", delete(delete_document))
        .route("/v1/collections/{name}/insert", post(insert_document))
        .route("/v1/collections/{name}/aggregate", post(aggregate_collection))
        .route("/v1/collections/{name}/ttl/purge", post(collection_ttl_purge))
        .route("/v1/backup/create", post(backup_create))
        .route("/v1/backup/restore", post(backup_restore))
        .layer(middleware::from_fn(rbac_write_middleware))
        .layer(middleware::from_fn_with_state(state.clone(), client_auth_middleware));

    let cluster_routes = Router::new()
        .route("/v1/cluster/status", get(cluster_status))
        .route("/v1/cluster/join", post(cluster_join))
        .route("/v1/cluster/failover", post(cluster_trigger_failover))
        .route("/v1/cluster/raft/vote", post(raft_request_vote))
        .route("/v1/cluster/raft/append", post(raft_append_entries))
        .layer(middleware::from_fn(cluster_auth_middleware));

    Router::new()
        .merge(public_routes)
        .merge(read_routes)
        .merge(write_routes)
        .merge(cluster_routes)
        // Global middleware (outermost = first to execute on every request)
        .layer(
            ServiceBuilder::new()
                // 1. Connection timeout — kills idle/slow clients after 30s (Slowloris protection)
                .layer(TimeoutLayer::new(Duration::from_secs(
                    std::env::var("FAIZDB_REQUEST_TIMEOUT_SECS")
                        .ok().and_then(|v| v.parse().ok()).unwrap_or(30)
                )))
                // 2. Request ID — inject trace ID
                .layer(middleware::from_fn(request_id_middleware))
                // 3. Audit Log — capture security events
                .layer(middleware::from_fn(audit_middleware))
                // 4. Rate Limit + Blocklist
                .layer(middleware::from_fn(rate_limit_middleware))
                // 5. Payload Size Limiter
                .layer(middleware::from_fn(payload_size_middleware))
                // 6. CORS
                .layer(middleware::from_fn(cors_middleware))
        )
        .with_state(state)
}

/// POST /v1/auth/login — exchange credentials for a short-lived JWT
async fn auth_login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    // In-memory user store seeded from environment variables.
    // In production, this should be backed by a persistent users collection.
    let admin_user = std::env::var("FAIZDB_ADMIN_USER").unwrap_or_else(|_| "admin".to_string());
    let admin_pass = std::env::var("FAIZDB_ADMIN_PASS").unwrap_or_else(|_| "faizdb-admin-2026".to_string());
    let rw_user = std::env::var("FAIZDB_RW_USER").unwrap_or_default();
    let rw_pass = std::env::var("FAIZDB_RW_PASS").unwrap_or_default();
    let ro_user = std::env::var("FAIZDB_RO_USER").unwrap_or_default();
    let ro_pass = std::env::var("FAIZDB_RO_PASS").unwrap_or_default();

    let role = if payload.username == admin_user && payload.password == admin_pass {
        Some(Role::Admin)
    } else if !rw_user.is_empty() && payload.username == rw_user && payload.password == rw_pass {
        Some(Role::ReadWrite)
    } else if !ro_user.is_empty() && payload.username == ro_user && payload.password == ro_pass {
        Some(Role::ReadOnly)
    } else {
        None
    };

    match role {
        Some(r) => {
            let expires_in: u64 = std::env::var("FAIZDB_TOKEN_TTL_SECS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(3600);
            match state.auth.generate_token(&payload.username, r, expires_in) {
                Ok(token) => {
                    info!("[Auth] Login success: {} ({:?})", payload.username, r);
                    (
                        StatusCode::OK,
                        Json(ApiResponse::ok(LoginResponse {
                            token,
                            username: payload.username,
                            role: format!("{:?}", r),
                            expires_in,
                        }))
                    ).into_response()
                }
                Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::err(e))).into_response(),
            }
        }
        None => {
            warn!("[Auth] Login failed for user: {}", payload.username);
            (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::err("Invalid username or password"))).into_response()
        }
    }
}

/// GET /v1/auth/whoami — returns the currently authenticated user info
async fn auth_whoami(req: axum::extract::Request) -> impl IntoResponse {
    match req.extensions().get::<AuthenticatedUser>() {
        Some(user) => Json(ApiResponse::ok(serde_json::json!({
            "username": user.username,
            "role": format!("{:?}", user.role),
        }))).into_response(),
        None => (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::err("Not authenticated"))).into_response(),
    }
}

async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "healthy",
        "engine": "FaizDB",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

async fn server_info() -> impl IntoResponse {
    Json(serde_json::json!({
        "name": "FaizDB Server",
        "version": env!("CARGO_PKG_VERSION"),
        "creator": "Ahmad Faiz",
        "features": ["document", "vector", "graph", "acid", "faizql", "change_streams", "websockets", "raft_clustering", "auto_sharding"]
    }))
}

async fn execute_query(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<QueryRequest>,
) -> impl IntoResponse {
    match parse_query(&payload.query) {
        Ok(stmt) => match state.db.execute(stmt) {
            Ok(result) => (StatusCode::OK, Json(ApiResponse::ok(result))),
            Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(e))),
        },
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(format!("Parse error: {e}")))),
    }
}

async fn insert_document(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(doc_val): Json<serde_json::Value>,
) -> impl IntoResponse {
    let doc = match Document::from_json_value(doc_val) {
        Some(d) => d,
        None => return (StatusCode::BAD_REQUEST, Json(ApiResponse::err("Expected JSON object"))),
    };

    let col = state.db.get_or_create_collection(&name);
    let doc_clone = doc.clone();
    match col.insert(doc) {
        Ok(id) => {
            state.db.change_stream_bus().publish(ChangeEvent::insert(&name, doc_clone));
            (
                StatusCode::CREATED,
                Json(ApiResponse::ok(serde_json::json!({ "id": id.as_str() }))),
            )
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e.to_string()))),
    }
}

async fn get_collection_documents(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let docs = col.find_all(None);
    let output: Vec<serde_json::Value> = docs
        .into_iter()
        .map(|d| {
            let mut val = serde_json::to_value(&d.fields).unwrap_or(serde_json::Value::Null);
            if let Some(obj) = val.as_object_mut() {
                obj.insert("_id".to_string(), serde_json::Value::String(d.id.as_str().to_string()));
            }
            val
        })
        .collect();
    Json(ApiResponse::ok(output))
}

async fn delete_document(
    State(state): State<Arc<AppState>>,
    Path((name, id)): Path<(String, String)>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    match col.delete_by_id(&id) {
        Ok(_) => {
            state.db.change_stream_bus().publish(ChangeEvent::delete(&name, &id));
            (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({ "deleted": true, "id": id }))))
        }
        Err(e) => (StatusCode::NOT_FOUND, Json(ApiResponse::err(e.to_string()))),
    }
}

async fn collection_stats(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let stats = col.stats();
    Json(ApiResponse::ok(serde_json::json!({
        "name": col.name(),
        "document_count": stats.document_count,
        "total_size": stats.total_size,
        "avg_document_size": stats.avg_document_size,
        "index_count": stats.index_count
    })))
}

#[derive(Debug, Deserialize)]
pub struct AggregateRequest {
    pub pipeline: serde_json::Value,
}

async fn aggregate_collection(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<AggregateRequest>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let all_docs = col.find_all(None);

    match faizdb_query::parse_pipeline(&payload.pipeline) {
        Ok(stages) => {
            let results = faizdb_query::execute_pipeline(all_docs, &stages);
            (StatusCode::OK, Json(ApiResponse::ok(results)))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(format!("Aggregation error: {e}")))),
    }
}

#[derive(Debug, Deserialize)]
pub struct FullTextSearchRequest {
    pub query: String,
    #[serde(default)]
    pub fuzzy: bool,
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

fn default_top_k() -> usize {
    10
}

async fn search_collection(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(payload): Json<FullTextSearchRequest>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let results = col.search_text(&payload.query, payload.fuzzy, payload.top_k);

    let output: Vec<serde_json::Value> = results
        .into_iter()
        .map(|(doc, score, matched_terms)| {
            let mut val = serde_json::to_value(&doc.fields).unwrap_or(serde_json::Value::Null);
            if let Some(obj) = val.as_object_mut() {
                obj.insert("_id".to_string(), serde_json::Value::String(doc.id.as_str().to_string()));
                obj.insert("_score".to_string(), serde_json::json!(score));
                obj.insert("_matched_terms".to_string(), serde_json::json!(matched_terms));
            }
            val
        })
        .collect();

    Json(ApiResponse::ok(output))
}

async fn collection_ttl_stats(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let stats = col.ttl_stats();
    Json(ApiResponse::ok(stats))
}

async fn collection_ttl_purge(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let col = state.db.get_or_create_collection(&name);
    let purged_ids = col.purge_expired();
    Json(ApiResponse::ok(serde_json::json!({
        "purged_count": purged_ids.len(),
        "purged_ids": purged_ids,
    })))
}

#[derive(Debug, Deserialize)]
pub struct RestoreBackupRequest {
    pub filename: Option<String>,
}

/// Create a new atomic consistent snapshot
async fn backup_create(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let collections = state.db.all_collections();
    let mut data = Vec::new();
    for (name, col) in collections {
        let docs = col.find_all(None);
        data.push((name, docs));
    }

    let archive = faizdb_core::backup::build_snapshot(&data);
    let filename = format!("faizdb_snapshot_{}.json", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let path = std::path::PathBuf::from("./backups").join(&filename);

    match faizdb_core::backup::save_snapshot_file(&archive, &path) {
        Ok(_) => (StatusCode::CREATED, Json(ApiResponse::ok(archive.manifest))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e))),
    }
}

/// List all available snapshot files
async fn backup_list() -> impl IntoResponse {
    let backup_dir = std::path::Path::new("./backups");
    if !backup_dir.exists() {
        return Json(ApiResponse::ok(Vec::<faizdb_core::backup::SnapshotManifest>::new()));
    }

    let mut manifests = Vec::new();
    if let Ok(entries) = std::fs::read_dir(backup_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Ok(archive) = faizdb_core::backup::load_and_verify_snapshot(&path) {
                    manifests.push(archive.manifest);
                }
            }
        }
    }

    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(ApiResponse::ok(manifests))
}

/// Restore database from snapshot
async fn backup_restore(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RestoreBackupRequest>,
) -> impl IntoResponse {
    let backup_dir = std::path::Path::new("./backups");
    let target_file = match payload.filename {
        Some(name) => backup_dir.join(name),
        None => {
            // Find latest
            let mut latest: Option<(std::path::PathBuf, String)> = None;
            if let Ok(entries) = std::fs::read_dir(backup_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        let name = path.to_string_lossy().to_string();
                        if latest.as_ref().map_or(true, |l| name > l.1) {
                            latest = Some((path, name));
                        }
                    }
                }
            }
            match latest {
                Some((p, _)) => p,
                None => return (StatusCode::NOT_FOUND, Json(ApiResponse::err("No backup snapshots found to restore"))),
            }
        }
    };

    match faizdb_core::backup::load_and_verify_snapshot(&target_file) {
        Ok(archive) => {
            let mut restored_count = 0;
            for (col_name, doc_vals) in archive.collections_data {
                let col = state.db.get_or_create_collection(&col_name);
                for val in doc_vals {
                    if let Some(doc) = faizdb_core::document::model::Document::from_json_value(val) {
                        if col.insert(doc).is_ok() {
                            restored_count += 1;
                        }
                    }
                }
            }
            (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                "message": "Database snapshot successfully verified and restored",
                "checksum": archive.manifest.checksum,
                "restored_documents": restored_count,
            }))))
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::err(format!("Restore verification failed: {e}")))),
    }
}

/// Cluster Status Handler: `/v1/cluster/status`
async fn cluster_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let node_info = state.db.raft().get_info();
    let shard_dist = state.db.shards().get_distribution();
    Json(ApiResponse::ok(serde_json::json!({
        "node": node_info,
        "shards": shard_dist,
        "consensus": "Raft v1.0",
        "virtual_slots": 16384,
    })))
}

/// Dynamic Cluster Join Handler: `/v1/cluster/join`
async fn cluster_join(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<JoinClusterRequest>,
) -> impl IntoResponse {
    state.db.raft().add_peer(payload.peer_id.clone(), payload.peer_address.clone());
    state.db.shards().register_node(payload.peer_id.clone(), payload.peer_address.clone());
    Json(ApiResponse::ok(serde_json::json!({
        "message": format!("Peer '{}' joined cluster successfully", payload.peer_id),
        "peer_id": payload.peer_id,
        "peer_address": payload.peer_address,
    })))
}

/// Simulate Failover Handler: `/v1/cluster/failover`
async fn cluster_trigger_failover(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    state.db.raft().trigger_election();
    let info = state.db.raft().get_info();
    Json(ApiResponse::ok(serde_json::json!({
        "message": "Election timeout triggered. Node promoted to new Leader",
        "new_term": info.term,
        "is_leader": info.is_leader,
    })))
}

/// Raft RequestVote RPC Handler: `/v1/cluster/raft/vote`
async fn raft_request_vote(
    State(state): State<Arc<AppState>>,
    Json(args): Json<RequestVoteArgs>,
) -> impl IntoResponse {
    let reply = state.db.raft().handle_request_vote(args);
    Json(reply)
}

/// Raft AppendEntries RPC Handler: `/v1/cluster/raft/append`
async fn raft_append_entries(
    State(state): State<Arc<AppState>>,
    Json(args): Json<AppendEntriesArgs>,
) -> impl IntoResponse {
    let reply = state.db.raft().handle_append_entries(args);
    Json(reply)
}

/// WebSocket Change Stream: `/v1/subscribe`
async fn ws_global_subscribe(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = state.db.clone();
    ws.on_upgrade(move |socket| handle_change_stream_socket(socket, db, None))
}

/// WebSocket Collection Watch: `/v1/collections/{name}/watch`
async fn ws_collection_watch(
    ws: WebSocketUpgrade,
    Path(collection_name): Path<String>,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let db = state.db.clone();
    ws.on_upgrade(move |socket| handle_change_stream_socket(socket, db, Some(collection_name)))
}

async fn handle_change_stream_socket(
    socket: WebSocket,
    db: Arc<DatabaseContext>,
    target_collection: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = db.change_stream_bus().subscribe();
    let col_filter = target_collection.unwrap_or_else(|| "*".to_string());

    info!(
        "WebSocket client connected to Change Stream for '{}' (Total subscribers: {})",
        col_filter,
        db.change_stream_bus().subscriber_count()
    );

    let welcome = serde_json::json!({
        "status": "connected",
        "stream": "faizdb-change-streams-v1",
        "collection": col_filter,
        "active_subscribers": db.change_stream_bus().subscriber_count(),
        "timestamp": chrono::Utc::now()
    });
    if let Ok(msg_str) = serde_json::to_string(&welcome) {
        let _ = sender.send(Message::Text(msg_str.into())).await;
    }

    let filter_for_task = col_filter.clone();
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if filter_for_task == "*" || filter_for_task == event.collection {
                if let Ok(json) = serde_json::to_string(&event) {
                    if sender.send(Message::Text(json.into())).await.is_err() {
                        break;
                    }
                }
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Close(_) = msg {
                break;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };

    debug!("WebSocket client disconnected from Change Stream");
}
