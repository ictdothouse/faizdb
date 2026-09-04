//! Legacy MongoDB OP_QUERY (2004) and OP_REPLY (1) implementations.

use bson::Document as BsonDocument;
use std::io::{self, Cursor, Read, Write};

use super::header::{MsgHeader, OpCode, HEADER_LEN};

/// Legacy OP_QUERY request
#[derive(Debug, Clone)]
pub struct OpQuery {
    pub header: MsgHeader,
    pub flags: i32,
    pub full_collection_name: String,
    pub number_to_skip: i32,
    pub number_to_return: i32,
    pub query: BsonDocument,
}

impl OpQuery {
    pub fn decode(src: &[u8]) -> io::Result<Self> {
        let header = MsgHeader::decode(src)?;
        let mut cursor = Cursor::new(&src[HEADER_LEN..]);

        let mut buf_i32 = [0u8; 4];
        cursor.read_exact(&mut buf_i32)?;
        let flags = i32::from_le_bytes(buf_i32);

        // Read C-String full_collection_name
        let mut name_bytes = Vec::new();
        let mut b = [0u8; 1];
        while cursor.read_exact(&mut b).is_ok() {
            if b[0] == 0 {
                break;
            }
            name_bytes.push(b[0]);
        }
        let full_collection_name = String::from_utf8_lossy(&name_bytes).to_string();

        cursor.read_exact(&mut buf_i32)?;
        let number_to_skip = i32::from_le_bytes(buf_i32);

        cursor.read_exact(&mut buf_i32)?;
        let number_to_return = i32::from_le_bytes(buf_i32);

        let query = BsonDocument::from_reader(&mut cursor)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(Self {
            header,
            flags,
            full_collection_name,
            number_to_skip,
            number_to_return,
            query,
        })
    }
}

/// Legacy OP_REPLY response
#[derive(Debug, Clone)]
pub struct OpReply {
    pub header: MsgHeader,
    pub response_flags: i32,
    pub cursor_id: i64,
    pub starting_from: i32,
    pub number_returned: i32,
    pub documents: Vec<BsonDocument>,
}

impl OpReply {
    pub fn new(request_id: i32, response_to: i32, documents: Vec<BsonDocument>) -> Self {
        let mut docs_bytes = Vec::new();
        for doc in &documents {
            let _ = doc.to_writer(&mut docs_bytes);
        }

        // 4 (flags) + 8 (cursor_id) + 4 (starting_from) + 4 (number_returned) + docs_bytes
        let body_len = 20 + docs_bytes.len();

        Self {
            header: MsgHeader::new(request_id, response_to, OpCode::OpReply, body_len),
            response_flags: 0,
            cursor_id: 0,
            starting_from: 0,
            number_returned: documents.len() as i32,
            documents,
        }
    }

    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let mut output = Vec::new();
        self.header.encode(&mut output)?;

        output.write_all(&self.response_flags.to_le_bytes())?;
        output.write_all(&self.cursor_id.to_le_bytes())?;
        output.write_all(&self.starting_from.to_le_bytes())?;
        output.write_all(&self.number_returned.to_le_bytes())?;

        for doc in &self.documents {
            doc.to_writer(&mut output)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        }

        Ok(output)
    }
}
