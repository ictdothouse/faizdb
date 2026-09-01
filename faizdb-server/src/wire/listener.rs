//! MongoDB Wire Protocol TCP Server (Port 27017).
//!
//! Accepts client TCP connections, decodes wire protocol messages,
//! executes them against the FaizDB core engine, and streams responses back.

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

use faizdb_query::DatabaseContext;
use super::header::{MsgHeader, OpCode, HEADER_LEN};
use super::op_msg::OpMsg;
use super::op_query::{OpQuery, OpReply};
use super::handler::handle_op_msg;

/// Run the MongoDB Wire Protocol server on the given address (e.g. "0.0.0.0:27017")
pub async fn run_wire_server(addr: &str, db: Arc<DatabaseContext>) -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(addr).await?;
    info!("🍃 MongoDB Wire Protocol Server running on mongodb://{addr}");

    loop {
        match listener.accept().await {
            Ok((socket, peer_addr)) => {
                let db_clone = db.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(socket, db_clone, peer_addr.to_string()).await {
                        error!("Connection error from {peer_addr}: {e}");
                    }
                });
            }
            Err(e) => {
                error!("Failed to accept wire connection: {e}");
            }
        }
    }
}

async fn handle_connection(
    mut stream: TcpStream,
    db: Arc<DatabaseContext>,
    client_addr: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut header_buf = [0u8; HEADER_LEN];

    loop {
        // Read 16-byte header
        if stream.read_exact(&mut header_buf).await.is_err() {
            // Client disconnected cleanly
            break;
        }

        let header = MsgHeader::decode(&header_buf)?;
        let body_len = (header.message_length as usize).saturating_sub(HEADER_LEN);

        let mut full_msg = Vec::with_capacity(header.message_length as usize);
        full_msg.extend_from_slice(&header_buf);

        if body_len > 0 {
            let mut body_buf = vec![0u8; body_len];
            stream.read_exact(&mut body_buf).await?;
            full_msg.extend_from_slice(&body_buf);
        }

        match header.op_code {
            OpCode::OpMsg => {
                let op_msg = OpMsg::decode(&full_msg)?;
                let reply_msg = handle_op_msg(&db, &op_msg, &client_addr);
                let reply_bytes = reply_msg.encode()?;
                stream.write_all(&reply_bytes).await?;
                stream.flush().await?;
            }
            OpCode::OpQuery => {
                let op_query = OpQuery::decode(&full_msg)?;
                // Convert legacy OP_QUERY into a dummy OP_MSG and reply with OP_REPLY
                let dummy_msg = OpMsg::response(op_query.header.request_id, 0, op_query.query);
                let reply_msg = handle_op_msg(&db, &dummy_msg, &client_addr);

                let reply_doc = reply_msg.primary_document().cloned().unwrap_or_default();
                let op_reply = OpReply::new(0, op_query.header.request_id, vec![reply_doc]);
                let reply_bytes = op_reply.encode()?;
                stream.write_all(&reply_bytes).await?;
                stream.flush().await?;
            }
            OpCode::Unknown(val) => {
                tracing::warn!("Ignoring unknown OpCode {val} from {client_addr}");
            }
            _ => {}
        }
    }

    Ok(())
}
