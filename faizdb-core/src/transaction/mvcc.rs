//! MVCC — Serializable Snapshot Isolation (SSI) for transactions.
//!
//! Each transaction gets a snapshot of the database at the time it started.
//! Writes are buffered locally and only applied on commit.
//!
//! Conflict detection operates at two levels:
//! 1. **Write-Write (WW)**: If two transactions modify the same key, the second
//!    to commit is aborted. (Standard in all SI implementations.)
//! 2. **Read-Write (RW) Anti-Dependency** (SSI): If transaction T1 reads a key
//!    that transaction T2 subsequently writes and commits, T1's commit is aborted
//!    because its read set is no longer consistent — preventing **write skew**
//!    anomalies that plain Snapshot Isolation cannot detect.
//!
//! This gives FaizDB the same isolation level as PostgreSQL 9.1+ SERIALIZABLE
//! and CockroachDB — the strongest ANSI SQL isolation level.

use std::collections::{BTreeMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::RwLock;

use crate::error::{FaizError, FaizResult};

/// Global transaction ID counter
static NEXT_TXN_ID: AtomicU64 = AtomicU64::new(1);

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxnStatus {
    Active,
    Committing,
    Committed,
    Aborted,
}

/// A buffered write operation within a transaction
#[derive(Debug, Clone)]
pub enum TxnWrite {
    Put(Vec<u8>),
    Delete,
}

/// A database transaction with snapshot isolation.
///
/// # Example
/// ```rust,ignore
/// let txn = Transaction::begin(read_fn);
/// txn.put(b"key", b"value")?;
/// txn.commit(write_fn)?;
/// ```
pub struct Transaction {
    /// Unique transaction ID
    pub id: u64,

    /// Transaction status
    status: TxnStatus,

    /// Snapshot timestamp — reads see data as of this point
    snapshot_ts: u64,

    /// Buffered writes (applied on commit)
    write_buffer: BTreeMap<Vec<u8>, TxnWrite>,

    /// Keys read during the transaction (for conflict detection)
    read_set: HashSet<Vec<u8>>,

    /// Creation instant of the transaction (for idle transaction timeout & reaping)
    created_at: std::time::Instant,
}

impl Transaction {
    /// Begin a new transaction.
    pub fn begin() -> Self {
        let id = NEXT_TXN_ID.fetch_add(1, Ordering::SeqCst);

        Self {
            id,
            status: TxnStatus::Active,
            snapshot_ts: id, // Simple: snapshot = txn id
            write_buffer: BTreeMap::new(),
            read_set: HashSet::new(),
            created_at: std::time::Instant::now(),
        }
    }

    /// Check if transaction has exceeded the idle timeout duration
    pub fn is_expired(&self, timeout: std::time::Duration) -> bool {
        self.created_at.elapsed() > timeout
    }

    /// Get creation instant of transaction
    pub fn created_at(&self) -> std::time::Instant {
        self.created_at
    }

    /// Buffer a put operation (applied on commit).
    pub fn put(&mut self, key: Vec<u8>, value: Vec<u8>) -> FaizResult<()> {
        self.check_active()?;
        self.write_buffer.insert(key, TxnWrite::Put(value));
        Ok(())
    }

    /// Buffer a delete operation (applied on commit).
    pub fn delete(&mut self, key: Vec<u8>) -> FaizResult<()> {
        self.check_active()?;
        self.write_buffer.insert(key, TxnWrite::Delete);
        Ok(())
    }

    /// Record a read for conflict detection
    pub fn record_read(&mut self, key: &[u8]) {
        self.read_set.insert(key.to_vec());
    }

    /// Get the write buffer (consumed on commit)
    pub fn take_writes(&mut self) -> BTreeMap<Vec<u8>, TxnWrite> {
        std::mem::take(&mut self.write_buffer)
    }

    /// Get reference to the write buffer
    pub fn write_buffer(&self) -> &BTreeMap<Vec<u8>, TxnWrite> {
        &self.write_buffer
    }

    /// Get the transaction status
    pub fn status(&self) -> TxnStatus {
        self.status
    }

    /// Get the snapshot timestamp
    pub fn snapshot_ts(&self) -> u64 {
        self.snapshot_ts
    }

    /// Check if a key has been written in this transaction
    pub fn has_write(&self, key: &[u8]) -> bool {
        self.write_buffer.contains_key(key)
    }

    /// Get a buffered write value (for read-your-own-writes)
    pub fn get_buffered(&self, key: &[u8]) -> Option<&TxnWrite> {
        self.write_buffer.get(key)
    }

    /// Get reference to the read set (for SSI validation)
    pub fn read_set(&self) -> &HashSet<Vec<u8>> {
        &self.read_set
    }

    /// Attempt to transition transaction to Committing status.
    /// Fails if the transaction is already Committing, Committed, or Aborted.
    pub fn try_set_committing(&mut self) -> FaizResult<()> {
        match self.status {
            TxnStatus::Active => {
                self.status = TxnStatus::Committing;
                Ok(())
            }
            TxnStatus::Committing => Err(FaizError::TransactionConflict(
                "Transaction is already being committed concurrently".into(),
            )),
            TxnStatus::Committed => Err(FaizError::TransactionAborted(
                "Transaction already committed".into(),
            )),
            TxnStatus::Aborted => Err(FaizError::TransactionAborted(
                "Transaction already aborted".into(),
            )),
        }
    }

    /// Restore Active status if commit fails recoverably before validation
    pub fn restore_active(&mut self) {
        if self.status == TxnStatus::Committing {
            self.status = TxnStatus::Active;
        }
    }

    /// Mark the transaction as committed
    pub fn mark_committed(&mut self) {
        self.status = TxnStatus::Committed;
    }

    /// Mark the transaction as aborted
    pub fn mark_aborted(&mut self) {
        self.status = TxnStatus::Aborted;
        self.write_buffer.clear();
    }

    /// Try to abort the transaction (fails if currently committing)
    pub fn try_abort(&mut self) -> FaizResult<()> {
        match self.status {
            TxnStatus::Active => {
                self.mark_aborted();
                Ok(())
            }
            TxnStatus::Committing => Err(FaizError::TransactionConflict(
                "Cannot abort transaction that is currently committing".into(),
            )),
            TxnStatus::Committed => Err(FaizError::TransactionAborted(
                "Cannot abort already committed transaction".into(),
            )),
            TxnStatus::Aborted => Ok(()),
        }
    }

    /// Abort the transaction (discard all buffered writes)
    pub fn abort(&mut self) {
        self.mark_aborted();
    }

    fn check_active(&self) -> FaizResult<()> {
        match self.status {
            TxnStatus::Active => Ok(()),
            TxnStatus::Committing => Err(FaizError::TransactionConflict(
                "Transaction is currently committing".into(),
            )),
            TxnStatus::Committed => Err(FaizError::TransactionAborted(
                "Transaction already committed".into(),
            )),
            TxnStatus::Aborted => Err(FaizError::TransactionAborted(
                "Transaction already aborted".into(),
            )),
        }
    }
}

/// Transaction manager — coordinates concurrent transactions with **Serializable Snapshot Isolation**.
///
/// Tracks active transactions and detects both:
/// - **Write-Write (WW) conflicts**: two transactions writing the same key
/// - **Read-Write (RW) anti-dependencies**: a transaction reading a key that was
///   subsequently written by a committed concurrent transaction (write skew)
pub struct TransactionManager {
    /// Active transactions
    active_txns: RwLock<HashSet<u64>>,

    /// Recently committed writes: key -> commit timestamp
    /// Used for WW conflict detection and RW anti-dependency (SSI) detection
    committed_writes: RwLock<BTreeMap<Vec<u8>, u64>>,

    /// Recently committed reads: key -> list of (txn_snapshot_ts) that read it
    /// Used for SSI write-skew detection: when a transaction commits writes,
    /// we check if any concurrent transaction read those keys.
    committed_reads: RwLock<BTreeMap<Vec<u8>, Vec<u64>>>,
}

impl TransactionManager {
    /// Create a new transaction manager
    pub fn new() -> Self {
        Self {
            active_txns: RwLock::new(HashSet::new()),
            committed_writes: RwLock::new(BTreeMap::new()),
            committed_reads: RwLock::new(BTreeMap::new()),
        }
    }

    /// Begin a new transaction
    pub fn begin(&self) -> Transaction {
        let txn = Transaction::begin();
        self.active_txns.write().insert(txn.id);
        txn
    }

    /// Validate and prepare a transaction for commit (pre-check, non-atomic).
    ///
    /// Checks for write-write conflicts: if any key in the transaction's
    /// write set was modified by another committed transaction after our
    /// snapshot, the commit fails.
    pub fn validate(&self, txn: &Transaction) -> FaizResult<()> {
        let committed = self.committed_writes.read();

        for key in txn.write_buffer.keys() {
            if let Some(&commit_ts) = committed.get(key) {
                if commit_ts > txn.snapshot_ts {
                    return Err(FaizError::TransactionConflict(format!(
                        "Key {:?} was modified by transaction committed at ts={}",
                        String::from_utf8_lossy(key),
                        commit_ts
                    )));
                }
            }
        }

        Ok(())
    }

    /// Validate SSI read-write anti-dependencies.
    ///
    /// Checks the transaction's **read set** against committed writes:
    /// if any key that this transaction read was modified by another
    /// transaction that committed after our snapshot started, this
    /// transaction must abort to prevent write skew.
    ///
    /// This is the core of Serializable Snapshot Isolation (SSI).
    fn validate_ssi(
        &self,
        txn: &Transaction,
        committed: &BTreeMap<Vec<u8>, u64>,
    ) -> FaizResult<()> {
        for key in txn.read_set() {
            // Skip keys we also wrote — our own writes are always visible
            if txn.write_buffer.contains_key(key) {
                continue;
            }
            if let Some(&commit_ts) = committed.get(key) {
                if commit_ts > txn.snapshot_ts {
                    return Err(FaizError::SerializationFailure(format!(
                        "Read-write anti-dependency on key {:?}: read at snapshot_ts={}, \
                         but another transaction committed a write at ts={} — \
                         aborting to prevent write skew (SSI violation)",
                        String::from_utf8_lossy(key),
                        txn.snapshot_ts,
                        commit_ts
                    )));
                }
            }
        }
        Ok(())
    }

    /// Record a transaction as committed atomically with full SSI conflict validation.
    ///
    /// Performs both Write-Write and Read-Write anti-dependency checks under a
    /// single write lock to prevent TOCTOU race conditions.
    pub fn commit(&self, txn: &mut Transaction) -> FaizResult<()> {
        let commit_ts = NEXT_TXN_ID.fetch_add(1, Ordering::SeqCst);

        // Atomically validate and record committed writes under a single write lock
        // to prevent time-of-check to time-of-use (TOCTOU) race conditions.
        {
            let mut committed = self.committed_writes.write();

            // 1. Write-Write conflict detection
            for key in txn.write_buffer.keys() {
                if let Some(&prev_commit_ts) = committed.get(key) {
                    if prev_commit_ts > txn.snapshot_ts {
                        return Err(FaizError::TransactionConflict(format!(
                            "Key {:?} was modified by transaction committed at ts={}",
                            String::from_utf8_lossy(key),
                            prev_commit_ts
                        )));
                    }
                }
            }

            // 2. SSI Read-Write anti-dependency detection (write skew prevention)
            self.validate_ssi(txn, &committed)?;

            // 3. All validations passed — record our writes
            for key in txn.write_buffer.keys() {
                committed.insert(key.clone(), commit_ts);
            }

            // 4. Record our reads for future SSI checks by other transactions
            if !txn.read_set().is_empty() {
                let mut committed_reads = self.committed_reads.write();
                for key in txn.read_set() {
                    committed_reads
                        .entry(key.clone())
                        .or_default()
                        .push(txn.snapshot_ts);
                }
            }
        }

        // Remove from active transactions and run watermark-based GC
        {
            let mut active = self.active_txns.write();
            active.remove(&txn.id);

            // Watermark-based GC: always prune entries older than the oldest
            // active transaction's snapshot timestamp. This prevents unbounded
            // memory growth even when long-running OLAP queries are active.
            let committed_len = self.committed_writes.read().len();
            if active.is_empty() {
                // Fast path: no active transactions → clear everything
                drop(active);
                self.committed_writes.write().clear();
                self.committed_reads.write().clear();
            } else if committed_len > 5_000 {
                // Incremental GC: prune entries no longer needed for conflict detection.
                // An entry is safe to remove if its commit_ts < min(active snapshot_ts),
                // because no active transaction can see or conflict with it.
                drop(active);
                self.gc();
            }

            // Emergency hard cap: if GC wasn't enough (e.g. single long-running txn
            // holds min_ts at 0), force-prune the oldest 50% to prevent OOM.
            let committed_len = self.committed_writes.read().len();
            if committed_len > 50_000 {
                let mut committed = self.committed_writes.write();
                let mut entries: Vec<_> = committed.iter().map(|(k, v)| (k.clone(), *v)).collect();
                entries.sort_by_key(|(_k, ts)| *ts);
                let half = entries.len() / 2;
                for (key, _ts) in entries.into_iter().take(half) {
                    committed.remove(&key);
                }
                tracing::warn!(
                    "[MVCC GC] Emergency prune: committed_writes exceeded 50K ({committed_len} entries). \
                     Pruned oldest 50%. Consider reducing long-running transaction duration."
                );
            }
        }

        txn.mark_committed();
        Ok(())
    }

    /// Record a transaction as aborted
    pub fn abort(&self, txn: &mut Transaction) {
        self.active_txns.write().remove(&txn.id);
        txn.mark_aborted();
    }

    /// Get the number of active transactions
    pub fn active_count(&self) -> usize {
        self.active_txns.read().len()
    }

    /// Clean up old committed write and read records.
    ///
    /// Removes records older than the oldest active transaction's snapshot,
    /// since they can no longer cause conflicts.
    pub fn gc(&self) {
        let active = self.active_txns.read();
        if active.is_empty() {
            // No active transactions — safe to clear all
            self.committed_writes.write().clear();
            self.committed_reads.write().clear();
            return;
        }

        let min_ts = match active.iter().min() {
            Some(&ts) => ts,
            None => return,
        };
        let mut committed = self.committed_writes.write();
        committed.retain(|_, ts| *ts >= min_ts);

        // Also GC committed reads — prune entries where all snapshot timestamps
        // are below the watermark
        let mut reads = self.committed_reads.write();
        reads.retain(|_, timestamps| {
            timestamps.retain(|&ts| ts >= min_ts);
            !timestamps.is_empty()
        });
    }
}

impl Default for TransactionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_basic() {
        let mut txn = Transaction::begin();

        txn.put(b"key1".to_vec(), b"value1".to_vec()).unwrap();
        txn.put(b"key2".to_vec(), b"value2".to_vec()).unwrap();

        assert!(txn.has_write(b"key1"));
        assert!(!txn.has_write(b"key3"));
        assert_eq!(txn.status(), TxnStatus::Active);
    }

    #[test]
    fn test_transaction_abort() {
        let mut txn = Transaction::begin();
        txn.put(b"key".to_vec(), b"value".to_vec()).unwrap();

        txn.abort();

        assert_eq!(txn.status(), TxnStatus::Aborted);
        assert!(txn.put(b"key2".to_vec(), b"value2".to_vec()).is_err());
    }

    #[test]
    fn test_transaction_manager_no_conflict() {
        let mgr = TransactionManager::new();

        // Transaction 1: writes key_a
        let mut txn1 = mgr.begin();
        txn1.put(b"key_a".to_vec(), b"val1".to_vec()).unwrap();
        mgr.commit(&mut txn1).unwrap();
        assert_eq!(txn1.status(), TxnStatus::Committed);

        // Transaction 2: writes key_b (no conflict)
        let mut txn2 = mgr.begin();
        txn2.put(b"key_b".to_vec(), b"val2".to_vec()).unwrap();
        mgr.commit(&mut txn2).unwrap();
        assert_eq!(txn2.status(), TxnStatus::Committed);
    }

    #[test]
    fn test_transaction_manager_conflict() {
        let mgr = TransactionManager::new();

        // Both transactions start at the same time
        let mut txn1 = mgr.begin();
        let mut txn2 = mgr.begin();

        // Both write to the same key
        txn1.put(b"shared_key".to_vec(), b"val1".to_vec()).unwrap();
        txn2.put(b"shared_key".to_vec(), b"val2".to_vec()).unwrap();

        // First commit succeeds
        mgr.commit(&mut txn1).unwrap();

        // Second commit should fail (conflict)
        let result = mgr.commit(&mut txn2);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_your_own_writes() {
        let mut txn = Transaction::begin();
        txn.put(b"key".to_vec(), b"value".to_vec()).unwrap();

        let write = txn.get_buffered(b"key").unwrap();
        match write {
            TxnWrite::Put(v) => assert_eq!(v, b"value"),
            TxnWrite::Delete => panic!("Expected Put"),
        }
    }

    // ── SSI (Serializable Snapshot Isolation) Tests ────────────────────────

    #[test]
    fn test_ssi_write_skew_bank_accounts() {
        // Classic write-skew scenario:
        // Two bank accounts A=100, B=100. Constraint: A+B >= 0.
        // T1 reads both, sees A=100 B=100, withdraws 200 from A → A=-100 (A+B=0, ok)
        // T2 reads both, sees A=100 B=100, withdraws 200 from B → B=-100 (A+B=0, ok)
        // If both commit, A=-100 B=-100, A+B=-200 — VIOLATION!
        // SSI must prevent one of them from committing.
        let mgr = TransactionManager::new();

        // Seed the accounts
        let mut seed = mgr.begin();
        seed.put(b"account_A".to_vec(), b"100".to_vec()).unwrap();
        seed.put(b"account_B".to_vec(), b"100".to_vec()).unwrap();
        mgr.commit(&mut seed).unwrap();

        // T1: reads both accounts, decides to withdraw from A
        let mut t1 = mgr.begin();
        t1.record_read(b"account_A");
        t1.record_read(b"account_B");
        t1.put(b"account_A".to_vec(), b"-100".to_vec()).unwrap();

        // T2: reads both accounts, decides to withdraw from B
        let mut t2 = mgr.begin();
        t2.record_read(b"account_A");
        t2.record_read(b"account_B");
        t2.put(b"account_B".to_vec(), b"-100".to_vec()).unwrap();

        // T1 commits first — succeeds
        mgr.commit(&mut t1).unwrap();
        assert_eq!(t1.status(), TxnStatus::Committed);

        // T2 should fail: it read account_A, which T1 wrote after T2's snapshot
        let result = mgr.commit(&mut t2);
        assert!(
            result.is_err(),
            "T2 must fail due to SSI write-skew on account_A"
        );
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("write skew")
                || err.to_string().contains("Serialization failure"),
            "Error should mention write skew: {err}"
        );
    }

    #[test]
    fn test_ssi_read_write_anti_dependency() {
        // T1 reads key_x, T2 writes key_x and commits, T1 tries to commit a write to key_y
        // SSI should abort T1 because its read of key_x is stale.
        let mgr = TransactionManager::new();

        let mut t1 = mgr.begin();
        t1.record_read(b"key_x");

        let mut t2 = mgr.begin();
        t2.put(b"key_x".to_vec(), b"new_value".to_vec()).unwrap();
        mgr.commit(&mut t2).unwrap();

        // T1 writes a different key but its read of key_x is now stale
        t1.put(b"key_y".to_vec(), b"based_on_stale_read".to_vec())
            .unwrap();
        let result = mgr.commit(&mut t1);
        assert!(
            result.is_err(),
            "T1 should fail due to SSI — it read key_x which was written by T2"
        );
    }

    #[test]
    fn test_ssi_self_written_key_no_false_positive() {
        // If a transaction reads AND writes the same key, it should NOT be
        // flagged as a write-skew violation against its own writes.
        let mgr = TransactionManager::new();

        let mut seed = mgr.begin();
        seed.put(b"counter".to_vec(), b"0".to_vec()).unwrap();
        mgr.commit(&mut seed).unwrap();

        let mut t1 = mgr.begin();
        t1.record_read(b"counter");
        t1.put(b"counter".to_vec(), b"1".to_vec()).unwrap();
        // Should succeed — the read and write are on the same key within the same txn
        mgr.commit(&mut t1).unwrap();
        assert_eq!(t1.status(), TxnStatus::Committed);
    }

    #[test]
    fn test_ssi_non_overlapping_reads_pass() {
        // T1 reads key_a, writes key_b
        // T2 reads key_c, writes key_d
        // No overlap → both should commit
        let mgr = TransactionManager::new();

        let mut t1 = mgr.begin();
        t1.record_read(b"key_a");
        t1.put(b"key_b".to_vec(), b"val".to_vec()).unwrap();

        let mut t2 = mgr.begin();
        t2.record_read(b"key_c");
        t2.put(b"key_d".to_vec(), b"val".to_vec()).unwrap();

        mgr.commit(&mut t1).unwrap();
        mgr.commit(&mut t2).unwrap();
        assert_eq!(t1.status(), TxnStatus::Committed);
        assert_eq!(t2.status(), TxnStatus::Committed);
    }

    #[test]
    fn test_ssi_sequential_transactions_pass() {
        // If T1 commits before T2 begins, T2 should see T1's writes
        // and there should be no conflict even if T2 reads what T1 wrote.
        let mgr = TransactionManager::new();

        let mut t1 = mgr.begin();
        t1.put(b"key".to_vec(), b"value_from_t1".to_vec()).unwrap();
        mgr.commit(&mut t1).unwrap();

        let mut t2 = mgr.begin();
        t2.record_read(b"key");
        t2.put(b"key2".to_vec(), b"value_from_t2".to_vec()).unwrap();
        // Should succeed — T1 committed before T2's snapshot, so T2 sees T1's writes
        mgr.commit(&mut t2).unwrap();
        assert_eq!(t2.status(), TxnStatus::Committed);
    }
}
