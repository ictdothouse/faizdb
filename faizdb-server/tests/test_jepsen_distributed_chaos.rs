//! Jepsen-Style Distributed Chaos & Bulletproof Resilience Suite
//!
//! Evaluates the 5 critical enterprise hardening pillars:
//! 1. Crash Safety & Torn-Write Recovery (Mid-block truncation / power cut resilience)
//! 2. Raft Distributed Quorum & Split-Brain Prevention (Majority vs Minority partition isolation)
//! 3. Active-Active CRDT Eventual Convergence & Vector Clock Causality
//! 4. LSM-Tree Compaction Anti-Stall Engine (Backpressure & SSTable depth bounding)
//! 5. Postgres Virtual System Catalog Introspection (pg_catalog & information_schema compliance)

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

use faizdb_core::cluster::crdt::CrdtLwwRegister;
use faizdb_core::cluster::raft::{AppendEntriesArgs, LogEntry, RaftNode};
use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use faizdb_query::DatabaseContext;
use faizdb_server::wire::postgres::handler::handle_postgres_query;

// ════════════════════════════════════════════════════════════════════════════
// Pillar 1: Crash Safety & Torn-Write Recovery Simulation
// ════════════════════════════════════════════════════════════════════════════
#[test]
fn test_jepsen_torn_write_crash_recovery() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().to_path_buf();

    // 1. Initial write phase: write 50 records to WAL
    {
        let config = StorageConfig {
            data_dir: data_dir.clone(),
            memtable_size: 1024 * 1024,
            sync_writes: false,
            enable_wal: true,
            ..Default::default()
        };
        let engine = StorageEngine::open(config).expect("Failed to open initial engine");

        for i in 0..50 {
            let k = format!("acct:user:{i:03}");
            let v = format!("balance:{i}000");
            engine.put(k.as_bytes(), v.as_bytes()).unwrap();
        }
        // Drop engine
        drop(engine);
    }

    // 2. Simulate torn write / mid-block OS crash:
    // Append 11 bytes of garbage to the tail of the WAL file (simulating power cut during 51st write)
    let wal_dir = data_dir.join("wal");
    let mut wal_file = None;
    for entry in std::fs::read_dir(&wal_dir).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().and_then(|e| e.to_str()) == Some("log") {
            wal_file = Some(entry.path());
            break;
        }
    }
    let wal_path = wal_file.expect("WAL file must exist");
    {
        let mut f = OpenOptions::new().append(true).open(&wal_path).unwrap();
        // 11 random incomplete bytes (invalid header, missing CRC, truncated payload)
        f.write_all(b"\x99\x88\x77\x66\x55\x44\x33\x22\x11\x00\xAA").unwrap();
        f.flush().unwrap();
    }

    // 3. Crash recovery phase: reopen engine
    // Engine MUST NOT panic on the torn write. It must detect corruption via CRC/bounds,
    // gracefully truncate the corrupted tail, and restore all 50 valid committed records.
    {
        let config = StorageConfig {
            data_dir: data_dir.clone(),
            memtable_size: 1024 * 1024,
            sync_writes: false,
            enable_wal: true,
            ..Default::default()
        };
        let engine = StorageEngine::open(config)
            .expect("Engine must gracefully recover from torn write without crashing");

        for i in 0..50 {
            let k = format!("acct:user:{i:03}");
            let expected_v = format!("balance:{i}000");
            let actual = engine.get(k.as_bytes()).unwrap();
            assert_eq!(
                actual,
                Some(expected_v.into_bytes()),
                "Committed transaction {k} must survive torn-write recovery"
            );
        }
        println!("  🛡️ [CHAOS-1] Torn-write recovery succeeded: 50 committed keys preserved, corrupted tail cleanly truncated");
    }
}

// ════════════════════════════════════════════════════════════════════════════
// Pillar 2: 5-Node Raft Quorum & Split-Brain Prevention Simulation
// ════════════════════════════════════════════════════════════════════════════
#[test]
fn test_jepsen_raft_majority_minority_split_brain_isolation() {
    // 5-node distributed cluster
    let n1 = RaftNode::new("node_1", "127.0.0.1:29011");
    let n2 = RaftNode::new("node_2", "127.0.0.1:29012");
    let n3 = RaftNode::new("node_3", "127.0.0.1:29013");
    let n4 = RaftNode::new("node_4", "127.0.0.1:29014");
    let n5 = RaftNode::new("node_5", "127.0.0.1:29015");

    // Register all peers on Node 1: Quorum for 5 nodes is (5 / 2) + 1 = 3
    n1.add_peer("node_2", "127.0.0.1:29012");
    n1.add_peer("node_3", "127.0.0.1:29013");
    n1.add_peer("node_4", "127.0.0.1:29014");
    n1.add_peer("node_5", "127.0.0.1:29015");
    assert_eq!(n1.quorum_size(), 3);

    // Register all peers on Node 4 (representing minority candidate)
    n4.add_peer("node_1", "127.0.0.1:29011");
    n4.add_peer("node_2", "127.0.0.1:29012");
    n4.add_peer("node_3", "127.0.0.1:29013");
    n4.add_peer("node_5", "127.0.0.1:29015");
    assert_eq!(n4.quorum_size(), 3);

    // Step 1: Node 1 starts election in Majority partition {node_1, node_2, node_3}
    let (term, vote_args) = n1.start_election();
    let reply2 = n2.handle_request_vote(vote_args.clone());
    let reply3 = n3.handle_request_vote(vote_args.clone());

    assert!(reply2.vote_granted);
    assert!(reply3.vote_granted);

    // Record votes: self (1) + node_2 (2) -> not yet quorum, + node_3 (3) -> quorum reached!
    n1.record_vote("node_2", term, reply2.vote_granted);
    let won = n1.record_vote("node_3", term, reply3.vote_granted);

    assert!(won, "Node 1 must achieve majority quorum (3 of 5 votes)");
    assert!(n1.get_info().is_leader, "Node 1 must become Leader");
    println!("  🗳️ [CHAOS-2A] Leader elected with majority quorum: Node 1");

    // Step 2: Leader proposes an entry
    let log_idx = n1
        .propose(
            "TXN_TRANSFER:SG_USD:1000",
            Some(serde_json::json!({ "amount": 1000, "curr": "USD" })),
        )
        .expect("Leader can propose entries");
    assert_eq!(log_idx, 1);

    // Step 3: Network partition isolates Minority {node_4, node_5}
    // Node 4 starts election in the minority partition
    let (term_m, vote_args_m) = n4.start_election();
    let reply5 = n5.handle_request_vote(vote_args_m);
    assert!(reply5.vote_granted);

    // Node 4 receives vote from node_5 (total 2 votes: self + node_5)
    let won_minority = n4.record_vote("node_5", term_m, reply5.vote_granted);

    // Node 4 has 2/5 votes (Less than quorum 3 needed).
    // Node 4 MUST NOT become leader! (Guarantees zero split-brain divergence)
    assert!(
        !won_minority,
        "Minority partition (2 votes) must NEVER achieve quorum in 5-node cluster"
    );
    assert!(
        !n4.get_info().is_leader,
        "Node 4 must remain a Candidate/Follower, never Leader"
    );
    println!("  🛡️ [CHAOS-2B] Split-Brain prevented: Minority partition (2/5) rejected leader promotion");

    // Step 4: Partition heals — Node 4 receives leader's log replication
    let append_args = AppendEntriesArgs {
        term,
        leader_id: "node_1".to_string(),
        prev_log_index: 0,
        prev_log_term: 0,
        entries: vec![LogEntry {
            index: 1,
            term,
            timestamp: chrono::Utc::now(),
            command: "TXN_TRANSFER:SG_USD:1000".to_string(),
            payload: Some(serde_json::json!({ "amount": 1000, "curr": "USD" })),
        }],
        leader_commit: 1,
    };
    let sync_resp4 = n4.handle_append_entries(append_args.clone());
    let sync_resp5 = n5.handle_append_entries(append_args);
    assert!(sync_resp4.success, "Node 4 reconciles log from true leader");
    assert!(sync_resp5.success, "Node 5 reconciles log from true leader");
    println!("  🤝 [CHAOS-2C] Partition healed: Minority nodes reconciled with leader log");
}

// ════════════════════════════════════════════════════════════════════════════
// Pillar 3: Active-Active CRDT Multi-Region Convergence Under Clock Skew
// ════════════════════════════════════════════════════════════════════════════
#[test]
fn test_jepsen_crdt_clock_skew_and_asymmetric_convergence() {
    // 3 Global Datacenters with asymmetric physical clock skew
    let mut reg_sg = CrdtLwwRegister::new("initial_state", 500, "dc_sg");
    let mut reg_eu = CrdtLwwRegister::new("initial_state", 500, "dc_eu");
    let mut reg_us = CrdtLwwRegister::new("initial_state", 500, "dc_us");

    // Simultaneous updates in all 3 regions
    reg_sg.update("sg_write", 1000, "dc_sg");
    reg_eu.update("eu_write", 1500, "dc_eu");
    reg_us.update("us_write", 3500, "dc_us");

    // Asymmetric gossip propagation: SG merges EU first, then US merges SG
    reg_sg.merge(&reg_eu);
    assert_eq!(reg_sg.value, "eu_write");

    reg_eu.merge(&reg_us);
    assert_eq!(reg_eu.value, "us_write");

    reg_sg.merge(&reg_eu);
    reg_us.merge(&reg_sg);

    // Strict Convergence across all regions
    assert_eq!(reg_sg.value, "us_write");
    assert_eq!(reg_eu.value, "us_write");
    assert_eq!(reg_us.value, "us_write");
    println!("  ⚡ [CHAOS-3] Asymmetric CRDT gossip converged to definitive state: us_write");
}

// ════════════════════════════════════════════════════════════════════════════
// Pillar 4: LSM-Tree Compaction Anti-Stall Engine
// ════════════════════════════════════════════════════════════════════════════
#[test]
fn test_jepsen_lsm_anti_stall_and_compaction_guard() {
    let dir = tempdir().unwrap();
    let data_dir = dir.path().to_path_buf();

    // Configure engine with aggressive compaction trigger (3 L0 tables) and small memtable
    let config = StorageConfig {
        data_dir: data_dir.clone(),
        memtable_size: 256, // Flush frequently to test SSTable compaction bounding
        sync_writes: false,
        enable_wal: true,
        l0_compaction_trigger: 3,
        l0_slowdown_writes_trigger: 6,
        l0_stop_writes_trigger: 10,
        ..Default::default()
    };
    let engine = StorageEngine::open(config).expect("Failed to open engine");

    // Ingest 300 records causing rapid multiple memtable flushes
    for i in 0..300 {
        let k = format!("sensor:reading:{i:04}");
        let v = format!("val:{i:08}:temperature_celsius:24.5");
        engine.put(k.as_bytes(), v.as_bytes()).unwrap();
    }

    let stats = engine.stats();
    println!(
        "  📊 [CHAOS-4] LSM stats after heavy write burst: SSTables={}, Compactions={}, Write Stalls={}",
        stats.sstable_count, stats.compactions_completed, stats.write_stalls
    );

    // Compaction must have run automatically
    assert!(
        stats.compactions_completed >= 1,
        "Compaction must execute to bound SSTable depth"
    );
    // SSTable count must be bounded safely below stop trigger (10)
    assert!(
        stats.sstable_count < 10,
        "SSTable count must remain bounded below stop trigger"
    );

    // Verify all 300 records are readable with 0 loss
    for i in 0..300 {
        let k = format!("sensor:reading:{i:04}");
        let expected_v = format!("val:{i:08}:temperature_celsius:24.5");
        let val = engine.get(k.as_bytes()).unwrap();
        assert_eq!(val, Some(expected_v.into_bytes()));
    }
    println!("  🚀 [CHAOS-4] Anti-Stall verification passed: 100% data intact across compacted SSTables");
}

// ════════════════════════════════════════════════════════════════════════════
// Pillar 5: Postgres Virtual System Catalog Introspection (pg_catalog)
// ════════════════════════════════════════════════════════════════════════════
#[test]
fn test_jepsen_postgres_system_catalog_introspection() {
    let db = Arc::new(DatabaseContext::new());
    db.get_or_create_collection("customers");
    db.get_or_create_collection("orders");

    let mut in_txn = false;

    // 1. pg_database
    let resp = handle_postgres_query(&db, "SELECT datname, oid FROM pg_catalog.pg_database", &mut in_txn);
    let s = String::from_utf8_lossy(&resp);
    assert!(s.contains("faizdb"), "pg_database must return faizdb database");
    assert!(s.contains("datname"));

    // 2. pg_namespace
    let resp_ns = handle_postgres_query(&db, "SELECT nspname, oid FROM pg_catalog.pg_namespace", &mut in_txn);
    let s_ns = String::from_utf8_lossy(&resp_ns);
    assert!(s_ns.contains("public"));
    assert!(s_ns.contains("pg_catalog"));
    assert!(s_ns.contains("information_schema"));

    // 3. pg_type
    let resp_types = handle_postgres_query(&db, "SELECT typname, oid, typarray FROM pg_catalog.pg_type", &mut in_txn);
    let s_types = String::from_utf8_lossy(&resp_types);
    assert!(s_types.contains("bool"));
    assert!(s_types.contains("int8"));
    assert!(s_types.contains("text"));
    assert!(s_types.contains("jsonb"));
    assert!(s_types.contains("vector"));

    // 4. information_schema.columns
    let resp_cols = handle_postgres_query(&db, "SELECT table_name, column_name FROM information_schema.columns", &mut in_txn);
    let s_cols = String::from_utf8_lossy(&resp_cols);
    assert!(s_cols.contains("customers"));
    assert!(s_cols.contains("orders"));
    assert!(s_cols.contains("_id"));

    println!("  🐘 [CHAOS-5] PostgreSQL system catalog introspection passed (pg_database, pg_namespace, pg_type, columns)");
}
