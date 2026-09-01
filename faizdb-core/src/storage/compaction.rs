//! Background compaction — merges SSTables to reclaim space and improve reads.
//!
//! Compaction is essential for LSM-tree based storage engines because:
//! 1. Reads get slower as more SSTables accumulate (must check each one)
//! 2. Deleted data (tombstones) still takes up space until compacted
//! 3. Updated data exists in multiple SSTables until merged
//!
//! FaizDB uses **Leveled Compaction** (like RocksDB), which provides:
//! - Bounded space amplification (~1.1x)
//! - Good read performance (limited SSTables per level)
//! - Predictable I/O patterns

use std::path::{Path, PathBuf};

use crate::error::FaizResult;
use crate::storage::memtable::MemEntry;
use crate::storage::sstable::{SSTableReader, SSTableWriter};

/// Compaction configuration
#[derive(Debug, Clone)]
pub struct CompactionConfig {
    /// Maximum number of SSTables at Level 0 before triggering compaction
    pub level0_trigger: usize,
    /// Size multiplier between levels (default: 10)
    pub level_multiplier: usize,
    /// Maximum number of levels (default: 7)
    pub max_levels: usize,
    /// Target SSTable size at Level 1 (default: 64MB)
    pub target_file_size: usize,
}

impl Default for CompactionConfig {
    fn default() -> Self {
        Self {
            level0_trigger: 4,
            level_multiplier: 10,
            max_levels: 7,
            target_file_size: 64 * 1024 * 1024, // 64MB
        }
    }
}

/// Merge multiple sorted SSTable iterators into a single sorted stream.
///
/// This is the core of compaction — it takes entries from multiple SSTables
/// and produces a single sorted output where:
/// - For duplicate keys, only the newest entry is kept
/// - Tombstones are removed (if all levels agree the key is deleted)
pub fn merge_sstables(
    input_paths: &[PathBuf],
    output_path: &Path,
    drop_tombstones: bool,
) -> FaizResult<PathBuf> {
    // Collect all entries from all input SSTables
    let mut all_entries: Vec<(Vec<u8>, MemEntry, usize)> = Vec::new();

    for (table_idx, path) in input_paths.iter().enumerate() {
        let reader = SSTableReader::open(path)?;
        for entry_result in reader.iter()? {
            let (key, entry) = entry_result?;
            all_entries.push((key, entry, table_idx));
        }
    }

    // Sort by key, then by table index (newer tables have higher index)
    all_entries.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| b.2.cmp(&a.2)) // Same key: higher index (newer) first
    });

    // Deduplicate: for same key, keep only the first (newest) entry
    let mut deduped: Vec<(Vec<u8>, MemEntry)> = Vec::new();
    let mut last_key: Option<Vec<u8>> = None;

    for (key, entry, _) in all_entries {
        if last_key.as_ref() == Some(&key) {
            continue; // Skip older version of same key
        }

        // Optionally drop tombstones
        if drop_tombstones && entry.is_tombstone() {
            last_key = Some(key);
            continue;
        }

        last_key = Some(key.clone());
        deduped.push((key, entry));
    }

    // Write merged output
    let mut writer = SSTableWriter::new(output_path, deduped.len())?;
    for (key, entry) in &deduped {
        writer.write_entry(key, entry)?;
    }

    writer.finish()
}

/// Compaction statistics
#[derive(Debug, Default)]
pub struct CompactionStats {
    /// Number of input SSTables merged
    pub input_tables: usize,
    /// Number of output SSTables produced
    pub output_tables: usize,
    /// Total input bytes
    pub input_bytes: u64,
    /// Total output bytes
    pub output_bytes: u64,
    /// Number of entries removed (duplicates + tombstones)
    pub entries_removed: u64,
    /// Time taken in milliseconds
    pub duration_ms: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::sstable::SSTableWriter;

    #[test]
    fn test_merge_sstables() {
        let dir = tempfile::tempdir().unwrap();

        // Create SSTable 1 (older)
        let path1 = dir.path().join("sst_001.sst");
        {
            let mut writer = SSTableWriter::new(&path1, 3).unwrap();
            writer
                .write_entry(b"alpha", &MemEntry::Value(b"old_a".to_vec()))
                .unwrap();
            writer
                .write_entry(b"bravo", &MemEntry::Value(b"old_b".to_vec()))
                .unwrap();
            writer
                .write_entry(b"charlie", &MemEntry::Value(b"old_c".to_vec()))
                .unwrap();
            writer.finish().unwrap();
        }

        // Create SSTable 2 (newer — has updated alpha and deleted charlie)
        let path2 = dir.path().join("sst_002.sst");
        {
            let mut writer = SSTableWriter::new(&path2, 2).unwrap();
            writer
                .write_entry(b"alpha", &MemEntry::Value(b"new_a".to_vec()))
                .unwrap();
            writer.write_entry(b"charlie", &MemEntry::Tombstone).unwrap();
            writer.finish().unwrap();
        }

        // Merge (keeping tombstones)
        let output_path = dir.path().join("merged.sst");
        merge_sstables(
            &[path1.clone(), path2.clone()],
            &output_path,
            false, // keep tombstones
        )
        .unwrap();

        // Verify merged output
        let reader = SSTableReader::open(&output_path).unwrap();

        // Alpha should have the new value (from SSTable 2)
        let entry = reader.get(b"alpha").unwrap().unwrap();
        assert_eq!(entry.as_value().unwrap(), b"new_a");

        // Bravo should still exist (only in SSTable 1)
        let entry = reader.get(b"bravo").unwrap().unwrap();
        assert_eq!(entry.as_value().unwrap(), b"old_b");

        // Charlie should be a tombstone
        let entry = reader.get(b"charlie").unwrap().unwrap();
        assert!(entry.is_tombstone());
    }

    #[test]
    fn test_merge_with_tombstone_removal() {
        let dir = tempfile::tempdir().unwrap();

        let path1 = dir.path().join("sst_001.sst");
        {
            let mut writer = SSTableWriter::new(&path1, 2).unwrap();
            writer
                .write_entry(b"key1", &MemEntry::Value(b"val1".to_vec()))
                .unwrap();
            writer
                .write_entry(b"key2", &MemEntry::Value(b"val2".to_vec()))
                .unwrap();
            writer.finish().unwrap();
        }

        let path2 = dir.path().join("sst_002.sst");
        {
            let mut writer = SSTableWriter::new(&path2, 1).unwrap();
            writer.write_entry(b"key1", &MemEntry::Tombstone).unwrap();
            writer.finish().unwrap();
        }

        let output_path = dir.path().join("merged.sst");
        merge_sstables(
            &[path1, path2],
            &output_path,
            true, // drop tombstones
        )
        .unwrap();

        let reader = SSTableReader::open(&output_path).unwrap();

        // key1 should be completely gone (tombstone dropped)
        assert!(reader.get(b"key1").unwrap().is_none());

        // key2 should still exist
        let entry = reader.get(b"key2").unwrap().unwrap();
        assert_eq!(entry.as_value().unwrap(), b"val2");
    }
}
