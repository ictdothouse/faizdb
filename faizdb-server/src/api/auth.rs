//! Authentication handlers: login, whoami, token generation.
//!
//! ## Security: Auth-Specific Rate Limiting
//!
//! The login endpoint has a **strict per-IP rate limit** (5 attempts / 60 seconds)
//! independent of the global API rate limiter. This prevents brute-force password
//! attacks even when Argon2 slows down individual hash computations.

use std::sync::Arc;
use std::time::Instant;

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use faizdb_security::auth::Role;

use super::{ApiResponse, AppState, AuthenticatedUser};

// ── Auth-Specific Rate Limiter ───────────────────────────────────────────────
// Strict per-IP limit for authentication endpoints to prevent brute-force attacks.
// Separate from the global rate limiter (which allows 100 req/10s — too generous for auth).

/// Per-IP auth attempt tracker: maps IP → (attempt_count, window_start)
static AUTH_RATE_LIMITER: std::sync::OnceLock<DashMap<String, (u32, Instant)>> =
    std::sync::OnceLock::new();

fn get_auth_rate_limiter() -> &'static DashMap<String, (u32, Instant)> {
    AUTH_RATE_LIMITER.get_or_init(DashMap::new)
}

/// Check auth rate limit: 5 login attempts per 60 seconds per IP.
/// Returns `Err(StatusCode::TOO_MANY_REQUESTS)` if the limit is exceeded.
fn check_auth_rate_limit(client_ip: &str) -> Result<(), StatusCode> {
    let limiter = get_auth_rate_limiter();
    let now = Instant::now();

    let max_attempts: u32 = std::env::var("FAIZDB_AUTH_RATE_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let window_secs: u64 = std::env::var("FAIZDB_AUTH_RATE_WINDOW_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    let window = std::time::Duration::from_secs(window_secs);

    let mut entry = limiter.entry(client_ip.to_string()).or_insert((0, now));
    if now.duration_since(entry.1) > window {
        // Window expired — reset counter
        entry.0 = 1;
        entry.1 = now;
        Ok(())
    } else {
        entry.0 += 1;
        if entry.0 > max_attempts {
            warn!(
                "[Auth RateLimit] IP {} exceeded auth rate limit ({}/{} attempts in {}s window)",
                client_ip, entry.0, max_attempts, window_secs
            );
            Err(StatusCode::TOO_MANY_REQUESTS)
        } else {
            Ok(())
        }
    }
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
pub struct GenerateTokenRequest {
    pub username: String,
    pub role: String,
    pub valid_seconds: Option<u64>,
}

/// POST /v1/auth/login — exchange credentials for a short-lived JWT.
///
/// Rate limited to 5 attempts per 60 seconds per IP to prevent brute-force attacks.
pub async fn auth_login(
    State(state): State<Arc<AppState>>,
    req: axum::extract::Request,
) -> impl IntoResponse {
    // Extract client IP for rate limiting
    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| {
            req.extensions()
                .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

    // Enforce auth-specific rate limit BEFORE attempting authentication
    if let Err(status) = check_auth_rate_limit(&client_ip) {
        return (
            status,
            Json(ApiResponse::<()>::err(format!(
                "Too many login attempts. Try again in 60 seconds. (IP: {})",
                client_ip
            ))),
        )
            .into_response();
    }

    // Parse the JSON body manually since we consumed req for IP extraction
    let body = axum::body::to_bytes(req.into_body(), 1024 * 16)
        .await
        .unwrap_or_default();
    let payload: LoginRequest = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::err(format!("Invalid request body: {e}"))),
            )
                .into_response()
        }
    };

    let role = state
        .user_store
        .authenticate(&payload.username, &payload.password);


    match role {
        Some(r) => {
            let expires_in: u64 = std::env::var("FAIZDB_TOKEN_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3600);
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
                        })),
                    )
                        .into_response()
                }
                Err(e) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiResponse::<()>::err(e)),
                )
                    .into_response(),
            }
        }
        None => {
            warn!("[Auth] Login failed for user: {}", payload.username);
            (
                StatusCode::UNAUTHORIZED,
                Json(ApiResponse::<()>::err("Invalid username or password")),
            )
                .into_response()
        }
    }
}

/// GET /v1/auth/whoami — returns the currently authenticated user's info
pub async fn auth_whoami(req: axum::extract::Request) -> impl IntoResponse {
    match req.extensions().get::<AuthenticatedUser>() {
        Some(user) => Json(ApiResponse::ok(serde_json::json!({
            "username": user.username,
            "role": format!("{:?}", user.role),
        })))
        .into_response(),
        None => (
            StatusCode::UNAUTHORIZED,
            Json(ApiResponse::<()>::err("Not authenticated")),
        )
            .into_response(),
    }
}

/// POST /v1/auth/token — admin-only token generator for service accounts
pub async fn generate_token_handler(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<GenerateTokenRequest>,
) -> impl IntoResponse {
    let role = match payload.role.to_lowercase().as_str() {
        "admin" => Role::Admin,
        "readwrite" | "read_write" => Role::ReadWrite,
        "readonly" | "read_only" => Role::ReadOnly,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::err(
                    "Invalid role. Use Admin, ReadWrite, or ReadOnly",
                )),
            )
                .into_response()
        }
    };
    let valid_seconds = payload.valid_seconds.unwrap_or(86400 * 30);
    match state
        .auth
        .generate_token(&payload.username, role, valid_seconds)
    {
        Ok(token) => (
            StatusCode::OK,
            Json(ApiResponse::ok(serde_json::json!({
                "token": token,
                "username": payload.username,
                "role": format!("{:?}", role),
                "valid_seconds": valid_seconds,
            }))),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiResponse::<()>::err(e)),
        )
            .into_response(),
    }
}

// ── User Management (Admin Only) ─────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
    pub role: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdatePasswordRequest {
    pub password: String,
}

/// POST /v1/users — create a new user (Admin only)
pub async fn create_user(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<CreateUserRequest>,
) -> impl IntoResponse {
    let role = match payload.role.to_lowercase().as_str() {
        "admin" => Role::Admin,
        "readwrite" | "read_write" => Role::ReadWrite,
        "readonly" | "read_only" => Role::ReadOnly,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<()>::err(
                    "Invalid role. Use Admin, ReadWrite, or ReadOnly",
                )),
            )
                .into_response()
        }
    };

    match state
        .user_store
        .create_user(&payload.username, &payload.password, role)
    {
        Ok(_) => (
            StatusCode::CREATED,
            Json(ApiResponse::ok(serde_json::json!({
                "username": payload.username,
                "role": format!("{:?}", role),
                "created": true
            }))),
        )
            .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::err(e))).into_response(),
    }
}

/// GET /v1/users — list all users (Admin only)
pub async fn list_users(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let users = state.user_store.list_users();
    Json(ApiResponse::ok(users)).into_response()
}

/// DELETE /v1/users/{username} — delete a user (Admin only)
pub async fn delete_user(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(username): axum::extract::Path<String>,
) -> impl IntoResponse {
    match state.user_store.delete_user(&username) {
        Ok(deleted) => {
            if deleted {
                (
                    StatusCode::OK,
                    Json(ApiResponse::ok(
                        serde_json::json!({ "deleted": true, "username": username }),
                    )),
                )
                    .into_response()
            } else {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiResponse::<()>::err(format!(
                        "User '{username}' not found"
                    ))),
                )
                    .into_response()
            }
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::err(e))).into_response(),
    }
}

/// PUT /v1/users/{username}/password — update a user's password (Admin only)
pub async fn update_user_password(
    State(state): State<Arc<AppState>>,
    axum::extract::Path(username): axum::extract::Path<String>,
    Json(payload): Json<UpdatePasswordRequest>,
) -> impl IntoResponse {
    match state
        .user_store
        .update_password(&username, &payload.password)
    {
        Ok(_) => (
            StatusCode::OK,
            Json(ApiResponse::ok(
                serde_json::json!({ "updated": true, "username": username }),
            )),
        )
            .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::err(e))).into_response(),
    }
}
