//! MongoDB Wire Protocol Message Header (16 bytes)
//!
//! Standard header for all MongoDB wire protocol packets.

use std::io::{self, Cursor, Read, Write};

pub const HEADER_LEN: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OpCode {
    OpReply = 1,
    OpQuery = 2004,
    OpMsg = 2013,
    Unknown(i32),
}

impl From<i32> for OpCode {
    fn from(val: i32) -> Self {
        match val {
            1 => OpCode::OpReply,
            2004 => OpCode::OpQuery,
            2013 => OpCode::OpMsg,
            other => OpCode::Unknown(other),
        }
    }
}

impl From<OpCode> for i32 {
    fn from(code: OpCode) -> Self {
        match code {
            OpCode::OpReply => 1,
            OpCode::OpQuery => 2004,
            OpCode::OpMsg => 2013,
            OpCode::Unknown(val) => val,
        }
    }
}

/// 16-byte message header
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsgHeader {
    pub message_length: i32,
    pub request_id: i32,
    pub response_to: i32,
    pub op_code: OpCode,
}

impl MsgHeader {
    pub fn new(request_id: i32, response_to: i32, op_code: OpCode, body_len: usize) -> Self {
        Self {
            message_length: (HEADER_LEN + body_len) as i32,
            request_id,
            response_to,
            op_code,
        }
    }

    pub fn decode(src: &[u8]) -> io::Result<Self> {
        if src.len() < HEADER_LEN {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Incomplete message header",
            ));
        }

        let mut cursor = Cursor::new(src);
        let mut buf_i32 = [0u8; 4];

        cursor.read_exact(&mut buf_i32)?;
        let message_length = i32::from_le_bytes(buf_i32);

        cursor.read_exact(&mut buf_i32)?;
        let request_id = i32::from_le_bytes(buf_i32);

        cursor.read_exact(&mut buf_i32)?;
        let response_to = i32::from_le_bytes(buf_i32);

        cursor.read_exact(&mut buf_i32)?;
        let op_code = OpCode::from(i32::from_le_bytes(buf_i32));

        Ok(Self {
            message_length,
            request_id,
            response_to,
            op_code,
        })
    }

    pub fn encode(&self, dst: &mut Vec<u8>) -> io::Result<()> {
        dst.write_all(&self.message_length.to_le_bytes())?;
        dst.write_all(&self.request_id.to_le_bytes())?;
        dst.write_all(&self.response_to.to_le_bytes())?;
        let code_i32: i32 = self.op_code.into();
        dst.write_all(&code_i32.to_le_bytes())?;
        Ok(())
    }
}
