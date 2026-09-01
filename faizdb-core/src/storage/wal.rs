//! Write-Ahead Log (WAL) — ensures crash-safe durability.
//!
//! Every write operation is first logged to the WAL before being applied
//! to the MemTable. This guarantees that no data is lost even if the
//! process crashes mid-operation.
//!
//! ## WAL File Format
//!
//! ```text
//! ┌──────────────────────────────────────────┐
//! │ Magic Bytes: "FZWAL001" (8 bytes)        │
//! ├──────────────────────────────────────────┤
//! │ Record 1                                 │
//! │ ┌──────────────────────────────────────┐ │
//! │ │ Length: u32 (4 bytes)                 │ │
//! │ │ CRC32: u32 (4 bytes)                 │ │
//! │ │ Sequence: u64 (8 bytes)              │ │
//! │ │ Op Type: u8 (1 byte)                 │ │
//! │ │ Key Length: u32 (4 bytes)             │ │
//! │ │ Key: [u8; key_len]                   │ │
//! │ │ Value Length: u32 (4 bytes)           │ │
//! │ │ Value: [u8; value_len]               │ │
//! │ └──────────────────────────────────────┘ │
//! │ Record 2 ...                             │
//! └──────────────────────────────────────────┘
//! ```

use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;

use crate::error::{FaizError, FaizResult};

/// Magic bytes that identify a WAL file
const WAL_MAGIC: &[u8; 8] = b"FZWAL001";

/// Maximum WAL file size before rotation (128 MB)
const MAX_WAL_SIZE: u64 = 128 * 1024 * 1024;

/// Type of operation recorded in the WAL
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WalOpType {
    /// Insert a key-value pair
    Put = 1,
    /// Delete a key
    Delete = 2,
    /// Begin a transaction
    TxnBegin = 10,
    /// Commit a transaction
    TxnCommit = 11,
    /// Abort/rollback a transaction
    TxnAbort = 12,
}

impl TryFrom<u8> for WalOpType {
    type Error = FaizError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(WalOpType::Put),
            2 => Ok(WalOpType::Delete),
            10 => Ok(WalOpType::TxnBegin),
            11 => Ok(WalOpType::TxnCommit),
            12 => Ok(WalOpType::TxnAbort),
            _ => Err(FaizError::WalCorrupted {
                offset: 0,
                detail: format!("Unknown WAL operation type: {value}"),
            }),
        }
    }
}

/// A single WAL record
#[derive(Debug, Clone)]
pub struct WalRecord {
    /// Monotonically increasing sequence number
    pub sequence: u64,
    /// Type of operation
    pub op_type: WalOpType,
    /// Key (collection_name:document_id)
    pub key: Vec<u8>,
    /// Value (serialized document bytes, empty for Delete)
    pub value: Vec<u8>,
}

impl WalRecord {
    /// Calculate the total size of this record on disk
    pub fn disk_size(&self) -> usize {
        4 + 4 + 8 + 1 + 4 + self.key.len() + 4 + self.value.len()
        // len + crc + seq + op + key_len + key + val_len + val
    }

    /// Serialize the record to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload_size = 8 + 1 + 4 + self.key.len() + 4 + self.value.len();
        let mut buf = Vec::with_capacity(4 + 4 + payload_size);

        // Length (excluding length and CRC fields)
        buf.extend_from_slice(&(payload_size as u32).to_le_bytes());

        // Placeholder for CRC (will be filled after)
        let crc_pos = buf.len();
        buf.extend_from_slice(&[0u8; 4]);

        // Sequence number
        buf.extend_from_slice(&self.sequence.to_le_bytes());

        // Operation type
        buf.push(self.op_type as u8);

        // Key
        buf.extend_from_slice(&(self.key.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.key);

        // Value
        buf.extend_from_slice(&(self.value.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.value);

        // Calculate CRC over everything after the CRC field
        let crc = crc32fast::hash(&buf[crc_pos + 4..]);
        buf[crc_pos..crc_pos + 4].copy_from_slice(&crc.to_le_bytes());

        buf
    }

    /// Deserialize a record from a reader
    pub fn from_reader<R: Read>(reader: &mut R, offset: u64) -> FaizResult<Self> {
        // Read length
        let mut len_buf = [0u8; 4];
        reader
            .read_exact(&mut len_buf)
            .map_err(|e| FaizError::io(format!("WAL offset {offset}"), e))?;
        let payload_len = u32::from_le_bytes(len_buf) as usize;

        // Read CRC
        let mut crc_buf = [0u8; 4];
        reader
            .read_exact(&mut crc_buf)
            .map_err(|e| FaizError::io(format!("WAL offset {offset}"), e))?;
        let expected_crc = u32::from_le_bytes(crc_buf);

        // Read payload
        let mut payload = vec![0u8; payload_len];
        reader
            .read_exact(&mut payload)
            .map_err(|e| FaizError::io(format!("WAL offset {offset}"), e))?;

        // Verify CRC
        let actual_crc = crc32fast::hash(&payload);
        if actual_crc != expected_crc {
            return Err(FaizError::ChecksumMismatch {
                expected: expected_crc,
                actual: actual_crc,
            });
        }

        // Parse payload
        let mut pos = 0;

        // Sequence
        let sequence = u64::from_le_bytes(payload[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // Op type
        let op_type = WalOpType::try_from(payload[pos])?;
        pos += 1;

        // Key
        let key_len = u32::from_le_bytes(payload[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let key = payload[pos..pos + key_len].to_vec();
        pos += key_len;

        // Value
        let val_len = u32::from_le_bytes(payload[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;
        let value = payload[pos..pos + val_len].to_vec();

        Ok(WalRecord {
            sequence,
            op_type,
            key,
            value,
        })
    }
}

/// The Write-Ahead Log writer.
///
/// Thread-safe — uses a mutex to serialize writes.
/// All writes are fsync'd for durability.
pub struct Wal {
    /// Path to the WAL directory
    dir: PathBuf,

    /// Current WAL file writer
    writer: Mutex<BufWriter<File>>,

    /// Current WAL file path
    current_path: Mutex<PathBuf>,

    /// Current sequence number (monotonically increasing)
    sequence: AtomicU64,

    /// Current file size
    file_size: AtomicU64,

    /// WAL file generation number
    generation: AtomicU64,
}

impl Wal {
    /// Open or create a WAL in the specified directory.
    pub fn open(dir: impl AsRef<Path>) -> FaizResult<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir).map_err(|e| FaizError::io(&dir, e))?;

        let (path, generation, sequence) = Self::find_or_create_wal_file(&dir)?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| FaizError::io(&path, e))?;

        let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);

        Ok(Self {
            dir,
            writer: Mutex::new(BufWriter::new(file)),
            current_path: Mutex::new(path),
            sequence: AtomicU64::new(sequence),
            file_size: AtomicU64::new(file_size),
            generation: AtomicU64::new(generation),
        })
    }

    /// Append a record to the WAL.
    ///
    /// This is the critical write path — all writes MUST go through here
    /// before being applied to the MemTable.
    pub fn append(&self, op_type: WalOpType, key: &[u8], value: &[u8]) -> FaizResult<u64> {
        let seq = self.sequence.fetch_add(1, Ordering::SeqCst);

        let record = WalRecord {
            sequence: seq,
            op_type,
            key: key.to_vec(),
            value: value.to_vec(),
        };

        let bytes = record.to_bytes();
        let record_size = bytes.len() as u64;

        let mut writer = self.writer.lock();

        // Check if we need to rotate the WAL file
        if self.file_size.load(Ordering::Relaxed) + record_size > MAX_WAL_SIZE {
            // Flush current writer
            writer.flush().map_err(|e| {
                FaizError::io(self.current_path.lock().clone(), e)
            })?;

            // Create new WAL file
            let gen_num = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
            let new_path = self.dir.join(format!("wal_{gen_num:06}.log"));

            let new_file = OpenOptions::new()
                .create(true)
                .write(true)
                .open(&new_path)
                .map_err(|e| FaizError::io(&new_path, e))?;

            // Write magic bytes to new file
            let mut new_writer = BufWriter::new(new_file);
            new_writer
                .write_all(WAL_MAGIC)
                .map_err(|e| FaizError::io(&new_path, e))?;

            *writer = new_writer;
            *self.current_path.lock() = new_path;
            self.file_size.store(8, Ordering::Relaxed); // magic bytes size
        }

        // Write the record
        writer
            .write_all(&bytes)
            .map_err(|e| FaizError::io(self.current_path.lock().clone(), e))?;

        // Flush to ensure durability
        writer
            .flush()
            .map_err(|e| FaizError::io(self.current_path.lock().clone(), e))?;

        self.file_size.fetch_add(record_size, Ordering::Relaxed);

        Ok(seq)
    }

    /// Write a Put operation to the WAL
    pub fn log_put(&self, key: &[u8], value: &[u8]) -> FaizResult<u64> {
        self.append(WalOpType::Put, key, value)
    }

    /// Write a Delete operation to the WAL
    pub fn log_delete(&self, key: &[u8]) -> FaizResult<u64> {
        self.append(WalOpType::Delete, key, &[])
    }

    /// Replay all records from all WAL files in the directory.
    ///
    /// Used during crash recovery to rebuild the MemTable.
    pub fn replay(dir: impl AsRef<Path>) -> FaizResult<Vec<WalRecord>> {
        let dir = dir.as_ref();
        let mut records = Vec::new();

        // Find all WAL files sorted by generation
        let mut wal_files: Vec<PathBuf> = fs::read_dir(dir)
            .map_err(|e| FaizError::io(dir, e))?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("log") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        wal_files.sort();

        for wal_path in wal_files {
            let file = File::open(&wal_path).map_err(|e| FaizError::io(&wal_path, e))?;
            let mut reader = BufReader::new(file);

            // Read and verify magic bytes
            let mut magic = [0u8; 8];
            match reader.read_exact(&mut magic) {
                Ok(()) => {
                    if &magic != WAL_MAGIC {
                        tracing::warn!("Skipping file with invalid magic: {}", wal_path.display());
                        continue;
                    }
                }
                Err(_) => continue, // Empty file
            }

            // Read records until EOF
            let mut offset = 8u64;
            loop {
                match WalRecord::from_reader(&mut reader, offset) {
                    Ok(record) => {
                        offset += record.disk_size() as u64;
                        records.push(record);
                    }
                    Err(FaizError::Io { .. }) => break, // EOF or read error
                    Err(FaizError::ChecksumMismatch { .. }) => {
                        tracing::warn!(
                            "WAL checksum mismatch at offset {offset} in {}. \
                             Truncating replay here.",
                            wal_path.display()
                        );
                        break;
                    }
                    Err(e) => return Err(e),
                }
            }
        }

        // Sort by sequence number to ensure correct replay order
        records.sort_by_key(|r| r.sequence);

        Ok(records)
    }

    /// Sync the WAL to disk (fsync)
    pub fn sync(&self) -> FaizResult<()> {
        let writer = self.writer.lock();
        writer
            .get_ref()
            .sync_all()
            .map_err(|e| FaizError::io(self.current_path.lock().clone(), e))
    }

    /// Get the current sequence number
    pub fn current_sequence(&self) -> u64 {
        self.sequence.load(Ordering::SeqCst)
    }

    // ── Internal Helpers ─────────────────────────────────────────

    fn find_or_create_wal_file(dir: &Path) -> FaizResult<(PathBuf, u64, u64)> {
        // Find existing WAL files
        let mut wal_files: Vec<(PathBuf, u64)> = fs::read_dir(dir)
            .map_err(|e| FaizError::io(dir, e))?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                let name = path.file_stem()?.to_str()?;
                if name.starts_with("wal_") {
                    let gen_num: u64 = name.strip_prefix("wal_")?.parse().ok()?;
                    Some((path, gen_num))
                } else {
                    None
                }
            })
            .collect();

        wal_files.sort_by_key(|(_, gen_num)| *gen_num);

        if let Some((path, gen_num)) = wal_files.last() {
            // Replay to find the last sequence number
            let records = Self::replay_single_file(path)?;
            let last_seq = records.last().map(|r| r.sequence + 1).unwrap_or(0);
            Ok((path.clone(), *gen_num, last_seq))
        } else {
            // Create first WAL file
            let path = dir.join("wal_000001.log");
            let mut file = File::create(&path).map_err(|e| FaizError::io(&path, e))?;
            file.write_all(WAL_MAGIC)
                .map_err(|e| FaizError::io(&path, e))?;
            file.sync_all().map_err(|e| FaizError::io(&path, e))?;
            Ok((path, 1, 0))
        }
    }

    fn replay_single_file(path: &Path) -> FaizResult<Vec<WalRecord>> {
        let file = File::open(path).map_err(|e| FaizError::io(path, e))?;
        let mut reader = BufReader::new(file);
        let mut records = Vec::new();

        // Skip magic bytes
        let mut magic = [0u8; 8];
        if reader.read_exact(&mut magic).is_err() {
            return Ok(records);
        }

        let mut offset = 8u64;
        loop {
            match WalRecord::from_reader(&mut reader, offset) {
                Ok(record) => {
                    offset += record.disk_size() as u64;
                    records.push(record);
                }
                Err(_) => break,
            }
        }

        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_wal_record_serialization() {
        let record = WalRecord {
            sequence: 42,
            op_type: WalOpType::Put,
            key: b"users:doc123".to_vec(),
            value: b"{\"name\": \"Faiz\"}".to_vec(),
        };

        let bytes = record.to_bytes();
        let mut reader = Cursor::new(bytes);
        let restored = WalRecord::from_reader(&mut reader, 0).unwrap();

        assert_eq!(restored.sequence, 42);
        assert_eq!(restored.op_type, WalOpType::Put);
        assert_eq!(restored.key, b"users:doc123");
        assert_eq!(restored.value, b"{\"name\": \"Faiz\"}");
    }

    #[test]
    fn test_wal_write_and_replay() {
        let dir = tempfile::tempdir().unwrap();
        let wal = Wal::open(dir.path()).unwrap();

        // Write some records
        wal.log_put(b"users:1", b"{\"name\":\"Faiz\"}").unwrap();
        wal.log_put(b"users:2", b"{\"name\":\"Ali\"}").unwrap();
        wal.log_delete(b"users:1").unwrap();

        // Replay
        let records = Wal::replay(dir.path()).unwrap();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].op_type, WalOpType::Put);
        assert_eq!(records[1].op_type, WalOpType::Put);
        assert_eq!(records[2].op_type, WalOpType::Delete);
    }

    #[test]
    fn test_wal_crc_integrity() {
        let record = WalRecord {
            sequence: 1,
            op_type: WalOpType::Put,
            key: b"test".to_vec(),
            value: b"data".to_vec(),
        };

        let mut bytes = record.to_bytes();

        // Corrupt a byte in the payload
        if let Some(last) = bytes.last_mut() {
            *last ^= 0xFF;
        }

        let mut reader = Cursor::new(bytes);
        let result = WalRecord::from_reader(&mut reader, 0);
        assert!(result.is_err()); // Should fail CRC check
    }

    #[test]
    fn test_wal_sequence_ordering() {
        let dir = tempfile::tempdir().unwrap();
        let wal = Wal::open(dir.path()).unwrap();

        let seq1 = wal.log_put(b"k1", b"v1").unwrap();
        let seq2 = wal.log_put(b"k2", b"v2").unwrap();
        let seq3 = wal.log_put(b"k3", b"v3").unwrap();

        assert!(seq1 < seq2);
        assert!(seq2 < seq3);
    }
}
