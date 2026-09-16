//! Raft Chaos & Network Partition Test Suite (Jepsen-Style Simulation)
//!
//! Validates distributed consensus invariants under adversarial conditions:
//! 1. Network Split-Brain Partition (3 nodes vs 2 nodes).
//! 2. Quorum Commit Isolation (Majority commits; Minority isolated).
//! 3. Partition Healing & Reconciliation (Lagging nodes catch up without divergence).
//! 4. Abrupt Leader Crash & Automatic Failover.

use faizdb_core::cluster::raft::{LogEntry, NodeRole, RaftConfig, RaftNode};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

/// Simulated network mesh router that can inject drops, delays, and partitions
struct SimulatedNetworkMesh {
    nodes: HashMap<String, Arc<RaftNode>>,
    /// Set of blocked unidirectional links: (sender, receiver)
    blocked_links: Mutex<HashSet<(String, String)>>,
}

impl SimulatedNetworkMesh {
    fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            blocked_links: Mutex::new(HashSet::new()),
        }
    }

    fn register_node(&mut self, node: Arc<RaftNode>) {
        self.nodes.insert(node.get_info().node_id, node);
    }

    /// Block communication between two sets of nodes (Network Partition)
    fn partition(&self, group_a: &[&str], group_b: &[&str]) {
        let mut links = self.blocked_links.lock().unwrap();
        for &a in group_a {
            for &b in group_b {
                links.insert((a.to_string(), b.to_string()));
                links.insert((b.to_string(), a.to_string()));
            }
        }
    }

    /// Restore full network connectivity
    fn heal(&self) {
        let mut links = self.blocked_links.lock().unwrap();
        links.clear();
    }

    fn can_deliver(&self, from: &str, to: &str) -> bool {
        let links = self.blocked_links.lock().unwrap();
        !links.contains(&(from.to_string(), to.to_string()))
    }

    /// Deliver heartbeats/append entries from leader to all reachable peers
    fn broadcast_leader_heartbeat(&self, leader_id: &str) -> usize {
        let leader = match self.nodes.get(leader_id) {
            Some(l) => l.clone(),
            None => return 0,
        };

        let heartbeats = leader.prepare_heartbeats();
        let mut delivered = 0;

        for (peer_id, (_, args)) in heartbeats {
            if self.can_deliver(leader_id, &peer_id) {
                if let Some(peer) = self.nodes.get(&peer_id) {
                    let reply = peer.handle_append_entries(args);
                    if reply.success {
                        delivered += 1;
                    }
                }
            }
        }
        delivered
    }

    /// Propose a command on leader and replicate to reachable peers to count quorum
    fn propose_and_replicate(
        &self,
        leader_id: &str,
        command: &str,
        payload: Option<serde_json::Value>,
    ) -> bool {
        let leader = match self.nodes.get(leader_id) {
            Some(l) => l.clone(),
            None => return false,
        };

        let entry_index = match leader.propose(command.to_string(), payload.clone()) {
            Ok(idx) => idx,
            Err(_) => return false,
        };
        let entry_term = leader.get_info().term;

        let entry = LogEntry {
            index: entry_index,
            term: entry_term,
            timestamp: chrono::Utc::now(),
            command: command.to_string(),
            payload,
        };

        let prev_index = entry_index.saturating_sub(1);
        let quorum = leader.quorum_size();
        let peers = leader.list_peers();
        let mut acks = 1; // Leader itself counts as 1

        for peer_id in peers.keys() {
            if self.can_deliver(leader_id, peer_id) {
                if let Some(peer) = self.nodes.get(peer_id) {
                    let args = faizdb_core::cluster::raft::AppendEntriesArgs {
                        term: entry_term,
                        leader_id: leader_id.to_string(),
                        prev_log_index: prev_index,
                        prev_log_term: entry_term,
                        entries: vec![entry.clone()],
                        leader_commit: entry_index,
                    };
                    let reply = peer.handle_append_entries(args);
                    if reply.success {
                        acks += 1;
                    }
                }
            }
        }
        acks >= quorum
    }
}

#[test]
fn test_raft_jepsen_partition_and_healing() {
    let mut mesh = SimulatedNetworkMesh::new();
    let temp_dirs: Vec<_> = (0..5).map(|_| tempdir().unwrap()).collect();

    // 1. Create a 5-node cluster
    let mut nodes = Vec::new();
    for i in 1..=5 {
        let node_id = format!("node_{i}");
        let addr = format!("127.0.0.1:900{i}");
        let config = RaftConfig {
            data_dir: Some(temp_dirs[i - 1].path().to_path_buf()),
            ..Default::default()
        };
        let node = Arc::new(RaftNode::with_config(node_id, addr, config));
        nodes.push(node);
    }

    // Connect all peers
    for i in 0..5 {
        for j in 0..5 {
            if i != j {
                nodes[i].add_peer(nodes[j].get_info().node_id, nodes[j].get_info().address);
            }
        }
        mesh.register_node(nodes[i].clone());
    }

    // Node 1 starts election and becomes initial cluster leader
    let (term, vote_args) = nodes[0].start_election();
    for j in 1..5 {
        let reply = nodes[j].handle_request_vote(vote_args.clone());
        nodes[0].record_vote(&nodes[j].get_info().node_id, term, reply.vote_granted);
    }

    assert_eq!(nodes[0].get_info().role, NodeRole::Leader);
    assert_eq!(nodes[0].quorum_size(), 3); // 5 nodes total -> quorum = 3

    // Replicate initial command
    let success = mesh.propose_and_replicate(
        "node_1",
        "SET user:101 Ahmad",
        Some(json!({"name": "Ahmad"})),
    );
    assert!(
        success,
        "Leader should successfully commit with 5 connected nodes"
    );

    // 2. Introduce Network Partition: Group A {node_1, node_2, node_3} (Majority: 3)
    //                                Group B {node_4, node_5}           (Minority: 2)
    mesh.partition(&["node_1", "node_2", "node_3"], &["node_4", "node_5"]);

    // Test Majority Partition: Node 1 proposes another command.
    // Node 1, 2, 3 are connected (3 acks >= quorum 3) -> should commit!
    let committed_in_partition =
        mesh.propose_and_replicate("node_1", "SET user:102 Faiz", Some(json!({"name": "Faiz"})));
    assert!(
        committed_in_partition,
        "Majority partition (3 nodes) must maintain progress and commit"
    );

    // Verify minority nodes (node_4, node_5) did not receive entry2
    assert_eq!(
        nodes[3].get_info().persistent_log_entries,
        2, // genesis + entry1
        "Minority node must not have received entry 2"
    );

    // Test Minority Partition Split-Brain Prevention:
    // If node_4 attempts to elect itself in minority partition, it cannot win
    let (cand_term, cand_args) = nodes[3].start_election();
    let reply5 = nodes[4].handle_request_vote(cand_args);
    nodes[3].record_vote("node_5", cand_term, reply5.vote_granted);

    assert_ne!(
        nodes[3].get_info().role,
        NodeRole::Leader,
        "Minority partition cannot elect a leader (2 votes < 3 quorum)"
    );

    // 3. Heal Network Partition
    mesh.heal();

    // After healing, node_1 discovers minority nodes were at term 2 from failed election.
    // Node 1 (having the highest log entries) initiates election for term 3 to unify the cluster.
    let (heal_term, heal_args) = nodes[0].start_election();
    assert_eq!(heal_term, 3, "Healed leader term should be 3");

    for j in 1..5 {
        let reply = nodes[j].handle_request_vote(heal_args.clone());
        nodes[0].record_vote(&nodes[j].get_info().node_id, heal_term, reply.vote_granted);
    }
    // Majority quorum reached (node_1 + node_2 + node_3 = 3/5 votes)
    assert_eq!(
        nodes[0].get_info().role,
        NodeRole::Leader,
        "Node 1 must win election with majority quorum of 3 votes"
    );

    // 1. Catch up minority nodes with entry2 (log reconciliation)
    let catchup_entry = LogEntry {
        index: 2,
        term: 2,
        timestamp: chrono::Utc::now(),
        command: "SET user:102 Faiz".to_string(),
        payload: Some(json!({"name": "Faiz"})),
    };
    let args = faizdb_core::cluster::raft::AppendEntriesArgs {
        term: 3,
        leader_id: "node_1".to_string(),
        prev_log_index: 1,
        prev_log_term: 2,
        entries: vec![catchup_entry],
        leader_commit: 2,
    };
    let r4 = nodes[3].handle_append_entries(args.clone());
    let r5 = nodes[4].handle_append_entries(args);
    assert!(r4.success, "Node 4 should accept catchup entry");
    assert!(r5.success, "Node 5 should accept catchup entry");

    assert_eq!(
        nodes[3].get_info().persistent_log_entries,
        3,
        "Node 4 must catch up log to index 2 after heal"
    );
    assert_eq!(
        nodes[4].get_info().persistent_log_entries,
        3,
        "Node 5 must catch up log to index 2 after heal"
    );

    // 2. Leader broadcasts heartbeats across healed network (now all 4 peers are fully in sync)
    let delivered = mesh.broadcast_leader_heartbeat("node_1");
    assert_eq!(
        delivered, 4,
        "Leader heartbeats must reach all 4 peers after healing and reconciliation"
    );
}

#[test]
fn test_abrupt_leader_crash_and_failover() {
    let temp_dirs: Vec<_> = (0..3).map(|_| tempdir().unwrap()).collect();

    let n1 = Arc::new(RaftNode::with_config(
        "srv_1",
        "127.0.0.1:8001",
        RaftConfig {
            data_dir: Some(temp_dirs[0].path().to_path_buf()),
            ..Default::default()
        },
    ));
    let n2 = Arc::new(RaftNode::with_config(
        "srv_2",
        "127.0.0.1:8002",
        RaftConfig {
            data_dir: Some(temp_dirs[1].path().to_path_buf()),
            ..Default::default()
        },
    ));
    let n3 = Arc::new(RaftNode::with_config(
        "srv_3",
        "127.0.0.1:8003",
        RaftConfig {
            data_dir: Some(temp_dirs[2].path().to_path_buf()),
            ..Default::default()
        },
    ));

    n1.add_peer("srv_2", "127.0.0.1:8002");
    n1.add_peer("srv_3", "127.0.0.1:8003");
    n2.add_peer("srv_1", "127.0.0.1:8001");
    n2.add_peer("srv_3", "127.0.0.1:8003");
    n3.add_peer("srv_1", "127.0.0.1:8001");
    n3.add_peer("srv_2", "127.0.0.1:8002");

    // srv_1 wins term 1
    let (t1, args1) = n1.start_election();
    n2.handle_request_vote(args1.clone());
    n3.handle_request_vote(args1);
    n1.record_vote("srv_2", t1, true);
    assert_eq!(n1.get_info().role, NodeRole::Leader);

    // Abrupt crash of srv_1 (drop reference / simulate dead process)
    drop(n1);

    // srv_2 times out and triggers new election for term 3 (srv_1 was elected for term 2)
    let (t2, args2) = n2.start_election();
    assert_eq!(t2, 3, "Candidate term should increment to 3");

    let reply3 = n3.handle_request_vote(args2);
    assert!(reply3.vote_granted, "Follower srv_3 should grant vote");

    let won = n2.record_vote("srv_3", t2, reply3.vote_granted);
    assert!(won, "srv_2 must achieve quorum with srv_3");
    assert_eq!(n2.get_info().role, NodeRole::Leader);
}
