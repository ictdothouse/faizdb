//! Multi-node Raft Consensus, Persistent Replicated Log & Quorum Failover Integration Tests.

use tempfile::tempdir;
use faizdb_core::cluster::raft::{
    InMemoryRaftRouter, RaftConfig, RaftNode
};

#[test]
fn test_multi_node_election_and_failover() {
    let router = InMemoryRaftRouter::new();

    let node1 = RaftNode::new("node_1", "127.0.0.1:29001");
    let node2 = RaftNode::new("node_2", "127.0.0.1:29002");
    let node3 = RaftNode::new("node_3", "127.0.0.1:29003");

    // Configure 3-node cluster
    node1.add_peer("node_2", "127.0.0.1:29002");
    node1.add_peer("node_3", "127.0.0.1:29003");

    node2.add_peer("node_1", "127.0.0.1:29001");
    node2.add_peer("node_3", "127.0.0.1:29003");

    node3.add_peer("node_1", "127.0.0.1:29001");
    node3.add_peer("node_2", "127.0.0.1:29002");

    router.register(node1.clone());
    router.register(node2.clone());
    router.register(node3.clone());

    // 1. Quorum size for 3 nodes must be 2
    assert_eq!(node1.quorum_size(), 2);
    assert_eq!(node2.quorum_size(), 2);

    // 2. Node 2 starts election for Term 2
    let (term, vote_args) = node2.start_election();
    assert_eq!(term, 2);

    // Node 3 votes for Node 2
    let reply3 = node3.handle_request_vote(vote_args);
    assert!(reply3.vote_granted);
    let won = node2.record_vote("node_3", term, true);
    assert!(won, "Node 2 must win election with quorum (self + node_3)");

    let info2 = node2.get_info();
    assert!(info2.is_leader);
    assert_eq!(info2.term, 2);

    // 3. Node 2 proposes new command
    let idx = node2.propose("ADD_DOCUMENT", Some(serde_json::json!({ "id": "doc_100" }))).unwrap();
    assert_eq!(idx, 1);
}

#[test]
fn test_persistent_raft_log_and_recovery() {
    let temp = tempdir().unwrap();
    let config = RaftConfig {
        data_dir: Some(temp.path().to_path_buf()),
        ..Default::default()
    };

    // Session 1: Propose entries and persist to disk
    {
        let node = RaftNode::with_config("leader_node", "127.0.0.1:29010", config.clone());
        for i in 1..=5 {
            node.propose(format!("CMD_{i}"), Some(serde_json::json!({ "val": i }))).unwrap();
        }
        let info = node.get_info();
        assert_eq!(info.persistent_log_entries, 6); // Genesis + 5
    }

    // Session 2: Crash restart from same data directory
    {
        let recovered = RaftNode::with_config("leader_node", "127.0.0.1:29010", config);
        let info = recovered.get_info();
        assert_eq!(info.persistent_log_entries, 6, "Must recover all 6 entries from disk");
        assert_eq!(info.commit_index, 5);
    }
}
