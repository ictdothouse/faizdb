//! MongoDB OP_MSG (OpCode 2013) Specification Implementation.
//!
//! Standard message format used by all modern MongoDB drivers (v3.6+).

use bson::Document as BsonDocument;
use std::io::{self, Cursor, Read, Write};

use super::header::{MsgHeader, OpCode, HEADER_LEN};

/// A Section within an OP_MSG
#[derive(Debug, Clone)]
pub enum Section {
    /// Kind 0: Standard single BSON document body
    Body(BsonDocument),
    /// Kind 1: Document sequence (identifier e.g. "documents" + stream of BSON documents)
    Sequence {
        identifier: String,
        documents: Vec<BsonDocument>,
    },
}

/// An OP_MSG message
#[derive(Debug, Clone)]
pub struct OpMsg {
    pub header: MsgHeader,
    pub flags: u32,
    pub sections: Vec<Section>,
    pub checksum: Option<u32>,
}

impl OpMsg {
    /// Create an OP_MSG response from a single BSON document
    pub fn response(request_id: i32, response_to: i32, body: BsonDocument) -> Self {
        let mut body_bytes = Vec::new();
        body.to_writer(&mut body_bytes).unwrap_or_default();

        // 4 bytes flag + 1 byte kind 0 + body_bytes.len()
        let total_body_len = 4 + 1 + body_bytes.len();

        Self {
            header: MsgHeader::new(request_id, response_to, OpCode::OpMsg, total_body_len),
            flags: 0,
            sections: vec![Section::Body(body)],
            checksum: None,
        }
    }

    /// Extract the primary command body document
    pub fn primary_document(&self) -> Option<&BsonDocument> {
        for sec in &self.sections {
            if let Section::Body(doc) = sec {
                return Some(doc);
            }
        }
        None
    }

    /// Extract document sequence list if present (e.g. for bulk inserts)
    pub fn document_sequence(&self, expected_identifier: &str) -> Vec<BsonDocument> {
        for sec in &self.sections {
            if let Section::Sequence {
                identifier,
                documents,
            } = sec
            {
                if identifier == expected_identifier {
                    return documents.clone();
                }
            }
        }
        Vec::new()
    }

    /// Decode an OP_MSG from raw bytes (including 16-byte header)
    pub fn decode(src: &[u8]) -> io::Result<Self> {
        if src.len() < HEADER_LEN + 4 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "OP_MSG too short",
            ));
        }

        let header = MsgHeader::decode(src)?;
        let mut cursor = Cursor::new(&src[HEADER_LEN..]);

        let mut flag_bytes = [0u8; 4];
        cursor.read_exact(&mut flag_bytes)?;
        let flags = u32::from_le_bytes(flag_bytes);

        let checksum_present = (flags & 1) != 0;
        let payload_len = if checksum_present {
            src.len().saturating_sub(4)
        } else {
            src.len()
        };

        let mut sections = Vec::new();
        let payload_cursor_limit = payload_len - HEADER_LEN;

        while (cursor.position() as usize) < payload_cursor_limit {
            let mut kind_byte = [0u8; 1];
            if cursor.read_exact(&mut kind_byte).is_err() {
                break;
            }

            match kind_byte[0] {
                0 => {
                    // Kind 0: Single BSON Document
                    let doc = BsonDocument::from_reader(&mut cursor)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    sections.push(Section::Body(doc));
                }
                1 => {
                    // Kind 1: Document Sequence
                    let mut size_buf = [0u8; 4];
                    cursor.read_exact(&mut size_buf)?;
                    let sec_size = i32::from_le_bytes(size_buf) as usize;

                    // Read C-string identifier
                    let mut id_bytes = Vec::new();
                    let mut b = [0u8; 1];
                    while cursor.read_exact(&mut b).is_ok() {
                        if b[0] == 0 {
                            break;
                        }
                        id_bytes.push(b[0]);
                    }
                    let identifier = String::from_utf8_lossy(&id_bytes).to_string();

                    // Read BSON documents until sec_size is consumed
                    let mut documents = Vec::new();
                    let sequence_end = cursor.position() as usize
                        + sec_size.saturating_sub(4 + id_bytes.len() + 1);

                    while (cursor.position() as usize) < sequence_end
                        && (cursor.position() as usize) < payload_cursor_limit
                    {
                        match BsonDocument::from_reader(&mut cursor) {
                            Ok(doc) => documents.push(doc),
                            Err(_) => break,
                        }
                    }

                    sections.push(Section::Sequence {
                        identifier,
                        documents,
                    });
                }
                _ => break,
            }
        }

        let checksum = if checksum_present && src.len() >= 4 {
            let mut cs_bytes = [0u8; 4];
            let cs_pos = src.len() - 4;
            cs_bytes.copy_from_slice(&src[cs_pos..]);
            Some(u32::from_le_bytes(cs_bytes))
        } else {
            None
        };

        Ok(Self {
            header,
            flags,
            sections,
            checksum,
        })
    }

    /// Encode OP_MSG to bytes
    pub fn encode(&self) -> io::Result<Vec<u8>> {
        let mut body_bytes = Vec::new();
        body_bytes.write_all(&self.flags.to_le_bytes())?;

        for sec in &self.sections {
            match sec {
                Section::Body(doc) => {
                    body_bytes.push(0); // Kind 0
                    doc.to_writer(&mut body_bytes)
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                }
                Section::Sequence {
                    identifier,
                    documents,
                } => {
                    body_bytes.push(1); // Kind 1
                    let mut sec_buf = Vec::new();
                    sec_buf.write_all(identifier.as_bytes())?;
                    sec_buf.push(0); // null terminator
                    for doc in documents {
                        doc.to_writer(&mut sec_buf)
                            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    }
                    let total_sec_size = (4 + sec_buf.len()) as i32;
                    body_bytes.write_all(&total_sec_size.to_le_bytes())?;
                    body_bytes.write_all(&sec_buf)?;
                }
            }
        }

        let mut output = Vec::with_capacity(HEADER_LEN + body_bytes.len());
        let final_header = MsgHeader::new(
            self.header.request_id,
            self.header.response_to,
            OpCode::OpMsg,
            body_bytes.len(),
        );
        final_header.encode(&mut output)?;
        output.write_all(&body_bytes)?;

        Ok(output)
    }
}
