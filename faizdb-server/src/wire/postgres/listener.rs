//! PostgreSQL Wire Protocol TCP Server (Port 5432).
//!
//! Accepts client TCP connections from PostgreSQL clients (psql, DBeaver, TablePlus,
//! Node.js `pg`, Prisma, GORM, Python asyncpg, etc.), handles the SSL and Startup handshake,
//! parses queries, and routes them to the FaizDB query execution engine.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

use super::codec::{
    encode_auth_cleartext_password, encode_auth_ok, encode_backend_key_data, encode_bind_complete,
    encode_close_complete, encode_error_response, encode_no_data, encode_parameter_description,
    encode_parameter_status, encode_parse_complete, encode_ready_for_query,
};
use super::handler::{handle_postgres_execute_query, handle_postgres_query};
use faizdb_query::DatabaseContext;
use faizdb_security::UserStore;

/// SSLRequest code sent by PostgreSQL clients (80877103 in decimal)
const PG_SSL_REQUEST_CODE: i32 = 80877103;

/// PostgreSQL Protocol v3.0 code (196608 in decimal / 0x00030000)
const PG_PROTOCOL_V3: i32 = 196608;

/// Run the PostgreSQL Wire Protocol server with optional graceful shutdown future
pub async fn run_postgres_server_with_shutdown<F>(
    addr: &str,
    db: Arc<DatabaseContext>,
    user_store: Arc<UserStore>,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind(addr).await?;
    info!("🐘 PostgreSQL Wire Protocol Server running on postgresql://{addr}");

    let max_conns = std::env::var("FAIZDB_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_conns));
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("🐘 PostgreSQL Wire Protocol Server received shutdown signal — draining listener...");
                break;
            }
            accept_res = listener.accept() => {
                match accept_res {
                    Ok((socket, peer_addr)) => {
                        let permit = match semaphore.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                warn!("Rejecting Postgres connection from {peer_addr}: max connections ({max_conns}) reached");
                                tokio::spawn(async move {
                                    let mut s = socket;
                                    let err =
                                        encode_error_response("FATAL", "53300", "too many clients already");
                                    let _ = s.write_all(&err).await;
                                    let _ = s.flush().await;
                                });
                                continue;
                            }
                        };
                        let db_clone = db.clone();
                        let store_clone = user_store.clone();
                        tokio::spawn(async move {
                            let _permit = permit;
                            if let Err(e) = handle_postgres_connection(
                                socket,
                                db_clone,
                                store_clone,
                                peer_addr.to_string(),
                            )
                            .await
                            {
                                warn!("Postgres connection closed for {peer_addr}: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept Postgres connection: {e}");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Run the PostgreSQL Wire Protocol server on the given address (e.g. "0.0.0.0:5432")
pub async fn run_postgres_server(
    addr: &str,
    db: Arc<DatabaseContext>,
    user_store: Arc<UserStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    run_postgres_server_with_shutdown(addr, db, user_store, std::future::pending()).await
}

async fn handle_postgres_connection(
    mut stream: TcpStream,
    db: Arc<DatabaseContext>,
    user_store: Arc<UserStore>,
    client_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut in_transaction = false;

    // 1. Initial Handshake (SSLRequest check & StartupMessage)
    loop {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            return Ok(()); // Connection closed
        }

        let msg_len = i32::from_be_bytes(len_buf);
        if !(4..=1_048_576).contains(&msg_len) {
            return Err("Invalid or oversized Postgres startup message length".into());
        }

        let mut body_buf = vec![0u8; (msg_len - 4) as usize];
        stream.read_exact(&mut body_buf).await?;

        if body_buf.len() < 4 {
            return Err("Incomplete Startup header".into());
        }

        let code = i32::from_be_bytes([body_buf[0], body_buf[1], body_buf[2], body_buf[3]]);

        if code == PG_SSL_REQUEST_CODE {
            // Client is asking for SSL: reply with 'N' (SSL not supported, continue plain)
            stream.write_all(b"N").await?;
            stream.flush().await?;
            continue;
        }

        if code == PG_PROTOCOL_V3 {
            // Parse startup parameters (null-terminated key-value pairs)
            let mut params = HashMap::new();
            let mut start = 4;
            while start < body_buf.len() {
                if body_buf[start] == 0 {
                    break;
                }
                if let Some(null_pos) = body_buf[start..].iter().position(|&b| b == 0) {
                    let key =
                        String::from_utf8_lossy(&body_buf[start..start + null_pos]).to_string();
                    start += null_pos + 1;
                    if let Some(val_null_pos) = body_buf[start..].iter().position(|&b| b == 0) {
                        let val = String::from_utf8_lossy(&body_buf[start..start + val_null_pos])
                            .to_string();
                        start += val_null_pos + 1;
                        params.insert(key, val);
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }

            let username = params.get("user").map(|s| s.as_str()).unwrap_or("admin");
            let no_auth = std::env::var("FAIZDB_NO_AUTH")
                .map(|v| v == "1" || v == "true")
                .unwrap_or(false);

            if !no_auth {
                // Request password from client via AuthenticationCleartextPassword
                stream.write_all(&encode_auth_cleartext_password()).await?;
                stream.flush().await?;

                // Read PasswordMessage ('p')
                let mut p_type = [0u8; 1];
                if let Err(e) = stream.read_exact(&mut p_type).await {
                    return Err(
                        format!("Client closed connection waiting for password: {e}").into(),
                    );
                }

                if p_type[0] != b'p' {
                    let err = encode_error_response(
                        "FATAL",
                        "28000",
                        "Expected password response from client",
                    );
                    stream.write_all(&err).await?;
                    stream.flush().await?;
                    return Err("Client did not send PasswordMessage".into());
                }

                let mut p_len_buf = [0u8; 4];
                stream.read_exact(&mut p_len_buf).await?;
                let p_len = i32::from_be_bytes(p_len_buf);
                if !(4..=4096).contains(&p_len) {
                    return Err("Invalid password message length".into());
                }

                let mut p_body = vec![0u8; (p_len - 4) as usize];
                stream.read_exact(&mut p_body).await?;
                if let Some(&0) = p_body.last() {
                    p_body.pop();
                }
                let password = String::from_utf8_lossy(&p_body).to_string();

                if user_store.authenticate(username, &password).is_none() {
                    warn!("Authentication failed for user '{username}' from {client_addr}");
                    let err = encode_error_response(
                        "FATAL",
                        "28P01",
                        &format!("password authentication failed for user \"{username}\""),
                    );
                    stream.write_all(&err).await?;
                    stream.flush().await?;
                    return Err(
                        format!("Password authentication failed for user '{username}'").into(),
                    );
                }
                info!("🔐 User '{username}' authenticated successfully via PostgreSQL wire from {client_addr}");
            }

            // Send AuthenticationOk
            stream.write_all(&encode_auth_ok()).await?;

            // Send standard ParameterStatus messages
            stream
                .write_all(&encode_parameter_status("server_version", "16.0 (FaizDB)"))
                .await?;
            stream
                .write_all(&encode_parameter_status("client_encoding", "UTF8"))
                .await?;
            stream
                .write_all(&encode_parameter_status("server_encoding", "UTF8"))
                .await?;
            stream
                .write_all(&encode_parameter_status("DateStyle", "ISO, MDY"))
                .await?;
            stream
                .write_all(&encode_parameter_status("integer_datetimes", "on"))
                .await?;
            stream
                .write_all(&encode_parameter_status(
                    "standard_conforming_strings",
                    "on",
                ))
                .await?;
            stream
                .write_all(&encode_parameter_status("TimeZone", "UTC"))
                .await?;

            // Send BackendKeyData (pid 1001, secret 2002)
            stream
                .write_all(&encode_backend_key_data(1001, 2002))
                .await?;

            // Send ReadyForQuery (status 'I' for idle)
            stream.write_all(&encode_ready_for_query(b'I')).await?;
            stream.flush().await?;
            break;
        }

        warn!("Unsupported Postgres protocol version {code} from {client_addr}");
        return Ok(());
    }

    #[derive(Debug, Clone)]
    struct PreparedStatement {
        query: String,
        param_oids: Vec<i32>,
    }

    #[derive(Debug, Clone)]
    struct Portal {
        query: String,
        params: Vec<String>,
    }

    let mut prepared_statements: HashMap<String, PreparedStatement> = HashMap::new();
    let mut portals: HashMap<String, Portal> = HashMap::new();

    // 2. Command processing loop (handling Simple 'Q' and Extended 'P', 'B', 'D', 'E', 'C', 'S' queries)
    loop {
        let mut type_buf = [0u8; 1];
        if stream.read_exact(&mut type_buf).await.is_err() {
            break; // Client disconnected cleanly
        }

        let msg_type = type_buf[0];

        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            break;
        }

        let msg_len = i32::from_be_bytes(len_buf);
        if !(4..=16_777_216).contains(&msg_len) {
            warn!("Rejected invalid or oversized Postgres message length {msg_len} from {client_addr}");
            let err = encode_error_response(
                "FATAL",
                "54000",
                "Message length exceeds maximum allowed limit (16MB)",
            );
            let _ = stream.write_all(&err).await;
            let _ = stream.flush().await;
            break;
        }
        let body_len = (msg_len - 4) as usize;

        let mut body = vec![0u8; body_len];
        if body_len > 0 && stream.read_exact(&mut body).await.is_err() {
            break;
        }

        match msg_type {
            b'Q' => {
                // Simple Query: body contains null-terminated UTF-8 query string
                let query_str = String::from_utf8_lossy(&body)
                    .trim_end_matches('\0')
                    .to_string();

                let response_bytes = handle_postgres_query(&db, &query_str, &mut in_transaction);
                stream.write_all(&response_bytes).await?;
                stream.flush().await?;
            }
            b'P' => {
                // Extended Query - Parse: [stmt_name\0] [query\0] [param_count (i16)] [param_oids (i32 * n)]
                let mut cursor = 0;
                let name_end = body[cursor..].iter().position(|&b| b == 0).unwrap_or(0);
                let stmt_name =
                    String::from_utf8_lossy(&body[cursor..cursor + name_end]).to_string();
                cursor += name_end + 1;

                let query_end = if cursor < body.len() {
                    body[cursor..].iter().position(|&b| b == 0).unwrap_or(0)
                } else {
                    0
                };
                let query_str =
                    String::from_utf8_lossy(&body[cursor..cursor + query_end]).to_string();
                cursor += query_end + 1;

                let mut param_oids = Vec::new();
                if cursor + 2 <= body.len() {
                    let num_params = i16::from_be_bytes([body[cursor], body[cursor + 1]]) as usize;
                    cursor += 2;
                    for _ in 0..num_params {
                        if cursor + 4 <= body.len() {
                            let oid = i32::from_be_bytes([
                                body[cursor],
                                body[cursor + 1],
                                body[cursor + 2],
                                body[cursor + 3],
                            ]);
                            param_oids.push(oid);
                            cursor += 4;
                        }
                    }
                }

                prepared_statements.insert(
                    stmt_name,
                    PreparedStatement {
                        query: query_str,
                        param_oids,
                    },
                );

                stream.write_all(&encode_parse_complete()).await?;
                stream.flush().await?;
            }
            b'B' => {
                // Extended Query - Bind: [portal\0] [stmt\0] [formats...] [params...]
                let mut cursor = 0;
                let portal_end = body[cursor..].iter().position(|&b| b == 0).unwrap_or(0);
                let portal_name =
                    String::from_utf8_lossy(&body[cursor..cursor + portal_end]).to_string();
                cursor += portal_end + 1;

                let stmt_end = if cursor < body.len() {
                    body[cursor..].iter().position(|&b| b == 0).unwrap_or(0)
                } else {
                    0
                };
                let stmt_name =
                    String::from_utf8_lossy(&body[cursor..cursor + stmt_end]).to_string();
                cursor += stmt_end + 1;

                let stmt_query = match prepared_statements.get(&stmt_name) {
                    Some(s) => s.query.clone(),
                    None => {
                        let err = encode_error_response(
                            "ERROR",
                            "26000",
                            &format!("Prepared statement '{stmt_name}' does not exist"),
                        );
                        stream.write_all(&err).await?;
                        stream.flush().await?;
                        continue;
                    }
                };

                // Read format codes
                if cursor + 2 <= body.len() {
                    let num_formats = i16::from_be_bytes([body[cursor], body[cursor + 1]]) as usize;
                    cursor += 2 + (num_formats * 2);
                }

                // Read bound parameter values
                let mut params = Vec::new();
                if cursor + 2 <= body.len() {
                    let num_params = i16::from_be_bytes([body[cursor], body[cursor + 1]]) as usize;
                    cursor += 2;
                    for _ in 0..num_params {
                        if cursor + 4 <= body.len() {
                            let param_len = i32::from_be_bytes([
                                body[cursor],
                                body[cursor + 1],
                                body[cursor + 2],
                                body[cursor + 3],
                            ]);
                            cursor += 4;
                            if param_len == -1 {
                                params.push("NULL".to_string());
                            } else if param_len >= 0 && cursor + (param_len as usize) <= body.len()
                            {
                                let val_bytes = &body[cursor..cursor + (param_len as usize)];
                                params.push(String::from_utf8_lossy(val_bytes).to_string());
                                cursor += param_len as usize;
                            }
                        }
                    }
                }

                portals.insert(
                    portal_name,
                    Portal {
                        query: stmt_query,
                        params,
                    },
                );

                stream.write_all(&encode_bind_complete()).await?;
                stream.flush().await?;
            }
            b'D' => {
                // Extended Query - Describe: 'S' (Statement) or 'P' (Portal) + name\0
                if !body.is_empty() {
                    let desc_type = body[0];
                    let name = String::from_utf8_lossy(&body[1..])
                        .trim_end_matches('\0')
                        .to_string();
                    if desc_type == b'S' {
                        if let Some(stmt) = prepared_statements.get(&name) {
                            stream
                                .write_all(&encode_parameter_description(&stmt.param_oids))
                                .await?;
                        } else {
                            stream.write_all(&encode_parameter_description(&[])).await?;
                        }
                    }
                    stream.write_all(&encode_no_data()).await?;
                    stream.flush().await?;
                }
            }
            b'E' => {
                // Extended Query - Execute: portal_name\0 + max_rows (i32)
                let portal_end = body.iter().position(|&b| b == 0).unwrap_or(0);
                let portal_name = String::from_utf8_lossy(&body[..portal_end]).to_string();

                let portal = portals.get(&portal_name).cloned();
                if let Some(portal) = portal {
                    // Parameter substitution: replace $1, $2 with values
                    let mut resolved_query = portal.query.clone();
                    for (idx, val) in portal.params.iter().enumerate() {
                        let placeholder = format!("${}", idx + 1);
                        let replacement = if val == "NULL" {
                            "NULL".to_string()
                        } else if val.parse::<f64>().is_ok() {
                            val.clone()
                        } else {
                            format!("'{}'", val.replace('\'', "''"))
                        };
                        resolved_query = resolved_query.replace(&placeholder, &replacement);
                    }

                    let response_bytes =
                        handle_postgres_execute_query(&db, &resolved_query, &mut in_transaction);
                    stream.write_all(&response_bytes).await?;
                    stream.flush().await?;
                } else {
                    let err = encode_error_response(
                        "ERROR",
                        "26000",
                        &format!("Portal '{portal_name}' does not exist"),
                    );
                    stream.write_all(&err).await?;
                    stream.flush().await?;
                }
            }
            b'C' => {
                // Extended Query - Close: 'S' or 'P' + name\0
                if !body.is_empty() {
                    let close_type = body[0];
                    let name = String::from_utf8_lossy(&body[1..])
                        .trim_end_matches('\0')
                        .to_string();
                    if close_type == b'S' {
                        prepared_statements.remove(&name);
                    } else if close_type == b'P' {
                        portals.remove(&name);
                    }
                }
                stream.write_all(&encode_close_complete()).await?;
                stream.flush().await?;
            }
            b'X' => {
                // Terminate
                break;
            }
            b'H' => {
                // Flush
                stream.flush().await?;
            }
            b'S' => {
                // Sync
                stream
                    .write_all(&encode_ready_for_query(if in_transaction {
                        b'T'
                    } else {
                        b'I'
                    }))
                    .await?;
                stream.flush().await?;
            }
            other => {
                warn!(
                    "Received unhandled PG message type '{}' (0x{:02X}) from {}",
                    other as char, other, client_addr
                );
                // Send ReadyForQuery to keep connection in sync
                stream
                    .write_all(&encode_ready_for_query(if in_transaction {
                        b'T'
                    } else {
                        b'I'
                    }))
                    .await?;
                stream.flush().await?;
            }
        }
    }

    Ok(())
}
