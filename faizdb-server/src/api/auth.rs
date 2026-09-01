//! Authentication handlers: login, whoami, token generation.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use faizdb_security::auth::Role;

use super::{ApiResponse, AppState, AuthenticatedUser};

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

/// POST /v1/auth/login — exchange credentials for a short-lived JWT
pub async fn auth_login(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
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
                    (StatusCode::OK, Json(ApiResponse::ok(LoginResponse {
                        token, username: payload.username, role: format!("{:?}", r), expires_in,
                    }))).into_response()
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

/// GET /v1/auth/whoami — returns the currently authenticated user's info
pub async fn auth_whoami(req: axum::extract::Request) -> impl IntoResponse {
    match req.extensions().get::<AuthenticatedUser>() {
        Some(user) => Json(ApiResponse::ok(serde_json::json!({
            "username": user.username,
            "role": format!("{:?}", user.role),
        }))).into_response(),
        None => (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::err("Not authenticated"))).into_response(),
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
        _ => return (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::err("Invalid role. Use Admin, ReadWrite, or ReadOnly"))).into_response(),
    };
    let valid_seconds = payload.valid_seconds.unwrap_or(86400 * 30);
    match state.auth.generate_token(&payload.username, role, valid_seconds) {
        Ok(token) => (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
            "token": token,
            "username": payload.username,
            "role": format!("{:?}", role),
            "valid_seconds": valid_seconds,
        })))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::<()>::err(e))).into_response(),
    }
}
