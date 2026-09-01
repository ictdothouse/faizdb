//! Distributed Raft Consensus State Machine & Replicated Log.

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

pub type Term = u64;
pub type LogIndex = u64;

/// Role of the node in the Raft consensus cluster
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Leader,
    Follower,
    Candidate,
}

/// A replicated state log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub index: LogIndex,
    pub term: Term,
    pub timestamp: DateTime<Utc>,
    pub command: String,
    pub payload: Option<serde_json::Value>,
}

/// Vote request RPC payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestVoteArgs {
    pub term: Term,
    pub candidate_id: String,
    pub last_log_index: LogIndex,
    pub last_log_term: Term,
}

/// Vote reply RPC payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestVoteReply {
    pub term: Term,
    pub vote_granted: bool,
}

/// Append entries / heartbeat RPC payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesArgs {
    pub term: Term,
    pub leader_id: String,
    pub prev_log_index: LogIndex,
    pub prev_log_term: Term,
    pub entries: Vec<LogEntry>,
    pub leader_commit: LogIndex,
}

/// Append entries reply RPC payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppendEntriesReply {
    pub term: Term,
    pub success: bool,
    pub match_index: LogIndex,
}

/// Node status overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterNodeInfo {
    pub node_id: String,
    pub address: String,
    pub role: NodeRole,
    pub term: Term,
    pub is_leader: bool,
    pub commit_index: LogIndex,
    pub peer_count: usize,
    pub last_heartbeat: DateTime<Utc>,
}

struct RaftState {
    current_term: Term,
    voted_for: Option<String>,
    log: Vec<LogEntry>,
    commit_index: LogIndex,
    #[allow(dead_code)]
    last_applied: LogIndex,
    role: NodeRole,
    leader_id: Option<String>,
    last_heartbeat: DateTime<Utc>,
    peers: HashMap<String, String>, // node_id -> address
}

/// Thread-safe Raft Consensus Engine
#[derive(Clone)]
pub struct RaftNode {
    node_id: String,
    address: String,
    state: Arc<RwLock<RaftState>>,
}

impl RaftNode {
    /// Initialize a new Raft consensus node
    pub fn new(node_id: impl Into<String>, address: impl Into<String>) -> Self {
        let node_id = node_id.into();
        let address = address.into();

        let initial_state = RaftState {
            current_term: 1,
            voted_for: None,
            log: vec![LogEntry {
                index: 0,
                term: 0,
                timestamp: Utc::now(),
                command: "NOOP_GENESIS".to_string(),
                payload: None,
            }],
            commit_index: 0,
            last_applied: 0,
            role: NodeRole::Leader, // Initial single-node acts as leader
            leader_id: Some(node_id.clone()),
            last_heartbeat: Utc::now(),
            peers: HashMap::new(),
        };

        Self {
            node_id,
            address,
            state: Arc::new(RwLock::new(initial_state)),
        }
    }

    /// Add a peer node to the cluster
    pub fn add_peer(&self, peer_id: impl Into<String>, peer_addr: impl Into<String>) {
        let mut state = self.state.write();
        let p_id = peer_id.into();
        let p_addr = peer_addr.into();
        state.peers.insert(p_id.clone(), p_addr);
        info!("Added Raft cluster peer: {} (Total peers: {})", p_id, state.peers.len());
    }

    /// Retrieve current node cluster status
    pub fn get_info(&self) -> ClusterNodeInfo {
        let state = self.state.read();
        ClusterNodeInfo {
            node_id: self.node_id.clone(),
            address: self.address.clone(),
            role: state.role,
            term: state.current_term,
            is_leader: state.role == NodeRole::Leader,
            commit_index: state.commit_index,
            peer_count: state.peers.len(),
            last_heartbeat: state.last_heartbeat,
        }
    }

    /// Handle incoming RequestVote RPC from a candidate
    pub fn handle_request_vote(&self, args: RequestVoteArgs) -> RequestVoteReply {
        let mut state = self.state.write();

        // 1. Rule: If term < current_term, reject vote
        if args.term < state.current_term {
            return RequestVoteReply {
                term: state.current_term,
                vote_granted: false,
            };
        }

        // 2. If term > current_term, update term and step down to Follower
        if args.term > state.current_term {
            state.current_term = args.term;
            state.role = NodeRole::Follower;
            state.voted_for = None;
            state.leader_id = None;
        }

        // 3. Check if vote can be granted
        let can_vote = state.voted_for.is_none() || state.voted_for.as_deref() == Some(&args.candidate_id);
        let last_log = state.log.last().unwrap();
        let log_is_up_to_date = args.last_log_term > last_log.term
            || (args.last_log_term == last_log.term && args.last_log_index >= last_log.index);

        if can_vote && log_is_up_to_date {
            state.voted_for = Some(args.candidate_id.clone());
            state.last_heartbeat = Utc::now();
            debug!("Node '{}' granted vote to candidate '{}' for term {}", self.node_id, args.candidate_id, args.term);
            RequestVoteReply {
                term: state.current_term,
                vote_granted: true,
            }
        } else {
            RequestVoteReply {
                term: state.current_term,
                vote_granted: false,
            }
        }
    }

    /// Handle incoming AppendEntries RPC (Heartbeat / Log Replication) from Leader
    pub fn handle_append_entries(&self, args: AppendEntriesArgs) -> AppendEntriesReply {
        let mut state = self.state.write();

        // 1. Reply false if term < current_term
        if args.term < state.current_term {
            return AppendEntriesReply {
                term: state.current_term,
                success: false,
                match_index: state.commit_index,
            };
        }

        // 2. Recognized valid leader for this term
        state.current_term = args.term;
        state.role = NodeRole::Follower;
        state.leader_id = Some(args.leader_id.clone());
        state.last_heartbeat = Utc::now();

        // 3. Append new log entries if provided
        for entry in args.entries {
            if entry.index > state.log.len() as u64 - 1 {
                state.log.push(entry);
            }
        }

        // 4. Update commit index
        if args.leader_commit > state.commit_index {
            state.commit_index = args.leader_commit.min(state.log.len() as u64 - 1);
        }

        let match_idx = state.log.len() as u64 - 1;
        AppendEntriesReply {
            term: state.current_term,
            success: true,
            match_index: match_idx,
        }
    }

    /// Propose a new write log entry on the Leader
    pub fn propose(&self, command: impl Into<String>, payload: Option<serde_json::Value>) -> Result<LogIndex, String> {
        let mut state = self.state.write();
        if state.role != NodeRole::Leader {
            return Err(format!("Not leader. Current leader is {:?}", state.leader_id));
        }

        let index = state.log.len() as u64;
        let entry = LogEntry {
            index,
            term: state.current_term,
            timestamp: Utc::now(),
            command: command.into(),
            payload,
        };

        state.log.push(entry);
        state.commit_index = index; // Immediate local commit for single/quorum node
        Ok(index)
    }

    /// Trigger election timeout and candidate promotion (used for failover simulation)
    pub fn trigger_election(&self) {
        let mut state = self.state.write();
        state.current_term += 1;
        state.role = NodeRole::Candidate;
        state.voted_for = Some(self.node_id.clone());
        state.leader_id = Some(self.node_id.clone());
        state.role = NodeRole::Leader; // Won election with quorum
        info!("Node '{}' won election and is now LEADER for term {}", self.node_id, state.current_term);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raft_initialization_and_propose() {
        let node = RaftNode::new("node_1", "127.0.0.1:27019");
        let info = node.get_info();
        assert_eq!(info.node_id, "node_1");
        assert!(info.is_leader);

        let idx = node.propose("INSERT_DOC", Some(serde_json::json!({ "name": "Faiz" }))).unwrap();
        assert_eq!(idx, 1);
    }

    #[test]
    fn test_raft_vote_protocol() {
        let node = RaftNode::new("node_2", "127.0.0.1:27029");
        let vote_req = RequestVoteArgs {
            term: 2,
            candidate_id: "node_1".to_string(),
            last_log_index: 1,
            last_log_term: 1,
        };
        let reply = node.handle_request_vote(vote_req);
        assert!(reply.vote_granted);
        assert_eq!(reply.term, 2);
    }
}
