//! Backup handlers: create, list, restore, schedule.

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;

use super::{ApiResponse, AppState, BackupScheduleConfig};

#[derive(Debug, Deserialize)]
pub struct CreateBackupRequest {
    pub passphrase: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RestoreBackupRequest {
    pub filename: Option<String>,
    pub passphrase: Option<String>,
}

/// POST /v1/backup/create — create an atomic consistent snapshot (optionally AES-256-GCM encrypted)
pub async fn backup_create(
    State(state): State<Arc<AppState>>,
    body: Option<Json<CreateBackupRequest>>,
) -> impl IntoResponse {
    let collections = state.db.all_collections();
    let mut data = Vec::new();
    for (name, col) in collections {
        data.push((name, col.find_all(None)));
    }
    let archive = faizdb_core::backup::build_snapshot(&data);
    let passphrase = body.and_then(|b| b.0.passphrase).filter(|p| !p.trim().is_empty());

    let filename = if passphrase.is_some() {
        format!("faizdb_snapshot_{}.enc.json", chrono::Utc::now().format("%Y%m%d_%H%M%S"))
    } else {
        format!("faizdb_snapshot_{}.json", chrono::Utc::now().format("%Y%m%d_%H%M%S"))
    };
    let path = std::path::PathBuf::from("./backups").join(&filename);

    if let Some(pass) = passphrase {
        let cipher = faizdb_security::Cipher::from_passphrase(pass.as_str());
        let raw_json = match serde_json::to_vec_pretty(&archive) {
            Ok(b) => b,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e.to_string()))),
        };
        let encrypted = match cipher.encrypt(&raw_json) {
            Ok(enc) => enc,
            Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e))),
        };
        if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
        let enc_wrapper = serde_json::json!({
            "encrypted": true, "cipher": "AES-256-GCM",
            "manifest": archive.manifest, "ciphertext": encrypted.ciphertext,
        });
        match std::fs::write(&path, serde_json::to_string_pretty(&enc_wrapper).unwrap_or_default()) {
            Ok(_) => (StatusCode::CREATED, Json(ApiResponse::ok(archive.manifest))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e.to_string()))),
        }
    } else {
        match faizdb_core::backup::save_snapshot_file(&archive, &path) {
            Ok(_) => (StatusCode::CREATED, Json(ApiResponse::ok(archive.manifest))),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiResponse::err(e))),
        }
    }
}

/// GET /v1/backup/list — list all available snapshot files
pub async fn backup_list() -> impl IntoResponse {
    let backup_dir = std::path::Path::new("./backups");
    if !backup_dir.exists() {
        return Json(ApiResponse::ok(Vec::<faizdb_core::backup::SnapshotManifest>::new()));
    }
    let mut manifests = Vec::new();
    if let Ok(entries) = std::fs::read_dir(backup_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let ext = path.extension().and_then(|s| s.to_str());
            if ext == Some("json") {
                if let Ok(raw) = std::fs::read_to_string(&path) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                        if val.get("encrypted").and_then(|v| v.as_bool()) == Some(true) {
                            if let Some(m_val) = val.get("manifest") {
                                if let Ok(mut m) = serde_json::from_value::<faizdb_core::backup::SnapshotManifest>(m_val.clone()) {
                                    m.checksum = format!("🔒 [Encrypted] {}", m.checksum);
                                    manifests.push(m);
                                    continue;
                                }
                            }
                        }
                    }
                }
                if let Ok(archive) = faizdb_core::backup::load_and_verify_snapshot(&path) {
                    manifests.push(archive.manifest);
                }
            }
        }
    }
    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Json(ApiResponse::ok(manifests))
}

/// POST /v1/backup/restore — restore from snapshot (with optional AES-256-GCM decryption)
pub async fn backup_restore(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RestoreBackupRequest>,
) -> impl IntoResponse {
    let backup_dir = std::path::Path::new("./backups");
    let target_file = match payload.filename {
        Some(name) => backup_dir.join(name),
        None => {
            let mut latest: Option<(std::path::PathBuf, String)> = None;
            if let Ok(entries) = std::fs::read_dir(backup_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("json") {
                        let name = path.to_string_lossy().to_string();
                        if latest.as_ref().is_none_or(|l| name > l.1) {
                            latest = Some((path, name));
                        }
                    }
                }
            }
            match latest {
                Some((p, _)) => p,
                None => return (StatusCode::NOT_FOUND, Json(ApiResponse::<()>::err("No backup snapshots found"))).into_response(),
            }
        }
    };

    let raw_content = match std::fs::read_to_string(&target_file) {
        Ok(c) => c,
        Err(e) => return (StatusCode::NOT_FOUND, Json(ApiResponse::<()>::err(e.to_string()))).into_response(),
    };

    let archive_result: Result<faizdb_core::backup::SnapshotArchive, String> =
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw_content) {
            if val.get("encrypted").and_then(|v| v.as_bool()) == Some(true) {
                let pass = match payload.passphrase.filter(|p| !p.trim().is_empty()) {
                    Some(p) => p,
                    None => return (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::err("Snapshot is encrypted. Passphrase required."))).into_response(),
                };
                let cipher = faizdb_security::Cipher::from_passphrase(pass.as_str());
                let ciphertext: Vec<u8> = match val.get("ciphertext").and_then(|c| serde_json::from_value(c.clone()).ok()) {
                    Some(ct) => ct,
                    None => return (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::err("Corrupted encrypted snapshot"))).into_response(),
                };
                match cipher.decrypt(&faizdb_security::EncryptedData { ciphertext }) {
                    Ok(plain) => serde_json::from_slice(&plain).map_err(|e| format!("Corrupted snapshot data: {e}")),
                    Err(e) => return (StatusCode::UNAUTHORIZED, Json(ApiResponse::<()>::err(format!("Invalid passphrase: {e}")))).into_response(),
                }
            } else {
                faizdb_core::backup::load_and_verify_snapshot(&target_file)
            }
        } else {
            faizdb_core::backup::load_and_verify_snapshot(&target_file)
        };

    match archive_result {
        Ok(archive) => {
            let mut restored_count = 0;
            for (col_name, doc_vals) in archive.collections_data {
                let col = state.db.get_or_create_collection(&col_name);
                for val in doc_vals {
                    if let Some(doc) = faizdb_core::document::model::Document::from_json_value(val) {
                        let _ = col.insert(doc);
                        restored_count += 1;
                    }
                }
            }
            (StatusCode::OK, Json(ApiResponse::ok(serde_json::json!({
                "restored": true,
                "checksum": archive.manifest.checksum,
                "documents_restored": restored_count,
                "created_at": archive.manifest.created_at,
            })))).into_response()
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(ApiResponse::<()>::err(format!("Restore failed: {e}")))).into_response(),
    }
}

/// GET /v1/backup/schedule — get current backup schedule configuration
pub async fn get_backup_schedule(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let config = state.backup_schedule.read().clone();
    Json(ApiResponse::ok(config))
}

/// POST /v1/backup/schedule — update backup schedule configuration
pub async fn update_backup_schedule(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<BackupScheduleConfig>,
) -> impl IntoResponse {
    let mut config = state.backup_schedule.write();
    *config = payload.clone();
    Json(ApiResponse::ok(payload))
}
