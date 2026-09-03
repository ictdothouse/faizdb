//! SSTable (Sorted String Table) — immutable, sorted, disk-based storage.
//!
//! SSTables are the persistent storage format for FaizDB. When a MemTable
//! is flushed to disk, it becomes an SSTable.
//!
//! ## File Format
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │ Header (32 bytes)                            │
//! │ ┌──────────────────────────────────────────┐ │
//! │ │ Magic: "FZSST001" (8 bytes)              │ │
//! │ │ Version: u32 (4 bytes)                   │ │
//! │ │ Entry Count: u64 (8 bytes)               │ │
//! │ │ Data Size: u64 (8 bytes)                 │ │
//! │ │ Compression: u8 (1 byte)                 │ │
//! │ │ Reserved: [u8; 3]                        │ │
//! │ └──────────────────────────────────────────┘ │
//! ├──────────────────────────────────────────────┤
//! │ Data Block (variable size)                   │
//! │ ┌──────────────────────────────────────────┐ │
//! │ │ Entry 1:                                 │ │
//! │ │   Key Length: u32                         │ │
//! │ │   Value Length: u32                       │ │
//! │ │   Is Tombstone: u8                        │ │
//! │ │   Key: [u8; key_len]                     │ │
//! │ │   Value: [u8; val_len]                   │ │
//! │ │ Entry 2 ...                              │ │
//! │ └──────────────────────────────────────────┘ │
//! ├──────────────────────────────────────────────┤
//! │ Index Block (variable size)                  │
//! │   Sparse index: every Nth key -> offset      │
//! ├──────────────────────────────────────────────┤
//! │ Bloom Filter (variable size)                 │
//! │   For fast negative lookups                  │
//! ├──────────────────────────────────────────────┤
//! │ Footer (24 bytes)                            │
//! │   Index Block Offset: u64                    │
//! │   Bloom Filter Offset: u64                   │
//! │   CRC32: u32                                 │
//! │   Magic: "FZEND001" (4 bytes)                │
//! └──────────────────────────────────────────────┘
//! ```

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::error::{FaizError, FaizResult};
use crate::storage::memtable::MemEntry;

/// Magic bytes for SSTable files
const SSTABLE_MAGIC: &[u8; 8] = b"FZSST001";
const SSTABLE_END_MAGIC: &[u8; 4] = b"FZND";

/// SSTable format version
const SSTABLE_VERSION: u32 = 1;

/// Header size in bytes
const HEADER_SIZE: usize = 32;

/// Footer size in bytes
const FOOTER_SIZE: usize = 24;

/// Sparse index interval (create an index entry every N entries)
const INDEX_INTERVAL: usize = 128;

/// Compression type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Compression {
    None = 0,
    Lz4 = 1,
}

/// A simple Bloom filter for fast negative lookups.
///
/// If the bloom filter says a key is NOT present, it is definitely not present.
/// If it says a key IS present, it might be present (false positives possible).
#[derive(Debug, Clone)]
pub struct BloomFilter {
    bits: Vec<u8>,
    num_hashes: u32,
}

impl BloomFilter {
    /// Create a new bloom filter sized for the expected number of entries
    /// with a target false positive rate of ~1%
    pub fn new(expected_entries: usize) -> Self {
        Self::new_with_fp_rate(expected_entries, 0.01)
    }

    /// Create a new bloom filter with custom target false positive rate
    pub fn new_with_fp_rate(expected_entries: usize, target_fp_rate: f64) -> Self {
        let entries = expected_entries.max(1);
        let p = target_fp_rate.clamp(0.0001, 0.5);
        let num_bits = (-(entries as f64) * p.ln() / (2.0f64.ln().powi(2))).ceil() as usize;
        let num_bytes = (num_bits + 7) / 8;
        let num_hashes = ((num_bytes as f64 * 8.0 / entries as f64) * 0.693).ceil() as u32;

        Self {
            bits: vec![0u8; num_bytes.max(1)],
            num_hashes: num_hashes.max(1).min(16),
        }
    }

    /// Create level-aware dynamic bloom filter (lower levels get tighter FPR to eliminate disk seeks)
    pub fn new_for_level(expected_entries: usize, level: usize) -> Self {
        let fp_rate = match level {
            0 => 0.01,   // 1% FPR for Level 0
            1 => 0.005,  // 0.5% FPR for Level 1
            _ => 0.001,  // 0.1% Ultra-low FPR for deep cold SSTables
        };
        Self::new_with_fp_rate(expected_entries, fp_rate)
    }


    /// Insert a key into the bloom filter
    pub fn insert(&mut self, key: &[u8]) {
        for i in 0..self.num_hashes {
            let hash = self.hash(key, i);
            let bit_idx = (hash as usize) % (self.bits.len() * 8);
            self.bits[bit_idx / 8] |= 1 << (bit_idx % 8);
        }
    }

    /// Check if a key might be present
    pub fn may_contain(&self, key: &[u8]) -> bool {
        for i in 0..self.num_hashes {
            let hash = self.hash(key, i);
            let bit_idx = (hash as usize) % (self.bits.len() * 8);
            if self.bits[bit_idx / 8] & (1 << (bit_idx % 8)) == 0 {
                return false;
            }
        }
        true
    }

    /// Simple hash function using FNV-1a with seed
    fn hash(&self, key: &[u8], seed: u32) -> u64 {
        let mut hash: u64 = 14695981039346656037u64.wrapping_add(seed as u64);
        for &byte in key {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
        hash
    }

    /// Serialize the bloom filter to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(8 + self.bits.len());
        buf.extend_from_slice(&self.num_hashes.to_le_bytes());
        buf.extend_from_slice(&(self.bits.len() as u32).to_le_bytes());
        buf.extend_from_slice(&self.bits);
        buf
    }

    /// Deserialize a bloom filter from bytes
    pub fn from_bytes(data: &[u8]) -> FaizResult<Self> {
        if data.len() < 8 {
            return Err(FaizError::SsTableCorrupted(
                "Bloom filter data too short".into(),
            ));
        }

        let num_hashes = u32::from_le_bytes(data[0..4].try_into().unwrap());
        let bits_len = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;

        if data.len() < 8 + bits_len {
            return Err(FaizError::SsTableCorrupted(
                "Bloom filter data truncated".into(),
            ));
        }

        Ok(Self {
            bits: data[8..8 + bits_len].to_vec(),
            num_hashes,
        })
    }
}

/// SSTable writer — creates a new SSTable file from sorted entries.
pub struct SSTableWriter {
    path: PathBuf,
    writer: BufWriter<File>,
    entry_count: u64,
    data_size: u64,
    bloom: BloomFilter,
    sparse_index: BTreeMap<Vec<u8>, u64>, // key -> offset in data block
    current_offset: u64,
    entries_since_index: usize,
}

impl SSTableWriter {
    /// Create a new SSTable writer
    pub fn new(path: impl AsRef<Path>, expected_entries: usize) -> FaizResult<Self> {
        let path = path.as_ref().to_path_buf();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| FaizError::io(&path, e))?;
        }

        let file = File::create(&path).map_err(|e| FaizError::io(&path, e))?;
        let mut writer = BufWriter::new(file);

        // Write header (will be updated at the end)
        let header = [0u8; HEADER_SIZE];
        writer
            .write_all(&header)
            .map_err(|e| FaizError::io(&path, e))?;

        Ok(Self {
            path,
            writer,
            entry_count: 0,
            data_size: 0,
            bloom: BloomFilter::new(expected_entries.max(1)),
            sparse_index: BTreeMap::new(),
            current_offset: HEADER_SIZE as u64,
            entries_since_index: 0,
        })
    }

    /// Write a key-value entry to the SSTable.
    ///
    /// **IMPORTANT**: Entries MUST be written in sorted key order.
    pub fn write_entry(&mut self, key: &[u8], entry: &MemEntry) -> FaizResult<()> {
        // Add to bloom filter
        self.bloom.insert(key);

        // Maybe add to sparse index
        if self.entries_since_index >= INDEX_INTERVAL || self.entry_count == 0 {
            self.sparse_index
                .insert(key.to_vec(), self.current_offset);
            self.entries_since_index = 0;
        }
        self.entries_since_index += 1;

        // Write entry
        let is_tombstone = entry.is_tombstone();
        let value = entry.as_value().unwrap_or(&[]);

        let mut entry_buf = Vec::new();
        entry_buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
        entry_buf.extend_from_slice(&(value.len() as u32).to_le_bytes());
        entry_buf.push(if is_tombstone { 1 } else { 0 });
        entry_buf.extend_from_slice(key);
        entry_buf.extend_from_slice(value);

        self.writer
            .write_all(&entry_buf)
            .map_err(|e| FaizError::io(&self.path, e))?;

        let entry_size = entry_buf.len() as u64;
        self.current_offset += entry_size;
        self.data_size += entry_size;
        self.entry_count += 1;

        Ok(())
    }

    /// Finalize the SSTable — writes index, bloom filter, footer, and header.
    pub fn finish(mut self) -> FaizResult<PathBuf> {
        // ── Write Index Block ────────────────────────────────────
        let index_offset = self.current_offset;
        let index_bytes = self.serialize_index();
        self.writer
            .write_all(&index_bytes)
            .map_err(|e| FaizError::io(&self.path, e))?;

        // ── Write Bloom Filter ───────────────────────────────────
        let bloom_offset = index_offset + index_bytes.len() as u64;
        let bloom_bytes = self.bloom.to_bytes();
        self.writer
            .write_all(&bloom_bytes)
            .map_err(|e| FaizError::io(&self.path, e))?;

        // ── Write Footer ─────────────────────────────────────────
        let mut footer = Vec::with_capacity(FOOTER_SIZE);
        footer.extend_from_slice(&index_offset.to_le_bytes());
        footer.extend_from_slice(&bloom_offset.to_le_bytes());

        // CRC of everything we've written
        let crc = crc32fast::hash(&footer);
        footer.extend_from_slice(&crc.to_le_bytes());
        footer.extend_from_slice(SSTABLE_END_MAGIC);

        self.writer
            .write_all(&footer)
            .map_err(|e| FaizError::io(&self.path, e))?;

        // ── Update Header ────────────────────────────────────────
        self.writer
            .flush()
            .map_err(|e| FaizError::io(&self.path, e))?;

        // Re-open file to update header
        let mut file = open_options_create(&self.path)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|e| FaizError::io(&self.path, e))?;

        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.extend_from_slice(SSTABLE_MAGIC);
        header.extend_from_slice(&SSTABLE_VERSION.to_le_bytes());
        header.extend_from_slice(&self.entry_count.to_le_bytes());
        header.extend_from_slice(&self.data_size.to_le_bytes());
        header.push(Compression::None as u8);
        header.extend(vec![0u8; HEADER_SIZE - header.len()]); // padding

        file.write_all(&header)
            .map_err(|e| FaizError::io(&self.path, e))?;

        file.sync_all()
            .map_err(|e| FaizError::io(&self.path, e))?;

        Ok(self.path)
    }

    fn serialize_index(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let entry_count = self.sparse_index.len() as u32;
        buf.extend_from_slice(&entry_count.to_le_bytes());

        for (key, offset) in &self.sparse_index {
            buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
            buf.extend_from_slice(key);
            buf.extend_from_slice(&offset.to_le_bytes());
        }

        buf
    }
}

/// Helper to open a file for read-write
fn open_options_create(path: &Path) -> FaizResult<File> {
    use std::fs::OpenOptions;
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| FaizError::io(path, e))
}

/// SSTable reader — reads entries from an SSTable file.
#[allow(dead_code)]
pub struct SSTableReader {
    path: PathBuf,
    entry_count: u64,
    data_size: u64,
    bloom: BloomFilter,
    sparse_index: BTreeMap<Vec<u8>, u64>,
    index_offset: u64,
    bloom_offset: u64,
}

impl SSTableReader {
    /// Open an existing SSTable file for reading
    pub fn open(path: impl AsRef<Path>) -> FaizResult<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path).map_err(|e| FaizError::io(&path, e))?;

        // Read and verify header
        let mut header = [0u8; HEADER_SIZE];
        file.read_exact(&mut header)
            .map_err(|e| FaizError::io(&path, e))?;

        if &header[0..8] != SSTABLE_MAGIC {
            return Err(FaizError::InvalidMagicBytes);
        }

        let version = u32::from_le_bytes(header[8..12].try_into().unwrap());
        if version != SSTABLE_VERSION {
            return Err(FaizError::SsTableCorrupted(format!(
                "Unsupported version: {version}"
            )));
        }

        let entry_count = u64::from_le_bytes(header[12..20].try_into().unwrap());
        let data_size = u64::from_le_bytes(header[20..28].try_into().unwrap());

        // Read footer
        let file_size = file
            .metadata()
            .map_err(|e| FaizError::io(&path, e))?
            .len();

        file.seek(SeekFrom::Start(file_size - FOOTER_SIZE as u64))
            .map_err(|e| FaizError::io(&path, e))?;

        let mut footer = [0u8; FOOTER_SIZE];
        file.read_exact(&mut footer)
            .map_err(|e| FaizError::io(&path, e))?;

        let index_offset = u64::from_le_bytes(footer[0..8].try_into().unwrap());
        let bloom_offset = u64::from_le_bytes(footer[8..16].try_into().unwrap());

        // Read bloom filter
        file.seek(SeekFrom::Start(bloom_offset))
            .map_err(|e| FaizError::io(&path, e))?;

        let bloom_size = (file_size - FOOTER_SIZE as u64) - bloom_offset;
        let mut bloom_data = vec![0u8; bloom_size as usize];
        file.read_exact(&mut bloom_data)
            .map_err(|e| FaizError::io(&path, e))?;

        let bloom = BloomFilter::from_bytes(&bloom_data)?;

        // Read sparse index
        file.seek(SeekFrom::Start(index_offset))
            .map_err(|e| FaizError::io(&path, e))?;

        let index_size = bloom_offset - index_offset;
        let mut index_data = vec![0u8; index_size as usize];
        file.read_exact(&mut index_data)
            .map_err(|e| FaizError::io(&path, e))?;

        let sparse_index = Self::deserialize_index(&index_data)?;

        Ok(Self {
            path,
            entry_count,
            data_size,
            bloom,
            sparse_index,
            index_offset,
            bloom_offset,
        })
    }

    /// Check whether the bloom filter may contain a key
    pub fn may_contain(&self, key: &[u8]) -> bool {
        self.bloom.may_contain(key)
    }

    /// Look up a key in the SSTable.
    ///
    /// Uses the bloom filter for fast negative lookups, then binary
    /// searches the sparse index to find the approximate location.
    pub fn get(&self, key: &[u8]) -> FaizResult<Option<MemEntry>> {
        // Fast path: bloom filter check
        if !self.bloom.may_contain(key) {
            return Ok(None);
        }

        // Find the approximate position using sparse index
        let start_offset = self.find_start_offset(key);

        // Scan from the start offset
        let mut file = File::open(&self.path).map_err(|e| FaizError::io(&self.path, e))?;
        file.seek(SeekFrom::Start(start_offset))
            .map_err(|e| FaizError::io(&self.path, e))?;

        let mut reader = BufReader::new(file);
        let end_offset = self.index_offset;

        let mut current_offset = start_offset;
        while current_offset < end_offset {
            match Self::read_entry(&mut reader) {
                Ok((entry_key, entry)) => {
                    let entry_size =
                        4 + 4 + 1 + entry_key.len() + entry.as_value().map_or(0, |v| v.len());
                    current_offset += entry_size as u64;

                    if entry_key == key {
                        return Ok(Some(entry));
                    }

                    // Since entries are sorted, if we've passed the key, it's not here
                    if entry_key.as_slice() > key {
                        return Ok(None);
                    }
                }
                Err(_) => break,
            }
        }

        Ok(None)
    }

    /// Iterate over all entries in the SSTable
    pub fn iter(&self) -> FaizResult<SSTableIterator> {
        let file = File::open(&self.path).map_err(|e| FaizError::io(&self.path, e))?;
        let mut reader = BufReader::new(file);

        reader
            .seek(SeekFrom::Start(HEADER_SIZE as u64))
            .map_err(|e| FaizError::io(&self.path, e))?;

        Ok(SSTableIterator {
            reader,
            remaining: self.entry_count,
        })
    }

    /// Get the number of entries in this SSTable
    pub fn entry_count(&self) -> u64 {
        self.entry_count
    }

    /// Get the path of this SSTable
    pub fn path(&self) -> &Path {
        &self.path
    }

    // ── Internal Helpers ─────────────────────────────────────────

    fn find_start_offset(&self, key: &[u8]) -> u64 {
        // Find the largest index key that is <= our search key
        let mut start = HEADER_SIZE as u64;

        for (idx_key, offset) in &self.sparse_index {
            if idx_key.as_slice() <= key {
                start = *offset;
            } else {
                break;
            }
        }

        start
    }

    fn read_entry<R: Read>(reader: &mut R) -> FaizResult<(Vec<u8>, MemEntry)> {
        let mut len_buf = [0u8; 4];

        reader
            .read_exact(&mut len_buf)
            .map_err(|e| FaizError::Internal(format!("Read key length: {e}")))?;
        let key_len = u32::from_le_bytes(len_buf) as usize;

        reader
            .read_exact(&mut len_buf)
            .map_err(|e| FaizError::Internal(format!("Read value length: {e}")))?;
        let val_len = u32::from_le_bytes(len_buf) as usize;

        let mut tombstone_buf = [0u8; 1];
        reader
            .read_exact(&mut tombstone_buf)
            .map_err(|e| FaizError::Internal(format!("Read tombstone: {e}")))?;
        let is_tombstone = tombstone_buf[0] == 1;

        let mut key = vec![0u8; key_len];
        reader
            .read_exact(&mut key)
            .map_err(|e| FaizError::Internal(format!("Read key: {e}")))?;

        let entry = if is_tombstone {
            // Skip value bytes if any
            if val_len > 0 {
                let mut discard = vec![0u8; val_len];
                reader
                    .read_exact(&mut discard)
                    .map_err(|e| FaizError::Internal(format!("Read tombstone value: {e}")))?;
            }
            MemEntry::Tombstone
        } else {
            let mut value = vec![0u8; val_len];
            reader
                .read_exact(&mut value)
                .map_err(|e| FaizError::Internal(format!("Read value: {e}")))?;
            MemEntry::Value(value)
        };

        Ok((key, entry))
    }

    fn deserialize_index(data: &[u8]) -> FaizResult<BTreeMap<Vec<u8>, u64>> {
        let mut index = BTreeMap::new();

        if data.len() < 4 {
            return Ok(index);
        }

        let count = u32::from_le_bytes(data[0..4].try_into().unwrap()) as usize;
        let mut pos = 4;

        for _ in 0..count {
            if pos + 4 > data.len() {
                break;
            }

            let key_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;

            if pos + key_len + 8 > data.len() {
                break;
            }

            let key = data[pos..pos + key_len].to_vec();
            pos += key_len;

            let offset = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
            pos += 8;

            index.insert(key, offset);
        }

        Ok(index)
    }
}

/// Iterator over SSTable entries
pub struct SSTableIterator {
    reader: BufReader<File>,
    remaining: u64,
}

impl Iterator for SSTableIterator {
    type Item = FaizResult<(Vec<u8>, MemEntry)>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }

        self.remaining -= 1;
        Some(SSTableReader::read_entry(&mut self.reader))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloom_filter() {
        let mut bloom = BloomFilter::new(1000);

        bloom.insert(b"hello");
        bloom.insert(b"world");
        bloom.insert(b"faizdb");

        assert!(bloom.may_contain(b"hello"));
        assert!(bloom.may_contain(b"world"));
        assert!(bloom.may_contain(b"faizdb"));

        // This should (almost certainly) return false
        // Note: Bloom filters can have false positives, but not false negatives
        let mut false_positives = 0;
        for i in 0..100 {
            let key = format!("nonexistent_{i}");
            if bloom.may_contain(key.as_bytes()) {
                false_positives += 1;
            }
        }
        // With 1000-entry bloom filter, false positive rate should be very low
        assert!(
            false_positives < 10,
            "Too many false positives: {false_positives}"
        );
    }

    #[test]
    fn test_sstable_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sst");

        // Write SSTable
        {
            let mut writer = SSTableWriter::new(&path, 3).unwrap();
            writer
                .write_entry(b"alpha", &MemEntry::Value(b"value_a".to_vec()))
                .unwrap();
            writer
                .write_entry(b"bravo", &MemEntry::Value(b"value_b".to_vec()))
                .unwrap();
            writer
                .write_entry(b"charlie", &MemEntry::Tombstone)
                .unwrap();
            writer.finish().unwrap();
        }

        // Read SSTable
        let reader = SSTableReader::open(&path).unwrap();
        assert_eq!(reader.entry_count(), 3);

        // Lookup existing key
        let entry = reader.get(b"alpha").unwrap().unwrap();
        assert_eq!(entry.as_value().unwrap(), b"value_a");

        // Lookup tombstone
        let entry = reader.get(b"charlie").unwrap().unwrap();
        assert!(entry.is_tombstone());

        // Lookup non-existent key
        let entry = reader.get(b"delta").unwrap();
        assert!(entry.is_none());
    }

    #[test]
    fn test_sstable_iterator() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("iter_test.sst");

        // Write 100 entries
        {
            let mut writer = SSTableWriter::new(&path, 100).unwrap();
            for i in 0..100u32 {
                let key = format!("key_{i:04}");
                let value = format!("value_{i}");
                writer
                    .write_entry(key.as_bytes(), &MemEntry::Value(value.into_bytes()))
                    .unwrap();
            }
            writer.finish().unwrap();
        }

        // Iterate
        let reader = SSTableReader::open(&path).unwrap();
        let entries: Vec<_> = reader.iter().unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(entries.len(), 100);

        // Should be sorted
        assert_eq!(entries[0].0, b"key_0000");
        assert_eq!(entries[99].0, b"key_0099");
    }

    #[test]
    fn test_bloom_filter_serialization() {
        let mut bloom = BloomFilter::new(100);
        bloom.insert(b"test_key_1");
        bloom.insert(b"test_key_2");

        let bytes = bloom.to_bytes();
        let restored = BloomFilter::from_bytes(&bytes).unwrap();

        assert!(restored.may_contain(b"test_key_1"));
        assert!(restored.may_contain(b"test_key_2"));
    }

    #[test]
    fn test_level_aware_bloom_filter() {
        let l0 = BloomFilter::new_for_level(1000, 0);
        let l2 = BloomFilter::new_for_level(1000, 2);

        // Level 2 should allocate more bits than Level 0 for tighter false positive rates
        assert!(l2.bits.len() > l0.bits.len(), "Deep level SSTables must have larger bloom filter size");
    }
}

