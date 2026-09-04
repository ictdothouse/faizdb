//! MVCC (Multi-Version Concurrency Control) — snapshot isolation for transactions.
//!
//! Each transaction gets a snapshot of the database at the time it started.
//! Writes are buffered locally and only applied on commit.
//! Write-write conflicts are detected: if two transactions modify the same key,
//! the second one to commit will be aborted.

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

/// Transaction manager — coordinates concurrent transactions.
///
/// Tracks active transactions and detects write-write conflicts.
pub struct TransactionManager {
    /// Active transactions
    active_txns: RwLock<HashSet<u64>>,

    /// Recently committed writes: key -> commit timestamp
    /// Used for conflict detection
    committed_writes: RwLock<BTreeMap<Vec<u8>, u64>>,
}

impl TransactionManager {
    /// Create a new transaction manager
    pub fn new() -> Self {
        Self {
            active_txns: RwLock::new(HashSet::new()),
            committed_writes: RwLock::new(BTreeMap::new()),
        }
    }

    /// Begin a new transaction
    pub fn begin(&self) -> Transaction {
        let txn = Transaction::begin();
        self.active_txns.write().insert(txn.id);
        txn
    }

    /// Validate and prepare a transaction for commit.
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

    /// Record a transaction as committed
    pub fn commit(&self, txn: &mut Transaction) -> FaizResult<()> {
        self.validate(txn)?;

        let commit_ts = NEXT_TXN_ID.fetch_add(1, Ordering::SeqCst);

        // Record committed writes for future conflict detection
        {
            let mut committed = self.committed_writes.write();
            for key in txn.write_buffer.keys() {
                committed.insert(key.clone(), commit_ts);
            }
        }

        // Remove from active transactions
        self.active_txns.write().remove(&txn.id);

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

    /// Clean up old committed write records.
    ///
    /// Removes records older than the oldest active transaction's snapshot,
    /// since they can no longer cause conflicts.
    pub fn gc(&self) {
        let active = self.active_txns.read();
        if active.is_empty() {
            // No active transactions — safe to clear all
            self.committed_writes.write().clear();
            return;
        }

        let min_ts = *active.iter().min().unwrap();
        let mut committed = self.committed_writes.write();
        committed.retain(|_, ts| *ts >= min_ts);
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
}
