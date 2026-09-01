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
/// Uses a **streaming k-way merge** (min-heap over per-table iterators) so that
/// memory usage is bounded to `O(k)` (one entry per input table at any time)
/// regardless of total dataset size. This replaces the previous approach that
/// loaded every entry into RAM, which could OOM on large multi-GB compactions.
///
/// Merge semantics:
/// - For duplicate keys, the entry from the **highest-indexed** table wins (newest data).
/// - Tombstones are propagated unless `drop_tombstones` is `true`.
pub fn merge_sstables(
    input_paths: &[PathBuf],
    output_path: &Path,
    drop_tombstones: bool,
) -> FaizResult<PathBuf> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;

    // ── Heap entry ──────────────────────────────────────────────────────────
    // Ascending key order; equal keys resolved by descending table_idx (newest wins).
    #[derive(Eq, PartialEq)]
    struct HeapEntry {
        key: Vec<u8>,
        entry: MemEntry,
        table_idx: usize,
    }
    impl Ord for HeapEntry {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.key.cmp(&other.key)
                .then_with(|| other.table_idx.cmp(&self.table_idx))
        }
    }
    impl PartialOrd for HeapEntry {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> { Some(self.cmp(other)) }
    }

    // Open iterators for all input SSTables
    let mut readers: Vec<SSTableReader> = input_paths
        .iter()
        .map(SSTableReader::open)
        .collect::<FaizResult<_>>()?;

    let mut iterators: Vec<Box<dyn Iterator<Item = FaizResult<(Vec<u8>, MemEntry)>>>> = readers
        .iter_mut()
        .map(|r| -> Box<dyn Iterator<Item = FaizResult<(Vec<u8>, MemEntry)>>> {
            Box::new(r.iter().unwrap_or_else(|_| Box::new(std::iter::empty())))
        })
        .collect();

    // Seed the heap — one entry per iterator
    let mut heap: BinaryHeap<Reverse<HeapEntry>> = BinaryHeap::new();
    for (table_idx, iter) in iterators.iter_mut().enumerate() {
        if let Some(result) = iter.next() {
            let (key, entry) = result?;
            heap.push(Reverse(HeapEntry { key, entry, table_idx }));
        }
    }

    // Streaming merge: write directly as we pop from the heap
    let mut writer = SSTableWriter::new(output_path, 0)?;
    let mut last_written_key: Option<Vec<u8>> = None;

    while let Some(Reverse(HeapEntry { key, entry, table_idx })) = heap.pop() {
        // Advance the iterator that produced this entry
        if let Some(result) = iterators[table_idx].next() {
            let (next_key, next_entry) = result?;
            heap.push(Reverse(HeapEntry { key: next_key, entry: next_entry, table_idx }));
        }

        // Skip stale copies of the same key (already written the newest version)
        if last_written_key.as_ref() == Some(&key) {
            continue;
        }

        // Optionally drop tombstones (only safe during full-level compaction)
        if drop_tombstones && entry.is_tombstone() {
            last_written_key = Some(key);
            continue;
        }

        writer.write_entry(&key, &entry)?;
        last_written_key = Some(key);
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
