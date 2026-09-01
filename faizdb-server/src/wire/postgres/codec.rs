//! PostgreSQL Frontend/Backend Protocol v3.0 Encoders & Decoders.
//!
//! Handles encoding and decoding of PostgreSQL binary wire packets:
//! - StartupMessage & SSLRequest
//! - AuthenticationOk, ParameterStatus, BackendKeyData, ReadyForQuery
//! - RowDescription, DataRow, CommandComplete, ErrorResponse

use bytes::{BufMut, BytesMut};

/// Well-known PostgreSQL DataType Object Identifiers (OIDs)
pub const PG_TYPE_BOOL: i32 = 16;
pub const PG_TYPE_INT8: i32 = 20;
pub const PG_TYPE_INT2: i32 = 21;
pub const PG_TYPE_INT4: i32 = 23;
pub const PG_TYPE_TEXT: i32 = 25;
pub const PG_TYPE_FLOAT4: i32 = 700;
pub const PG_TYPE_FLOAT8: i32 = 701;
pub const PG_TYPE_VARCHAR: i32 = 1043;
pub const PG_TYPE_JSON: i32 = 114;
pub const PG_TYPE_JSONB: i32 = 3802;

/// Field description in a `RowDescription` ('T') message
#[derive(Debug, Clone)]
pub struct PgField {
    pub name: String,
    pub table_oid: i32,
    pub column_attr_num: i16,
    pub type_oid: i32,
    pub type_size: i16,
    pub type_modifier: i32,
    pub format_code: i16, // 0 = text, 1 = binary
}

impl PgField {
    pub fn new(name: impl Into<String>, type_oid: i32) -> Self {
        let (type_size, type_modifier) = match type_oid {
            PG_TYPE_BOOL => (1, -1),
            PG_TYPE_INT2 => (2, -1),
            PG_TYPE_INT4 => (4, -1),
            PG_TYPE_INT8 => (8, -1),
            PG_TYPE_FLOAT4 => (4, -1),
            PG_TYPE_FLOAT8 => (8, -1),
            _ => (-1, -1), // variable length (text, varchar, jsonb)
        };

        Self {
            name: name.into(),
            table_oid: 0,
            column_attr_num: 0,
            type_oid,
            type_size,
            type_modifier,
            format_code: 0, // Text format default
        }
    }

    pub fn text(name: impl Into<String>) -> Self {
        Self::new(name, PG_TYPE_TEXT)
    }
}

/// Encode `AuthenticationOk` message: 'R' + len (8) + code (0)
pub fn encode_auth_ok() -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(9);
    buf.put_u8(b'R');
    buf.put_i32(8);
    buf.put_i32(0);
    buf.to_vec()
}

/// Encode `ParameterStatus` message: 'S' + len + name\0 + value\0
pub fn encode_parameter_status(name: &str, value: &str) -> Vec<u8> {
    let payload_len = 4 + name.len() + 1 + value.len() + 1;
    let mut buf = BytesMut::with_capacity(1 + payload_len);
    buf.put_u8(b'S');
    buf.put_i32(payload_len as i32);
    buf.put_slice(name.as_bytes());
    buf.put_u8(0);
    buf.put_slice(value.as_bytes());
    buf.put_u8(0);
    buf.to_vec()
}

/// Encode `BackendKeyData` message: 'K' + len (12) + pid + secret_key
pub fn encode_backend_key_data(pid: i32, secret_key: i32) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(13);
    buf.put_u8(b'K');
    buf.put_i32(12);
    buf.put_i32(pid);
    buf.put_i32(secret_key);
    buf.to_vec()
}

/// Encode `ReadyForQuery` message: 'Z' + len (5) + status ('I' for idle, 'T' for in-txn)
pub fn encode_ready_for_query(status: u8) -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(6);
    buf.put_u8(b'Z');
    buf.put_i32(5);
    buf.put_u8(status);
    buf.to_vec()
}

/// Encode `CommandComplete` message: 'C' + len + tag\0
pub fn encode_command_complete(tag: &str) -> Vec<u8> {
    let payload_len = 4 + tag.len() + 1;
    let mut buf = BytesMut::with_capacity(1 + payload_len);
    buf.put_u8(b'C');
    buf.put_i32(payload_len as i32);
    buf.put_slice(tag.as_bytes());
    buf.put_u8(0);
    buf.to_vec()
}

/// Encode `RowDescription` message: 'T' + len + field_count + fields
pub fn encode_row_description(fields: &[PgField]) -> Vec<u8> {
    let mut fields_len = 2; // field count (i16)
    for f in fields {
        fields_len += f.name.len() + 1 + 4 + 2 + 4 + 2 + 4 + 2;
    }
    let payload_len = 4 + fields_len;

    let mut buf = BytesMut::with_capacity(1 + payload_len);
    buf.put_u8(b'T');
    buf.put_i32(payload_len as i32);
    buf.put_i16(fields.len() as i16);

    for f in fields {
        buf.put_slice(f.name.as_bytes());
        buf.put_u8(0);
        buf.put_i32(f.table_oid);
        buf.put_i16(f.column_attr_num);
        buf.put_i32(f.type_oid);
        buf.put_i16(f.type_size);
        buf.put_i32(f.type_modifier);
        buf.put_i16(f.format_code);
    }

    buf.to_vec()
}

/// Encode `DataRow` message: 'D' + len + col_count + [col_len (i32) + col_data]
pub fn encode_data_row(columns: &[Option<String>]) -> Vec<u8> {
    let mut data_len = 2; // column count (i16)
    for col in columns {
        match col {
            Some(s) => data_len += 4 + s.len(),
            None => data_len += 4, // -1 i32 for NULL
        }
    }
    let payload_len = 4 + data_len;

    let mut buf = BytesMut::with_capacity(1 + payload_len);
    buf.put_u8(b'D');
    buf.put_i32(payload_len as i32);
    buf.put_i16(columns.len() as i16);

    for col in columns {
        match col {
            Some(s) => {
                buf.put_i32(s.len() as i32);
                buf.put_slice(s.as_bytes());
            }
            None => {
                buf.put_i32(-1); // NULL indicator in PG protocol
            }
        }
    }

    buf.to_vec()
}

/// Encode `ErrorResponse` message: 'E' + len + fields ('S' severity, 'C' code, 'M' message, '\0')
pub fn encode_error_response(severity: &str, code: &str, message: &str) -> Vec<u8> {
    let mut payload_len = 4;
    payload_len += 1 + severity.len() + 1; // 'S'
    payload_len += 1 + code.len() + 1;     // 'C'
    payload_len += 1 + message.len() + 1;  // 'M'
    payload_len += 1;                      // Terminator '\0'

    let mut buf = BytesMut::with_capacity(1 + payload_len);
    buf.put_u8(b'E');
    buf.put_i32(payload_len as i32);

    buf.put_u8(b'S');
    buf.put_slice(severity.as_bytes());
    buf.put_u8(0);

    buf.put_u8(b'C');
    buf.put_slice(code.as_bytes());
    buf.put_u8(0);

    buf.put_u8(b'M');
    buf.put_slice(message.as_bytes());
    buf.put_u8(0);

    buf.put_u8(0); // final null terminator
    buf.to_vec()
}

/// Encode `EmptyQueryResponse` message: 'I' + len (4)
pub fn encode_empty_query_response() -> Vec<u8> {
    let mut buf = BytesMut::with_capacity(5);
    buf.put_u8(b'I');
    buf.put_i32(4);
    buf.to_vec()
}
