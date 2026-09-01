//! PostgreSQL Wire Protocol TCP Server (Port 5432).
//!
//! Accepts client TCP connections from PostgreSQL clients (psql, DBeaver, TablePlus,
//! Node.js `pg`, Prisma, GORM, Python asyncpg, etc.), handles the SSL and Startup handshake,
//! parses queries, and routes them to the FaizDB query execution engine.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

use faizdb_query::DatabaseContext;
use super::codec::{
    encode_auth_ok, encode_backend_key_data, encode_parameter_status, encode_ready_for_query,
};
use super::handler::handle_postgres_query;

/// SSLRequest code sent by PostgreSQL clients (80877103 in decimal)
const PG_SSL_REQUEST_CODE: i32 = 80877103;

/// PostgreSQL Protocol v3.0 code (196608 in decimal / 0x00030000)
const PG_PROTOCOL_V3: i32 = 196608;

/// Run the PostgreSQL Wire Protocol server on the given address (e.g. "0.0.0.0:5432")
pub async fn run_postgres_server(
    addr: &str,
    db: Arc<DatabaseContext>,
) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    info!("🐘 PostgreSQL Wire Protocol Server running on postgresql://{addr}");

    loop {
        match listener.accept().await {
            Ok((socket, peer_addr)) => {
                let db_clone = db.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_postgres_connection(socket, db_clone, peer_addr.to_string()).await {
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

async fn handle_postgres_connection(
    mut stream: TcpStream,
    db: Arc<DatabaseContext>,
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
        if msg_len < 4 {
            return Err("Invalid Postgres message length".into());
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
            // Send AuthenticationOk
            stream.write_all(&encode_auth_ok()).await?;

            // Send standard ParameterStatus messages
            stream.write_all(&encode_parameter_status("server_version", "16.0 (FaizDB)")).await?;
            stream.write_all(&encode_parameter_status("client_encoding", "UTF8")).await?;
            stream.write_all(&encode_parameter_status("server_encoding", "UTF8")).await?;
            stream.write_all(&encode_parameter_status("DateStyle", "ISO, MDY")).await?;
            stream.write_all(&encode_parameter_status("integer_datetimes", "on")).await?;
            stream.write_all(&encode_parameter_status("standard_conforming_strings", "on")).await?;
            stream.write_all(&encode_parameter_status("TimeZone", "UTC")).await?;

            // Send BackendKeyData (pid 1001, secret 2002)
            stream.write_all(&encode_backend_key_data(1001, 2002)).await?;

            // Send ReadyForQuery (status 'I' for idle)
            stream.write_all(&encode_ready_for_query(b'I')).await?;
            stream.flush().await?;
            break;
        }

        warn!("Unsupported Postgres protocol version {code} from {client_addr}");
        return Ok(());
    }

    // 2. Command processing loop (handling 'Q', 'X', etc.)
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
        let body_len = (msg_len - 4).max(0) as usize;

        let mut body = vec![0u8; body_len];
        if body_len > 0 {
            if stream.read_exact(&mut body).await.is_err() {
                break;
            }
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
                stream.write_all(&encode_ready_for_query(if in_transaction { b'T' } else { b'I' })).await?;
                stream.flush().await?;
            }
            other => {
                warn!("Received unhandled PG message type '{}' (0x{:02X}) from {}", other as char, other, client_addr);
                // Send ReadyForQuery to keep connection in sync
                stream.write_all(&encode_ready_for_query(if in_transaction { b'T' } else { b'I' })).await?;
                stream.flush().await?;
            }
        }
    }

    Ok(())
}
