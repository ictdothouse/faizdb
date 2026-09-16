//! Property-Based Testing Suite for FaizDB Invariants
//!
//! Uses `proptest` to mathematically verify core storage engine and concurrency invariants
//! across thousands of randomly generated permutations:
//! 1. SSTable Block Compression & Roundtrip Invariant.
//! 2. MemTable Lexicographical Ordering Invariant.
//! 3. MVCC Snapshot Conflict Detection Invariant.

use faizdb_core::storage::memtable::{MemEntry, MemTable};
use faizdb_core::storage::sstable::{Compression, SSTableReader, SSTableWriter};
use faizdb_core::transaction::mvcc::TransactionManager;
use proptest::prelude::*;
use tempfile::tempdir;

proptest! {
    /// Invariant: Any arbitrary collection of (Key, Value) pairs written to an SSTable
    /// must be decoded with 100% byte-for-byte fidelity across sparse block indexes.
    #[test]
    fn prop_sstable_roundtrip_fidelity(
        mut entries in prop::collection::vec(
            (prop::collection::vec(any::<u8>(), 1..64), prop::collection::vec(any::<u8>(), 0..256)),
            1..50
        )
    ) {
        // Ensure unique keys and sorted order as required by SSTable specification
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.dedup_by(|a, b| a.0 == b.0);

        let dir = tempdir().unwrap();
        let sst_path = dir.path().join("prop_test.sst");

        // Write to SSTable with LZ4 block compression
        let mut writer = SSTableWriter::with_compression(&sst_path, entries.len(), Compression::Lz4).unwrap();
        for (k, v) in &entries {
            writer.write_entry(k, &MemEntry::Value(v.clone())).unwrap();
        }
        writer.finish().unwrap();

        // Read back from SSTable
        let reader = SSTableReader::open(&sst_path).unwrap();
        assert_eq!(reader.entry_count(), entries.len() as u64);

        for (k, v) in &entries {
            let res = reader.get(k).unwrap();
            prop_assert_eq!(res, Some(MemEntry::Value(v.clone())));
        }
    }

    /// Invariant: Irrespective of key insertion order into MemTable,
    /// a prefix scan must always yield keys in strict lexicographical order.
    #[test]
    fn prop_memtable_sorted_order_invariant(
        keys in prop::collection::vec(prop::collection::vec(any::<u8>(), 1..32), 1..100)
    ) {
        let memtable = MemTable::new(1024 * 1024);

        for k in &keys {
            memtable.put(k.clone(), b"value".to_vec()).unwrap();
        }

        let scanned = memtable.prefix_scan(b"");
        for window in scanned.windows(2) {
            prop_assert!(window[0].0 <= window[1].0, "MemTable keys must be monotonically increasing");
        }
    }

    /// Invariant: In MVCC, if transaction T2 commits a write to key K after T1's snapshot,
    /// T1 must be rejected by SSI Write-Write or Read-Write validation.
    #[test]
    fn prop_mvcc_concurrent_write_conflict_invariant(
        key in prop::collection::vec(any::<u8>(), 1..32),
        val1 in prop::collection::vec(any::<u8>(), 1..32),
        val2 in prop::collection::vec(any::<u8>(), 1..32)
    ) {
        let tm = TransactionManager::new();

        let mut t1 = tm.begin();
        let mut t2 = tm.begin();

        // Both write to the same key
        t1.put(key.clone(), val1).unwrap();
        t2.put(key, val2).unwrap();

        // T1 commits first
        prop_assert!(tm.commit(&mut t1).is_ok());

        // T2 must be aborted because the key was modified after its snapshot started
        prop_assert!(tm.commit(&mut t2).is_err());
    }
}
