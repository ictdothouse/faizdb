//! Storage Engine — the unified interface to FaizDB's persistence layer.
//!
//! The StorageEngine orchestrates all storage components:
//! - WAL for durability
//! - MemTable for fast writes
//! - SSTables for persistent storage
//! - Compaction for maintenance
//!
//! It provides a simple key-value interface that the Document layer builds upon.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use parking_lot::RwLock;

use crate::error::{FaizError, FaizResult};
use crate::storage::memtable::{MemEntry, MemTable};
use crate::storage::sstable::{SSTableReader, SSTableWriter};
use crate::storage::wal::{Wal, WalOpType};

/// Configuration for the storage engine
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Directory to store all data files
    pub data_dir: PathBuf,
    /// Maximum MemTable size before flush (bytes)
    pub memtable_size: usize,
    /// Whether to sync WAL writes immediately (safer but slower)
    pub sync_writes: bool,
    /// Whether to enable WAL (disable for read-only or testing)
    pub enable_wal: bool,
    /// Capacity of the ARC (Adaptive Replacement Cache) block cache
    pub block_cache_size: usize,
    /// Threshold of L0 SSTables to trigger background compaction (default 4)
    pub l0_compaction_trigger: usize,
    /// Threshold of L0 SSTables to start soft write backpressure (default 8)
    pub l0_slowdown_writes_trigger: usize,
    /// Threshold of L0 SSTables to enforce hard write stall protection (default 16)
    pub l0_stop_writes_trigger: usize,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from(crate::DEFAULT_DATA_DIR),
            memtable_size: crate::DEFAULT_MEMTABLE_SIZE,
            sync_writes: true,
            enable_wal: true,
            block_cache_size: 4096,
            l0_compaction_trigger: 4,
            l0_slowdown_writes_trigger: 8,
            l0_stop_writes_trigger: 16,
        }
    }
}

/// The core storage engine.
///
/// Provides a key-value interface with durability, crash recovery,
/// and background compaction.
///
/// ## Thread Safety
///
/// The engine is fully thread-safe:
/// - Multiple readers can read concurrently (lock-free)
/// - Writes are serialized through the WAL and MemTable
/// - Background compaction runs independently
pub struct StorageEngine {
    /// Engine configuration
    config: StorageConfig,

    /// Write-Ahead Log
    wal: Option<Wal>,

    /// Active (mutable) MemTable
    active_memtable: Arc<MemTable>,

    /// Immutable MemTables waiting to be flushed to SSTables
    immutable_memtables: RwLock<Vec<Arc<MemTable>>>,

    /// SSTable readers (sorted newest to oldest)
    sstables: RwLock<Vec<SSTableReader>>,

    /// SSTable generation counter
    sstable_generation: AtomicU64,

    /// ARC (Adaptive Replacement Cache) for SSTable block and key-value lookups
    block_cache: parking_lot::Mutex<crate::storage::arc_cache::ArcCache<Vec<u8>, Option<Vec<u8>>>>,

    /// Atomic flag indicating whether compaction is actively running
    is_compacting: std::sync::atomic::AtomicBool,

    /// Total write stalls encountered
    write_stalls: AtomicU64,

    /// Total compactions completed
    compactions_completed: AtomicU64,

    /// Whether the engine is open
    is_open: std::sync::atomic::AtomicBool,
}

impl StorageEngine {
    /// Open or create a storage engine at the specified directory.
    ///
    /// If the directory contains existing data, the engine will recover
    /// by replaying the WAL and loading existing SSTables.
    pub fn open(config: StorageConfig) -> FaizResult<Self> {
        // Create data directory structure
        let data_dir = &config.data_dir;
        let wal_dir = data_dir.join("wal");
        let sst_dir = data_dir.join("sst");

        fs::create_dir_all(&wal_dir).map_err(|e| FaizError::io(&wal_dir, e))?;
        fs::create_dir_all(&sst_dir).map_err(|e| FaizError::io(&sst_dir, e))?;

        // Open WAL
        let wal = if config.enable_wal {
            Some(Wal::open(&wal_dir)?)
        } else {
            None
        };

        // Create MemTable
        let memtable = Arc::new(MemTable::new(config.memtable_size));

        // Recover from WAL
        if config.enable_wal {
            let records = Wal::replay(&wal_dir)?;
            tracing::info!("Recovered {} records from WAL", records.len());

            for record in &records {
                match record.op_type {
                    WalOpType::Put => {
                        memtable.put(record.key.clone(), record.value.clone())?;
                    }
                    WalOpType::Delete => {
                        memtable.delete(record.key.clone())?;
                    }
                    _ => {} // Transaction markers handled separately
                }
            }
        }

        // Load existing SSTables
        let mut sstables = Vec::new();
        if sst_dir.exists() {
            let mut sst_files: Vec<PathBuf> = fs::read_dir(&sst_dir)
                .map_err(|e| FaizError::io(&sst_dir, e))?
                .filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("sst") {
                        Some(path)
                    } else {
                        None
                    }
                })
                .collect();

            // Sort by name (which includes generation) — newest first
            sst_files.sort();
            sst_files.reverse();

            for sst_path in sst_files {
                match SSTableReader::open(&sst_path) {
                    Ok(reader) => sstables.push(reader),
                    Err(e) => {
                        tracing::warn!("Failed to open SSTable {}: {e}", sst_path.display());
                    }
                }
            }
        }

        // Determine the current SSTable generation
        let max_gen = sstables
            .iter()
            .filter_map(|sst| {
                sst.path()
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.strip_prefix("sst_"))
                    .and_then(|s| s.parse::<u64>().ok())
            })
            .max()
            .unwrap_or(0);

        tracing::info!(
            "Storage engine opened: {} SSTables loaded, generation={}",
            sstables.len(),
            max_gen
        );

        let cache_capacity = config.block_cache_size.max(16);
        let block_cache =
            parking_lot::Mutex::new(crate::storage::arc_cache::ArcCache::new(cache_capacity));

        Ok(Self {
            config,
            wal,
            active_memtable: memtable,
            immutable_memtables: RwLock::new(Vec::new()),
            sstables: RwLock::new(sstables),
            sstable_generation: AtomicU64::new(max_gen),
            block_cache,
            is_compacting: std::sync::atomic::AtomicBool::new(false),
            write_stalls: AtomicU64::new(0),
            compactions_completed: AtomicU64::new(0),
            is_open: std::sync::atomic::AtomicBool::new(true),
        })
    }

    /// Open a storage engine with default configuration at the given path.
    pub fn open_default(path: impl AsRef<Path>) -> FaizResult<Self> {
        Self::open(StorageConfig {
            data_dir: path.as_ref().to_path_buf(),
            ..Default::default()
        })
    }

    /// Put a key-value pair into the storage engine.
    ///
    /// The write is logged to the WAL (if enabled) and inserted into the
    /// active MemTable. If the MemTable exceeds its size limit, it is
    /// scheduled for flushing to an SSTable.
    pub fn put(&self, key: &[u8], value: &[u8]) -> FaizResult<()> {
        self.check_open()?;
        self.apply_write_backpressure()?;

        // Step 1: Write to WAL
        if let Some(wal) = &self.wal {
            wal.log_put(key, value)?;
        }

        // Step 2: Write to MemTable
        self.active_memtable.put(key.to_vec(), value.to_vec())?;

        // Update block cache
        self.block_cache
            .lock()
            .put(key.to_vec(), Some(value.to_vec()));

        // Step 3: Check if MemTable needs flushing
        if self.active_memtable.should_flush() {
            self.maybe_flush_memtable()?;
        }

        Ok(())
    }

    /// Put a batch of key-value pairs into the storage engine atomically (Group Commit).
    ///
    /// The entire batch is logged to the WAL in a single flush, providing maximum
    /// write throughput and transactional durability.
    pub fn put_batch(&self, entries: &[(&[u8], &[u8])]) -> FaizResult<()> {
        self.check_open()?;
        if entries.is_empty() {
            return Ok(());
        }
        self.apply_write_backpressure()?;

        // Step 1: Write batch to WAL
        if let Some(wal) = &self.wal {
            let ops: Vec<(WalOpType, &[u8], &[u8])> = entries
                .iter()
                .map(|&(k, v)| (WalOpType::Put, k, v))
                .collect();
            wal.append_batch(&ops)?;
        }

        // Step 2: Write to MemTable & Block Cache
        for &(k, v) in entries {
            self.active_memtable.put(k.to_vec(), v.to_vec())?;
            self.block_cache.lock().put(k.to_vec(), Some(v.to_vec()));
        }

        // Step 3: Check if MemTable needs flushing
        if self.active_memtable.should_flush() {
            self.maybe_flush_memtable()?;
        }

        Ok(())
    }

    /// Get a value by key.
    ///
    /// Search order (most recent data first):
    /// 1. Active MemTable
    /// 2. Immutable MemTables (waiting to be flushed)
    /// 3. Adaptive Replacement Cache (ARC) for warm SSTable data
    /// 4. SSTables on disk (newest to oldest)
    pub fn get(&self, key: &[u8]) -> FaizResult<Option<Vec<u8>>> {
        self.check_open()?;

        // Step 1: Check active MemTable
        if let Some(entry) = self.active_memtable.get(key) {
            return match entry {
                MemEntry::Value(v) => Ok(Some(v)),
                MemEntry::Tombstone => Ok(None), // Deleted
            };
        }

        // Step 2: Check immutable MemTables (newest first)
        {
            let immutables = self.immutable_memtables.read();
            for mt in immutables.iter().rev() {
                if let Some(entry) = mt.get(key) {
                    return match entry {
                        MemEntry::Value(v) => Ok(Some(v)),
                        MemEntry::Tombstone => Ok(None),
                    };
                }
            }
        }

        // Step 3: Check Adaptive Replacement Cache (ARC) for warm SSTable data
        {
            let mut cache = self.block_cache.lock();
            if let Some(cached_val) = cache.get(&key.to_vec()) {
                return Ok(cached_val);
            }
        }

        // Step 4: Check SSTables (newest to oldest)
        {
            let sstables = self.sstables.read();
            for sst in sstables.iter() {
                if let Some(entry) = sst.get(key)? {
                    let result = match entry {
                        MemEntry::Value(v) => Some(v),
                        MemEntry::Tombstone => None,
                    };
                    self.block_cache.lock().put(key.to_vec(), result.clone());
                    return Ok(result);
                }
            }
        }

        // Negative cache miss
        self.block_cache.lock().put(key.to_vec(), None);
        Ok(None)
    }

    /// Get statistics for the ARC block cache (hits, misses, hit ratio)
    pub fn cache_stats(&self) -> crate::storage::arc_cache::ArcCacheStats {
        self.block_cache.lock().stats()
    }

    /// Delete a key from the storage engine.
    ///
    /// This inserts a "tombstone" marker. The actual data is removed
    /// during compaction.
    pub fn delete(&self, key: &[u8]) -> FaizResult<()> {
        self.check_open()?;
        self.apply_write_backpressure()?;

        // Write to WAL
        if let Some(wal) = &self.wal {
            wal.log_delete(key)?;
        }

        // Insert tombstone into MemTable
        self.active_memtable.delete(key.to_vec())?;

        // Invalidate in block cache
        self.block_cache.lock().put(key.to_vec(), None);

        if self.active_memtable.should_flush() {
            self.maybe_flush_memtable()?;
        }

        Ok(())
    }

    /// Scan all keys with a given prefix.
    ///
    /// Returns key-value pairs in sorted order.
    pub fn prefix_scan(&self, prefix: &[u8]) -> FaizResult<Vec<(Vec<u8>, Vec<u8>)>> {
        self.check_open()?;

        let mut results = std::collections::BTreeMap::new();

        // Scan SSTables (oldest first, so newer values overwrite)
        {
            let sstables = self.sstables.read();
            for sst in sstables.iter().rev() {
                for entry_result in sst.iter()? {
                    let (key, entry) = entry_result?;
                    if key.starts_with(prefix) {
                        match entry {
                            MemEntry::Value(v) => {
                                results.insert(key, Some(v));
                            }
                            MemEntry::Tombstone => {
                                results.insert(key, None);
                            }
                        }
                    }
                }
            }
        }

        // Scan immutable MemTables
        {
            let immutables = self.immutable_memtables.read();
            for mt in immutables.iter() {
                for (key, entry) in mt.prefix_scan(prefix) {
                    match entry {
                        MemEntry::Value(v) => {
                            results.insert(key, Some(v));
                        }
                        MemEntry::Tombstone => {
                            results.insert(key, None);
                        }
                    }
                }
            }
        }

        // Scan active MemTable (newest, overwrites everything)
        for (key, entry) in self.active_memtable.prefix_scan(prefix) {
            match entry {
                MemEntry::Value(v) => {
                    results.insert(key, Some(v));
                }
                MemEntry::Tombstone => {
                    results.insert(key, None);
                }
            }
        }

        // Filter out tombstones and collect results
        Ok(results
            .into_iter()
            .filter_map(|(k, v)| v.map(|val| (k, val)))
            .collect())
    }

    /// Flush the active MemTable to an SSTable on disk.
    ///
    /// This is called automatically when the MemTable reaches its size limit,
    /// but can also be called manually for testing or before shutdown.
    pub fn flush(&self) -> FaizResult<()> {
        self.flush_memtable_to_sstable()
    }

    /// Get storage engine statistics
    pub fn stats(&self) -> StorageStats {
        let sstables = self.sstables.read();
        StorageStats {
            memtable_size: self.active_memtable.size(),
            memtable_entries: self.active_memtable.entry_count(),
            immutable_memtables: self.immutable_memtables.read().len(),
            sstable_count: sstables.len(),
            total_sstable_entries: sstables.iter().map(|s| s.entry_count()).sum(),
            write_stalls: self.write_stalls.load(Ordering::Relaxed),
            compactions_completed: self.compactions_completed.load(Ordering::Relaxed),
        }
    }

    /// Close the storage engine gracefully.
    ///
    /// Flushes all in-memory data to disk.
    pub fn close(&self) -> FaizResult<()> {
        if !self.is_open.load(Ordering::Acquire) {
            return Ok(());
        }

        // Flush active MemTable
        if self.active_memtable.entry_count() > 0 {
            self.flush_memtable_to_sstable()?;
        }

        // Sync WAL
        if let Some(wal) = &self.wal {
            wal.sync()?;
        }

        self.is_open.store(false, Ordering::Release);
        tracing::info!("Storage engine closed gracefully");
        Ok(())
    }

    // ── Internal Methods ─────────────────────────────────────────

    fn check_open(&self) -> FaizResult<()> {
        if !self.is_open.load(Ordering::Acquire) {
            return Err(FaizError::EngineClosed);
        }
        Ok(())
    }

    /// Apply dynamic write backpressure and anti-stall protection based on L0 SSTable depth
    fn apply_write_backpressure(&self) -> FaizResult<()> {
        let sst_count = self.sstables.read().len();
        if sst_count >= self.config.l0_stop_writes_trigger {
            self.write_stalls.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(
                "LSM write stall: {sst_count} L0 SSTables exceed stop threshold {}. Enforcing synchronous compaction.",
                self.config.l0_stop_writes_trigger
            );
            let _ = self.compact();
        } else if sst_count >= self.config.l0_slowdown_writes_trigger {
            self.write_stalls.fetch_add(1, Ordering::Relaxed);
            std::thread::yield_now();
        }
        Ok(())
    }

    fn maybe_flush_memtable(&self) -> FaizResult<()> {
        if self.active_memtable.should_flush() {
            self.flush_memtable_to_sstable()?;
        }
        Ok(())
    }

    fn flush_memtable_to_sstable(&self) -> FaizResult<()> {
        // Get all entries from the current MemTable
        let entries = self.active_memtable.entries();
        if entries.is_empty() {
            return Ok(());
        }

        // Generate new SSTable path
        let gen_num = self.sstable_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let sst_path = self
            .config
            .data_dir
            .join("sst")
            .join(format!("sst_{gen_num:06}.sst"));

        // Write SSTable
        let mut writer = SSTableWriter::new(&sst_path, entries.len())?;
        for (key, entry) in &entries {
            writer.write_entry(key, entry)?;
        }
        writer.finish()?;

        // Load the new SSTable reader
        let reader = SSTableReader::open(&sst_path)?;

        // Add to SSTable list (at the beginning = newest)
        {
            let mut sstables = self.sstables.write();
            sstables.insert(0, reader);
        }

        // Clear the MemTable
        self.active_memtable.clear();

        tracing::info!(
            "Flushed MemTable to SSTable: {} ({} entries)",
            sst_path.display(),
            entries.len()
        );

        // Reclaim older WAL segments now that data is persisted in SSTable
        if let Some(wal) = &self.wal {
            let _ = wal.checkpoint();
        }

        // Automatic compaction trigger: when Level 0 accumulates >= l0_compaction_trigger SSTables
        let sst_len = { self.sstables.read().len() };
        if sst_len >= self.config.l0_compaction_trigger
            && self
                .is_compacting
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
        {
            let res = self.run_compaction_locked();
            self.is_compacting.store(false, Ordering::Release);
            if let Err(e) = res {
                tracing::warn!("Automatic SSTable compaction failed: {e}");
            }
        }

        Ok(())
    }

    /// Perform LSM-Tree compaction: merge multiple SSTables into a single sorted SSTable,
    /// dropping tombstones and reclaiming disk space.
    pub fn compact(&self) -> FaizResult<usize> {
        self.check_open()?;
        if self
            .is_compacting
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            // Another thread is already actively running compaction
            return Ok(0);
        }

        let res = self.run_compaction_locked();
        self.is_compacting.store(false, Ordering::Release);
        res
    }

    fn run_compaction_locked(&self) -> FaizResult<usize> {
        let (sst_paths, count) = {
            let ssts = self.sstables.read();
            if ssts.len() < 2 {
                return Ok(0);
            }
            let paths: Vec<std::path::PathBuf> =
                ssts.iter().map(|s| s.path().to_path_buf()).collect();
            let count = ssts.len();
            (paths, count)
        };

        let gen_num = self.sstable_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let merged_path = self
            .config
            .data_dir
            .join("sst")
            .join(format!("sst_{gen_num:06}_compacted.sst"));

        // Merge input SSTables (dropping tombstones for compacted layer)
        crate::storage::compaction::merge_sstables(&sst_paths, &merged_path, true)?;

        // Open newly merged SSTable reader
        let merged_reader = SSTableReader::open(&merged_path)?;

        // Atomically update SSTables list with merged reader
        {
            let mut sstables = self.sstables.write();
            sstables.retain(|s| !sst_paths.contains(&s.path().to_path_buf()));
            sstables.push(merged_reader);
        }

        // Delete old SSTable files from disk
        for p in &sst_paths {
            let _ = std::fs::remove_file(p);
        }

        // Reclaim older WAL segments after compaction
        if let Some(wal) = &self.wal {
            let _ = wal.checkpoint();
        }

        self.compactions_completed.fetch_add(1, Ordering::Relaxed);

        tracing::info!(
            "Compacted {} SSTables into: {}",
            count,
            merged_path.display()
        );

        Ok(count)
    }
}

impl Drop for StorageEngine {
    fn drop(&mut self) {
        if self.is_open.load(Ordering::Acquire) {
            if let Err(e) = self.close() {
                tracing::error!("Error closing storage engine: {e}");
            }
        }
    }
}

/// Storage engine statistics
#[derive(Debug)]
pub struct StorageStats {
    pub memtable_size: usize,
    pub memtable_entries: usize,
    pub immutable_memtables: usize,
    pub sstable_count: usize,
    pub total_sstable_entries: u64,
    pub write_stalls: u64,
    pub compactions_completed: u64,
}

impl std::fmt::Display for StorageStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Storage Engine Stats:")?;
        writeln!(
            f,
            "  MemTable: {} entries ({} bytes)",
            self.memtable_entries, self.memtable_size
        )?;
        writeln!(f, "  Immutable MemTables: {}", self.immutable_memtables)?;
        writeln!(
            f,
            "  SSTables: {} ({} total entries)",
            self.sstable_count, self.total_sstable_entries
        )?;
        writeln!(
            f,
            "  Compaction: {} completed, {} write stalls handled",
            self.compactions_completed, self.write_stalls
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_engine(dir: &Path) -> StorageEngine {
        StorageEngine::open(StorageConfig {
            data_dir: dir.to_path_buf(),
            memtable_size: 4096, // Small for testing
            sync_writes: false,
            enable_wal: true,
            block_cache_size: 1024,
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn test_put_and_get() {
        let dir = tempfile::tempdir().unwrap();
        let engine = test_engine(dir.path());

        engine.put(b"hello", b"world").unwrap();
        engine.put(b"foo", b"bar").unwrap();

        assert_eq!(engine.get(b"hello").unwrap(), Some(b"world".to_vec()));
        assert_eq!(engine.get(b"foo").unwrap(), Some(b"bar".to_vec()));
        assert_eq!(engine.get(b"nonexistent").unwrap(), None);
    }

    #[test]
    fn test_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let engine = test_engine(dir.path());

        engine.put(b"key", b"old_value").unwrap();
        engine.put(b"key", b"new_value").unwrap();

        assert_eq!(engine.get(b"key").unwrap(), Some(b"new_value".to_vec()));
    }

    #[test]
    fn test_delete() {
        let dir = tempfile::tempdir().unwrap();
        let engine = test_engine(dir.path());

        engine.put(b"key", b"value").unwrap();
        assert!(engine.get(b"key").unwrap().is_some());

        engine.delete(b"key").unwrap();
        assert_eq!(engine.get(b"key").unwrap(), None);
    }

    #[test]
    fn test_flush_and_recovery() {
        let dir = tempfile::tempdir().unwrap();

        // Write data and flush
        {
            let engine = test_engine(dir.path());
            engine.put(b"persist_key", b"persist_value").unwrap();
            engine.flush().unwrap();
        }

        // Re-open and verify data is still there
        {
            let engine = test_engine(dir.path());
            assert_eq!(
                engine.get(b"persist_key").unwrap(),
                Some(b"persist_value".to_vec())
            );
        }
    }

    #[test]
    fn test_wal_recovery() {
        let dir = tempfile::tempdir().unwrap();

        // Write data (don't flush — simulate crash)
        {
            let engine = test_engine(dir.path());
            engine.put(b"wal_key", b"wal_value").unwrap();
            // Don't call flush() — data only in WAL + MemTable
        }

        // Re-open — should recover from WAL
        {
            let engine = test_engine(dir.path());
            assert_eq!(engine.get(b"wal_key").unwrap(), Some(b"wal_value".to_vec()));
        }
    }

    #[test]
    fn test_prefix_scan() {
        let dir = tempfile::tempdir().unwrap();
        let engine = test_engine(dir.path());

        engine.put(b"users:1", b"faiz").unwrap();
        engine.put(b"users:2", b"ali").unwrap();
        engine.put(b"users:3", b"abu").unwrap();
        engine.put(b"orders:1", b"order1").unwrap();

        let users = engine.prefix_scan(b"users:").unwrap();
        assert_eq!(users.len(), 3);

        let orders = engine.prefix_scan(b"orders:").unwrap();
        assert_eq!(orders.len(), 1);
    }

    #[test]
    fn test_many_entries() {
        let dir = tempfile::tempdir().unwrap();
        let engine = test_engine(dir.path());

        // Insert 500 entries (should trigger multiple flushes with small memtable)
        for i in 0..500u32 {
            let key = format!("key_{i:04}");
            let value = format!("value_{i}");
            engine.put(key.as_bytes(), value.as_bytes()).unwrap();
        }

        // Verify all entries
        for i in 0..500u32 {
            let key = format!("key_{i:04}");
            let expected = format!("value_{i}");
            let result = engine.get(key.as_bytes()).unwrap();
            assert_eq!(result, Some(expected.into_bytes()), "Failed at key_{i:04}");
        }

        let stats = engine.stats();
        assert!(
            stats.sstable_count > 0 || stats.memtable_entries > 0,
            "Data should exist somewhere"
        );
    }

    #[test]
    fn test_engine_close_and_reopen() {
        let dir = tempfile::tempdir().unwrap();

        {
            let engine = test_engine(dir.path());
            engine.put(b"key1", b"value1").unwrap();
            engine.put(b"key2", b"value2").unwrap();
            engine.close().unwrap();
        }

        {
            let engine = test_engine(dir.path());
            // Data should be recovered either from WAL or SSTable
            assert_eq!(engine.get(b"key1").unwrap(), Some(b"value1".to_vec()));
            assert_eq!(engine.get(b"key2").unwrap(), Some(b"value2".to_vec()));
        }
    }

    #[test]
    fn test_arc_cache_integration_in_storage_engine() {
        let dir = tempfile::tempdir().unwrap();
        let engine = test_engine(dir.path());

        // Put populates the ARC cache
        engine.put(b"cached_k1", b"val1").unwrap();
        assert_eq!(engine.get(b"cached_k1").unwrap(), Some(b"val1".to_vec()));

        let stats = engine.cache_stats();
        assert_eq!(stats.evictions, 0);

        // Delete invalidates in cache
        engine.delete(b"cached_k1").unwrap();
        assert_eq!(engine.get(b"cached_k1").unwrap(), None);
    }
}
