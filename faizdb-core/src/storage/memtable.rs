//! MemTable — in-memory write buffer using a **lock-free concurrent SkipMap**.
//!
//! The MemTable is the first stop for all writes. It provides:
//! - O(log n) insert, get, and delete operations
//! - **Lock-free concurrent reads AND writes** (via `crossbeam-skiplist`)
//! - Ordered iteration (for efficient range scans and SSTable flushing)
//! - Configurable size threshold for triggering flush to disk
//!
//! When the MemTable reaches its size limit, it becomes "immutable" and
//! is flushed to an SSTable on disk. A new empty MemTable is created
//! for incoming writes.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use crossbeam_skiplist::SkipMap;

use crate::error::{FaizError, FaizResult};

/// A single entry in the MemTable
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemEntry {
    /// A key-value pair (Put operation)
    Value(Vec<u8>),
    /// A tombstone marker (Delete operation)
    Tombstone,
}

impl MemEntry {
    /// Check if this entry is a tombstone (deletion marker)
    pub fn is_tombstone(&self) -> bool {
        matches!(self, MemEntry::Tombstone)
    }

    /// Get the value bytes, if this is a Value entry
    pub fn as_value(&self) -> Option<&[u8]> {
        match self {
            MemEntry::Value(v) => Some(v),
            MemEntry::Tombstone => None,
        }
    }

    /// Get the approximate size in bytes
    pub fn size(&self) -> usize {
        match self {
            MemEntry::Value(v) => v.len(),
            MemEntry::Tombstone => 0,
        }
    }
}

/// In-memory sorted buffer for recent writes.
///
/// Uses a **lock-free `crossbeam_skiplist::SkipMap`** for concurrent access.
/// This gives us:
/// - **Lock-free reads** — multiple threads read simultaneously without blocking
/// - **Lock-free writes** — multiple threads write simultaneously without contention
/// - **Sorted iteration** — essential for SSTable flushing
/// - **No reader-writer starvation** — unlike `RwLock<BTreeMap>`
///
/// ## Why SkipMap?
///
/// SkipMap provides O(log n) concurrent insert/get/delete with no global locks.
/// This is critical for high-throughput write workloads (>100K ops/sec) where
/// a single `RwLock` write lock becomes the bottleneck. All production databases
/// (RocksDB, LevelDB, Pebble) use lock-free skip lists for their MemTables.
///
/// Size tracking is approximate (standard in all production databases) — the
/// worst case is triggering a flush a few bytes earlier or later, which has
/// zero impact on correctness.
pub struct MemTable {
    /// Lock-free sorted key-value store
    data: SkipMap<Vec<u8>, MemEntry>,

    /// Approximate size of all entries in bytes (lock-free atomic counter)
    size: AtomicUsize,

    /// Maximum size before triggering a flush (in bytes)
    max_size: usize,

    /// Whether this MemTable is frozen (immutable, waiting to be flushed)
    frozen: AtomicBool,

    /// Number of entries (approximate under high concurrency)
    count: AtomicUsize,
}

impl MemTable {
    /// Create a new MemTable with the specified maximum size.
    pub fn new(max_size: usize) -> Self {
        Self {
            data: SkipMap::new(),
            size: AtomicUsize::new(0),
            max_size,
            frozen: AtomicBool::new(false),
            count: AtomicUsize::new(0),
        }
    }

    /// Create a new MemTable with the default max size (64 MB)
    pub fn with_default_size() -> Self {
        Self::new(crate::DEFAULT_MEMTABLE_SIZE)
    }

    /// Insert a key-value pair.
    ///
    /// Returns an error if the MemTable is frozen (immutable).
    ///
    /// This is **lock-free** — multiple threads can call `put` concurrently
    /// without blocking each other or any concurrent readers.
    pub fn put(&self, key: Vec<u8>, value: Vec<u8>) -> FaizResult<()> {
        if self.frozen.load(Ordering::Acquire) {
            return Err(FaizError::Internal(
                "MemTable is frozen — cannot accept writes".into(),
            ));
        }

        let entry_size = key.len() + value.len();

        // Approximate size tracking: check if replacing an existing entry.
        // There is a benign TOCTOU window here (another thread could modify the
        // same key between get and insert), but approximate size tracking is the
        // industry standard (RocksDB, LevelDB, Pebble all do this). The worst
        // case is triggering a memtable flush a few bytes earlier or later.
        if let Some(old) = self.data.get(&key) {
            let old_size = key.len() + old.value().size();
            self.size.fetch_sub(old_size, Ordering::Relaxed);
        } else {
            self.count.fetch_add(1, Ordering::Relaxed);
        }

        self.data.insert(key, MemEntry::Value(value));
        self.size.fetch_add(entry_size, Ordering::Relaxed);

        Ok(())
    }

    /// Mark a key as deleted (insert a tombstone).
    pub fn delete(&self, key: Vec<u8>) -> FaizResult<()> {
        if self.frozen.load(Ordering::Acquire) {
            return Err(FaizError::Internal(
                "MemTable is frozen — cannot accept writes".into(),
            ));
        }

        if let Some(old) = self.data.get(&key) {
            let old_size = key.len() + old.value().size();
            self.size.fetch_sub(old_size, Ordering::Relaxed);
        } else {
            self.count.fetch_add(1, Ordering::Relaxed);
        }

        let tombstone_size = key.len();
        self.data.insert(key, MemEntry::Tombstone);
        self.size.fetch_add(tombstone_size, Ordering::Relaxed);

        Ok(())
    }

    /// Get a value by key.
    ///
    /// Returns:
    /// - `Some(MemEntry::Value(bytes))` if the key exists
    /// - `Some(MemEntry::Tombstone)` if the key was deleted
    /// - `None` if the key is not in this MemTable
    ///
    /// This is **lock-free** — never blocks writers or other readers.
    pub fn get(&self, key: &[u8]) -> Option<MemEntry> {
        self.data.get(key).map(|entry| entry.value().clone())
    }

    /// Check if the MemTable contains a key (including tombstones)
    pub fn contains(&self, key: &[u8]) -> bool {
        self.data.contains_key(key)
    }

    /// Get the current approximate size in bytes
    pub fn size(&self) -> usize {
        self.size.load(Ordering::Relaxed)
    }

    /// Get the number of entries (including tombstones)
    pub fn entry_count(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    /// Check if the MemTable should be flushed (has reached max size)
    pub fn should_flush(&self) -> bool {
        self.size.load(Ordering::Relaxed) >= self.max_size
    }

    /// Check if the MemTable is frozen (immutable)
    pub fn is_frozen(&self) -> bool {
        self.frozen.load(Ordering::Acquire)
    }

    /// Freeze the MemTable — makes it immutable.
    ///
    /// After freezing, no more writes are accepted. The MemTable is
    /// now ready to be flushed to an SSTable.
    pub fn freeze(&self) {
        self.frozen.store(true, Ordering::Release);
    }

    /// Get all entries in sorted order.
    ///
    /// Used when flushing to an SSTable. The SkipMap iterator naturally
    /// yields entries in sorted key order.
    pub fn entries(&self) -> Vec<(Vec<u8>, MemEntry)> {
        self.data
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Perform a range scan over keys.
    ///
    /// Returns all entries where `start <= key < end`.
    pub fn range_scan(&self, start: &[u8], end: &[u8]) -> Vec<(Vec<u8>, MemEntry)> {
        self.data
            .range(start.to_vec()..end.to_vec())
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Scan all entries with a key prefix.
    pub fn prefix_scan(&self, prefix: &[u8]) -> Vec<(Vec<u8>, MemEntry)> {
        // Use range scan from prefix start to compute efficient bounds
        self.data
            .range(prefix.to_vec()..)
            .take_while(|entry| entry.key().starts_with(prefix))
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Clear all entries (used after successful flush to SSTable).
    ///
    /// This is O(n) but only called during flush, which is an infrequent
    /// background operation. The frozen flag prevents concurrent writes.
    pub fn clear(&self) {
        // Drain all entries from the SkipMap
        let keys: Vec<Vec<u8>> = self.data.iter().map(|e| e.key().clone()).collect();
        for key in keys {
            self.data.remove(&key);
        }
        self.size.store(0, Ordering::Relaxed);
        self.count.store(0, Ordering::Relaxed);
        self.frozen.store(false, Ordering::Release);
    }
}

impl std::fmt::Debug for MemTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemTable")
            .field("entries", &self.count.load(Ordering::Relaxed))
            .field("size_bytes", &self.size.load(Ordering::Relaxed))
            .field("max_size", &self.max_size)
            .field("frozen", &self.frozen.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memtable_put_get() {
        let mt = MemTable::new(1024 * 1024); // 1MB

        mt.put(b"key1".to_vec(), b"value1".to_vec()).unwrap();
        mt.put(b"key2".to_vec(), b"value2".to_vec()).unwrap();

        let entry = mt.get(b"key1").unwrap();
        assert_eq!(entry.as_value().unwrap(), b"value1");

        let entry = mt.get(b"key2").unwrap();
        assert_eq!(entry.as_value().unwrap(), b"value2");

        assert!(mt.get(b"key3").is_none());
    }

    #[test]
    fn test_memtable_delete() {
        let mt = MemTable::new(1024 * 1024);

        mt.put(b"key1".to_vec(), b"value1".to_vec()).unwrap();
        mt.delete(b"key1".to_vec()).unwrap();

        let entry = mt.get(b"key1").unwrap();
        assert!(entry.is_tombstone());
    }

    #[test]
    fn test_memtable_overwrite() {
        let mt = MemTable::new(1024 * 1024);

        mt.put(b"key1".to_vec(), b"old_value".to_vec()).unwrap();
        mt.put(b"key1".to_vec(), b"new_value".to_vec()).unwrap();

        let entry = mt.get(b"key1").unwrap();
        assert_eq!(entry.as_value().unwrap(), b"new_value");
        assert_eq!(mt.entry_count(), 1); // Should not double-count
    }

    #[test]
    fn test_memtable_frozen() {
        let mt = MemTable::new(1024 * 1024);

        mt.put(b"key1".to_vec(), b"value1".to_vec()).unwrap();
        mt.freeze();

        assert!(mt.is_frozen());

        // Should fail because frozen
        let result = mt.put(b"key2".to_vec(), b"value2".to_vec());
        assert!(result.is_err());
    }

    #[test]
    fn test_memtable_ordered_entries() {
        let mt = MemTable::new(1024 * 1024);

        mt.put(b"charlie".to_vec(), b"3".to_vec()).unwrap();
        mt.put(b"alpha".to_vec(), b"1".to_vec()).unwrap();
        mt.put(b"bravo".to_vec(), b"2".to_vec()).unwrap();

        let entries = mt.entries();
        let keys: Vec<&[u8]> = entries.iter().map(|(k, _)| k.as_slice()).collect();

        // Should be sorted
        assert_eq!(keys, vec![b"alpha".as_slice(), b"bravo", b"charlie"]);
    }

    #[test]
    fn test_memtable_should_flush() {
        let mt = MemTable::new(100); // Very small max size

        // Should not need flush yet
        assert!(!mt.should_flush());

        // Insert enough data to trigger flush
        mt.put(b"key".to_vec(), vec![0u8; 101]).unwrap();
        assert!(mt.should_flush());
    }

    #[test]
    fn test_memtable_prefix_scan() {
        let mt = MemTable::new(1024 * 1024);

        mt.put(b"users:1".to_vec(), b"faiz".to_vec()).unwrap();
        mt.put(b"users:2".to_vec(), b"ali".to_vec()).unwrap();
        mt.put(b"orders:1".to_vec(), b"order1".to_vec()).unwrap();
        mt.put(b"users:3".to_vec(), b"abu".to_vec()).unwrap();

        let users = mt.prefix_scan(b"users:");
        assert_eq!(users.len(), 3);

        let orders = mt.prefix_scan(b"orders:");
        assert_eq!(orders.len(), 1);
    }

    #[test]
    fn test_memtable_clear() {
        let mt = MemTable::new(1024 * 1024);

        mt.put(b"key1".to_vec(), b"value1".to_vec()).unwrap();
        mt.put(b"key2".to_vec(), b"value2".to_vec()).unwrap();
        mt.freeze();

        mt.clear();

        assert!(!mt.is_frozen());
        assert_eq!(mt.entry_count(), 0);
        assert_eq!(mt.size(), 0);
        assert!(mt.get(b"key1").is_none());
    }

    #[test]
    fn test_memtable_size_tracking() {
        let mt = MemTable::new(1024 * 1024);

        mt.put(b"key".to_vec(), b"value".to_vec()).unwrap(); // 3 + 5 = 8 bytes
        assert_eq!(mt.size(), 8);

        mt.put(b"key".to_vec(), b"new_value".to_vec()).unwrap(); // Replace: 3 + 9 = 12 bytes
        assert_eq!(mt.size(), 12); // Old 8 removed, new 12 added
    }
}
