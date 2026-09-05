//! MySQL / MariaDB Wire Protocol v10 Encoders & Decoders.
//!
//! Handles encoding and decoding of MySQL binary wire packets (Port 3306):
//! - Packet framing (`[3-byte length LE][1-byte sequence_id][payload]`)
//! - HandshakeV10 (server initial greeting)
//! - HandshakeResponse41 (client connection response)
//! - OK_Packet (0x00) & ERR_Packet (0xFF)
//! - ColumnDefinition41 & Row Data packets
//! - EOF_Packet (0xFE)

use bytes::{Buf, BufMut, Bytes, BytesMut};

/// MySQL Field Data Types
pub const MYSQL_TYPE_TINY: u8 = 0x01;
pub const MYSQL_TYPE_SHORT: u8 = 0x02;
pub const MYSQL_TYPE_LONG: u8 = 0x03;
pub const MYSQL_TYPE_FLOAT: u8 = 0x04;
pub const MYSQL_TYPE_DOUBLE: u8 = 0x05;
pub const MYSQL_TYPE_NULL: u8 = 0x06;
pub const MYSQL_TYPE_LONGLONG: u8 = 0x08;
pub const MYSQL_TYPE_DATETIME: u8 = 0x0C;
pub const MYSQL_TYPE_VARCHAR: u8 = 0x0F;
pub const MYSQL_TYPE_JSON: u8 = 0xF5;
pub const MYSQL_TYPE_VAR_STRING: u8 = 0xFD;
pub const MYSQL_TYPE_STRING: u8 = 0xFE;

/// MySQL Capabilities Flags
pub const CLIENT_LONG_PASSWORD: u32 = 0x00000001;
pub const CLIENT_FOUND_ROWS: u32 = 0x00000002;
pub const CLIENT_LONG_FLAG: u32 = 0x00000004;
pub const CLIENT_CONNECT_WITH_DB: u32 = 0x00000008;
pub const CLIENT_PROTOCOL_41: u32 = 0x00000200;
pub const CLIENT_INTERACTIVE: u32 = 0x00000400;
pub const CLIENT_TRANSACTIONS: u32 = 0x00002000;
pub const CLIENT_SECURE_CONNECTION: u32 = 0x00008000;
pub const CLIENT_MULTI_STATEMENTS: u32 = 0x00010000;
pub const CLIENT_MULTI_RESULTS: u32 = 0x00020000;
pub const CLIENT_PLUGIN_AUTH: u32 = 0x00080000;
pub const CLIENT_CONNECT_ATTRS: u32 = 0x00100000;
pub const CLIENT_DEPRECATE_EOF: u32 = 0x01000000;

/// Server Status Flags
pub const SERVER_STATUS_AUTOCOMMIT: u16 = 0x0002;

/// Character set: utf8mb4_general_ci (45)
pub const CHARSET_UTF8MB4: u8 = 45;

/// Write a standard MySQL packet with [3-byte len][1-byte seq_id][payload]
pub fn encode_packet(seq_id: u8, payload: &[u8]) -> Bytes {
    let len = payload.len();
    let mut buf = BytesMut::with_capacity(4 + len);
    buf.put_u8((len & 0xFF) as u8);
    buf.put_u8(((len >> 8) & 0xFF) as u8);
    buf.put_u8(((len >> 16) & 0xFF) as u8);
    buf.put_u8(seq_id);
    buf.put_slice(payload);
    buf.freeze()
}

/// Write length-encoded integer into buffer
pub fn put_lenenc_int(buf: &mut BytesMut, val: u64) {
    if val < 251 {
        buf.put_u8(val as u8);
    } else if val <= 0xFFFF {
        buf.put_u8(0xFC);
        buf.put_u16_le(val as u16);
    } else if val <= 0xFFFFFF {
        buf.put_u8(0xFD);
        buf.put_u8((val & 0xFF) as u8);
        buf.put_u8(((val >> 8) & 0xFF) as u8);
        buf.put_u8(((val >> 16) & 0xFF) as u8);
    } else {
        buf.put_u8(0xFE);
        buf.put_u64_le(val);
    }
}

/// Write length-encoded string into buffer
pub fn put_lenenc_str(buf: &mut BytesMut, s: &str) {
    let bytes = s.as_bytes();
    put_lenenc_int(buf, bytes.len() as u64);
    buf.put_slice(bytes);
}

/// Read length-encoded integer from buffer
pub fn read_lenenc_int(buf: &mut Bytes) -> Option<u64> {
    if buf.is_empty() {
        return None;
    }
    let first = buf.get_u8();
    match first {
        0..=250 => Some(first as u64),
        0xFC => {
            if buf.remaining() < 2 {
                return None;
            }
            Some(buf.get_u16_le() as u64)
        }
        0xFD => {
            if buf.remaining() < 3 {
                return None;
            }
            let b0 = buf.get_u8() as u64;
            let b1 = buf.get_u8() as u64;
            let b2 = buf.get_u8() as u64;
            Some(b0 | (b1 << 8) | (b2 << 16))
        }
        0xFE => {
            if buf.remaining() < 8 {
                return None;
            }
            Some(buf.get_u64_le())
        }
        _ => None, // 0xFF is error / undefined
    }
}

/// Read null-terminated string from buffer
pub fn read_null_terminated_str(buf: &mut Bytes) -> Option<String> {
    let mut pos = 0;
    while pos < buf.len() {
        if buf[pos] == 0 {
            let str_bytes = buf.split_to(pos);
            buf.advance(1); // consume null byte
            return String::from_utf8(str_bytes.to_vec()).ok();
        }
        pos += 1;
    }
    None
}

/// Build HandshakeV10 packet (Server Initial Greeting)
pub fn build_handshake_v10(connection_id: u32, salt: &[u8; 20]) -> Bytes {
    let mut payload = BytesMut::with_capacity(128);

    // 1. Protocol version 10
    payload.put_u8(10);

    // 2. Server version string (null-terminated)
    payload.put_slice(b"8.0.35-FaizDB-Universal\0");

    // 3. Thread ID (Connection ID)
    payload.put_u32_le(connection_id);

    // 4. Auth plugin data part 1 (first 8 bytes of salt)
    payload.put_slice(&salt[0..8]);

    // 5. Filter / constant 0x00
    payload.put_u8(0x00);

    // 6. Capability flags lower 2 bytes
    let server_capabilities = CLIENT_LONG_PASSWORD
        | CLIENT_FOUND_ROWS
        | CLIENT_LONG_FLAG
        | CLIENT_CONNECT_WITH_DB
        | CLIENT_PROTOCOL_41
        | CLIENT_INTERACTIVE
        | CLIENT_TRANSACTIONS
        | CLIENT_SECURE_CONNECTION
        | CLIENT_MULTI_STATEMENTS
        | CLIENT_MULTI_RESULTS
        | CLIENT_PLUGIN_AUTH
        | CLIENT_CONNECT_ATTRS;

    payload.put_u16_le((server_capabilities & 0xFFFF) as u16);

    // 7. Character set: utf8mb4 (45)
    payload.put_u8(CHARSET_UTF8MB4);

    // 8. Server status: autocommit (2)
    payload.put_u16_le(SERVER_STATUS_AUTOCOMMIT);

    // 9. Capability flags upper 2 bytes
    payload.put_u16_le(((server_capabilities >> 16) & 0xFFFF) as u16);

    // 10. Auth plugin data length (21 = 20-byte salt + null byte)
    payload.put_u8(21);

    // 11. Reserved (10 bytes zero)
    payload.put_slice(&[0u8; 10]);

    // 12. Auth plugin data part 2 (remaining 12 bytes of salt + 0x00)
    payload.put_slice(&salt[8..20]);
    payload.put_u8(0x00);

    // 13. Auth plugin name (null-terminated)
    payload.put_slice(b"mysql_native_password\0");

    encode_packet(0, &payload)
}

/// Parsed Client Handshake Response (Protocol 41)
#[derive(Debug, Clone)]
pub struct HandshakeResponse {
    pub client_capabilities: u32,
    pub max_packet_size: u32,
    pub charset: u8,
    pub username: String,
    pub auth_response: Vec<u8>,
    pub database: Option<String>,
    pub auth_plugin_name: Option<String>,
}

/// Parse client HandshakeResponse41 packet
pub fn parse_handshake_response(mut payload: Bytes) -> Result<HandshakeResponse, String> {
    if payload.len() < 32 {
        return Err("Handshake response packet too short".to_string());
    }

    let client_capabilities = payload.get_u32_le();
    let max_packet_size = payload.get_u32_le();
    let charset = payload.get_u8();

    // 23 reserved bytes
    if payload.remaining() < 23 {
        return Err("Truncated reserved bytes in handshake response".to_string());
    }
    payload.advance(23);

    // Username (null-terminated)
    let username = read_null_terminated_str(&mut payload)
        .ok_or_else(|| "Missing username in handshake response".to_string())?;

    // Auth response data
    let auth_response = if (client_capabilities & CLIENT_PLUGIN_AUTH) != 0 {
        if payload.is_empty() {
            Vec::new()
        } else {
            let auth_len = payload.get_u8() as usize;
            if payload.remaining() >= auth_len {
                let bytes = payload.split_to(auth_len);
                bytes.to_vec()
            } else {
                Vec::new()
            }
        }
    } else if (client_capabilities & CLIENT_SECURE_CONNECTION) != 0 {
        if payload.is_empty() {
            Vec::new()
        } else {
            let auth_len = payload.get_u8() as usize;
            if payload.remaining() >= auth_len {
                let bytes = payload.split_to(auth_len);
                bytes.to_vec()
            } else {
                Vec::new()
            }
        }
    } else {
        read_null_terminated_str(&mut payload)
            .map(|s| s.into_bytes())
            .unwrap_or_default()
    };

    // Database (if CLIENT_CONNECT_WITH_DB is set)
    let database = if (client_capabilities & CLIENT_CONNECT_WITH_DB) != 0 {
        read_null_terminated_str(&mut payload)
    } else {
        None
    };

    // Auth plugin name (if CLIENT_PLUGIN_AUTH is set)
    let auth_plugin_name = if (client_capabilities & CLIENT_PLUGIN_AUTH) != 0 {
        read_null_terminated_str(&mut payload)
    } else {
        None
    };

    Ok(HandshakeResponse {
        client_capabilities,
        max_packet_size,
        charset,
        username,
        auth_response,
        database,
        auth_plugin_name,
    })
}

/// Build OK_Packet (0x00)
pub fn build_ok_packet(seq_id: u8, affected_rows: u64, last_insert_id: u64, info: &str) -> Bytes {
    let mut payload = BytesMut::with_capacity(32 + info.len());
    payload.put_u8(0x00); // OK header
    put_lenenc_int(&mut payload, affected_rows);
    put_lenenc_int(&mut payload, last_insert_id);
    payload.put_u16_le(SERVER_STATUS_AUTOCOMMIT);
    payload.put_u16_le(0); // 0 warnings
    if !info.is_empty() {
        put_lenenc_str(&mut payload, info);
    }
    encode_packet(seq_id, &payload)
}

/// Build ERR_Packet (0xFF)
pub fn build_err_packet(seq_id: u8, error_code: u16, sql_state: &str, error_msg: &str) -> Bytes {
    let mut payload = BytesMut::with_capacity(16 + error_msg.len());
    payload.put_u8(0xFF); // ERR header
    payload.put_u16_le(error_code);
    payload.put_u8(b'#'); // SQL State marker
    let state_bytes = sql_state.as_bytes();
    if state_bytes.len() >= 5 {
        payload.put_slice(&state_bytes[0..5]);
    } else {
        payload.put_slice(b"HY000");
    }
    payload.put_slice(error_msg.as_bytes());
    encode_packet(seq_id, &payload)
}

/// Build ColumnDefinition41 Packet
pub fn build_column_def(
    seq_id: u8,
    db: &str,
    table: &str,
    name: &str,
    column_type: u8,
    column_length: u32,
) -> Bytes {
    let mut payload = BytesMut::with_capacity(64);
    put_lenenc_str(&mut payload, "def"); // Catalog
    put_lenenc_str(&mut payload, db); // Schema / DB
    put_lenenc_str(&mut payload, table); // Table
    put_lenenc_str(&mut payload, table); // Org table
    put_lenenc_str(&mut payload, name); // Name
    put_lenenc_str(&mut payload, name); // Org name
    payload.put_u8(0x0C); // Length of fixed-length fields
    payload.put_u16_le(CHARSET_UTF8MB4 as u16);
    payload.put_u32_le(column_length);
    payload.put_u8(column_type);
    payload.put_u16_le(0); // Flags
    payload.put_u8(0); // Decimals
    payload.put_u16_le(0); // Filler (2 bytes reserved)
    encode_packet(seq_id, &payload)
}

/// Build EOF_Packet (0xFE)
pub fn build_eof_packet(seq_id: u8) -> Bytes {
    let mut payload = BytesMut::with_capacity(8);
    payload.put_u8(0xFE);
    payload.put_u16_le(0); // Warnings
    payload.put_u16_le(SERVER_STATUS_AUTOCOMMIT);
    encode_packet(seq_id, &payload)
}

/// Build Row Data Packet from string values
pub fn build_row_packet(seq_id: u8, values: &[Option<String>]) -> Bytes {
    let mut payload = BytesMut::with_capacity(32 * values.len());
    for val in values {
        match val {
            Some(s) => put_lenenc_str(&mut payload, s),
            None => payload.put_u8(0xFB), // 0xFB represents NULL in MySQL row packet
        }
    }
    encode_packet(seq_id, &payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_packet_and_handshake() {
        let salt = [1u8; 20];
        let packet = build_handshake_v10(1, &salt);
        assert!(packet.len() > 30);
        let len = (packet[0] as usize) | ((packet[1] as usize) << 8) | ((packet[2] as usize) << 16);
        assert_eq!(len, packet.len() - 4);
        assert_eq!(packet[3], 0); // Seq ID = 0
        assert_eq!(packet[4], 10); // Handshake V10
    }

    #[test]
    fn test_ok_and_err_packet() {
        let ok = build_ok_packet(1, 4, 10, "OK");
        assert_eq!(ok[3], 1); // Seq ID
        assert_eq!(ok[4], 0x00); // OK Header

        let err = build_err_packet(2, 1064, "42000", "Syntax error");
        assert_eq!(err[3], 2);
        assert_eq!(err[4], 0xFF); // ERR Header
    }

    #[test]
    fn test_column_def_and_row_packet() {
        let col = build_column_def(1, "faizdb", "users", "name", MYSQL_TYPE_VAR_STRING, 255);
        assert_eq!(col[3], 1);

        let row = build_row_packet(2, &[Some("Faiz".to_string()), None]);
        assert_eq!(row[3], 2);
    }
}
