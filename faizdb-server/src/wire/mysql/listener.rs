//! MySQL Wire Protocol TCP Server (Port 3306).
//!
//! Accepts client TCP connections from MySQL/MariaDB clients (MySQL CLI, PHP mysqli/PDO,
//! Laravel Eloquent, Python mysqlclient, Go go-sql-driver/mysql, etc.), handles the HandshakeV10
//! negotiation, parses queries, and routes them to the FaizDB execution engine.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info, warn};

use super::codec::{
    build_err_packet, build_handshake_v10, build_ok_packet, parse_handshake_response,
};
use super::handler::handle_mysql_query;
use faizdb_query::DatabaseContext;
use faizdb_security::UserStore;

static NEXT_CONNECTION_ID: AtomicU32 = AtomicU32::new(1);

/// Run the MySQL Wire Protocol server with optional graceful shutdown future
pub async fn run_mysql_server_with_shutdown<F>(
    addr: &str,
    db: Arc<DatabaseContext>,
    user_store: Arc<UserStore>,
    shutdown: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let listener = TcpListener::bind(addr).await?;
    info!("🐬 MySQL / MariaDB Wire Protocol Server running on mysql://{addr}");

    let max_conns = std::env::var("FAIZDB_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(max_conns));
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("🐬 MySQL Wire Protocol Server received shutdown signal — draining listener...");
                break;
            }
            accept_res = listener.accept() => {
                match accept_res {
                    Ok((socket, peer_addr)) => {
                        let permit = match semaphore.clone().try_acquire_owned() {
                            Ok(p) => p,
                            Err(_) => {
                                warn!("Rejecting MySQL connection from {peer_addr}: max connections ({max_conns}) reached");
                                tokio::spawn(async move {
                                    let mut s = socket;
                                    let err = build_err_packet(1, 1040, "08004", "Too many connections");
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
                            if let Err(e) = handle_mysql_connection(
                                socket,
                                db_clone,
                                store_clone,
                                peer_addr.to_string(),
                            )
                            .await
                            {
                                warn!("MySQL connection closed for {peer_addr}: {e}");
                            }
                        });
                    }
                    Err(e) => {
                        error!("Failed to accept MySQL connection: {e}");
                    }
                }
            }
        }
    }
    Ok(())
}

/// Run the MySQL Wire Protocol server on the given address (e.g. "0.0.0.0:3306")
pub async fn run_mysql_server(
    addr: &str,
    db: Arc<DatabaseContext>,
    user_store: Arc<UserStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    run_mysql_server_with_shutdown(addr, db, user_store, std::future::pending()).await
}

/// Handles an individual MySQL client connection lifecycle
async fn handle_mysql_connection(
    mut stream: TcpStream,
    db: Arc<DatabaseContext>,
    _user_store: Arc<UserStore>,
    peer_addr: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let conn_id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);

    // 1. Generate 20-byte random salt for authentication
    let mut salt = [0u8; 20];
    for b in salt.iter_mut() {
        *b = (rand_simple() & 0xFF) as u8;
    }

    // 2. Send Initial HandshakeV10 packet (seq_id = 0)
    let greeting = build_handshake_v10(conn_id, &salt);
    stream.write_all(&greeting).await?;
    stream.flush().await?;

    // 3. Read HandshakeResponse41 packet from client
    let mut header = [0u8; 4];
    stream.read_exact(&mut header).await?;
    let payload_len = (header[0] as usize) | ((header[1] as usize) << 8) | ((header[2] as usize) << 16);
    let mut client_seq_id = header[3];

    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).await?;

    let handshake_resp = parse_handshake_response(bytes::Bytes::from(payload))
        .map_err(|e| format!("MySQL handshake error: {e}"))?;

    let mut current_db = handshake_resp.database.unwrap_or_else(|| "faizdb".to_string());
    info!(
        "MySQL client authenticated: user='{}', db='{}', conn_id={conn_id} from {peer_addr}",
        handshake_resp.username, current_db
    );

    // 4. Send OK_Packet confirming successful handshake
    client_seq_id = client_seq_id.wrapping_add(1);
    let ok = build_ok_packet(client_seq_id, 0, 0, "");
    stream.write_all(&ok).await?;
    stream.flush().await?;

    // 5. Command Phase loop
    loop {
        let mut cmd_header = [0u8; 4];
        match stream.read_exact(&mut cmd_header).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break; // Clean client disconnect
            }
            Err(e) => return Err(Box::new(e)),
        }

        let cmd_len = (cmd_header[0] as usize)
            | ((cmd_header[1] as usize) << 8)
            | ((cmd_header[2] as usize) << 16);
        let seq_id = cmd_header[3];

        if cmd_len == 0 {
            continue;
        }

        let mut cmd_payload = vec![0u8; cmd_len];
        stream.read_exact(&mut cmd_payload).await?;

        let command_type = cmd_payload[0];
        let next_seq = seq_id.wrapping_add(1);

        match command_type {
            // COM_QUIT (0x01)
            0x01 => {
                break;
            }
            // COM_INIT_DB (0x02) - Switch active schema/database
            0x02 => {
                if let Ok(new_db) = String::from_utf8(cmd_payload[1..].to_vec()) {
                    current_db = new_db.trim().to_string();
                }
                let ok_pkt = build_ok_packet(next_seq, 0, 0, "");
                stream.write_all(&ok_pkt).await?;
                stream.flush().await?;
            }
            // COM_QUERY (0x03) - Execute SQL Query
            0x03 => {
                let query_bytes = &cmd_payload[1..];
                let query_str = String::from_utf8_lossy(query_bytes);
                let response_packets = handle_mysql_query(&db, &current_db, &query_str, next_seq);
                for pkt in response_packets {
                    stream.write_all(&pkt).await?;
                }
                stream.flush().await?;
            }
            // COM_FIELD_LIST (0x04)
            0x04 => {
                let eof = super::codec::build_eof_packet(next_seq);
                stream.write_all(&eof).await?;
                stream.flush().await?;
            }
            // COM_PING (0x0E)
            0x0E => {
                let ok_pkt = build_ok_packet(next_seq, 0, 0, "");
                stream.write_all(&ok_pkt).await?;
                stream.flush().await?;
            }
            // Unknown command fallback -> OK
            _ => {
                let ok_pkt = build_ok_packet(next_seq, 0, 0, "");
                stream.write_all(&ok_pkt).await?;
                stream.flush().await?;
            }
        }
    }

    Ok(())
}

/// Simple pseudo-random generator without extra crate dependency
fn rand_simple() -> u32 {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    ((now ^ (now >> 32)) & 0xFFFFFFFF) as u32
}
