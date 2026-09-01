//! Distributed Cluster, Raft Consensus & Auto-Sharding Module.

pub mod raft;
pub mod sharding;
pub mod crdt;
pub mod geo;

pub use raft::{RaftNode, NodeRole, ClusterNodeInfo, LogEntry, RequestVoteArgs, RequestVoteReply, AppendEntriesArgs, AppendEntriesReply};
pub use sharding::{ShardRouter, ShardDistribution, ShardRange, TOTAL_SHARD_SLOTS};
pub use crdt::{VersionVector, LwwRegister, OrSet, PnCounter, CrdtDocument};
pub use geo::{GeoReplicationEngine, RegionConfig, ReplicationDelta};
