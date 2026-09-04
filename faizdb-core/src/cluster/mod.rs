//! Distributed Cluster, Raft Consensus & Auto-Sharding Module.

pub mod crdt;
pub mod geo;
pub mod raft;
pub mod sharding;

pub use crdt::{CrdtDocument, LwwRegister, OrSet, PnCounter, VersionVector};
pub use geo::{GeoReplicationEngine, RegionConfig, ReplicationDelta};
pub use raft::{
    AppendEntriesArgs, AppendEntriesReply, ClusterNodeInfo, InMemoryRaftRouter, LogEntry, LogIndex,
    NodeRole, RaftConfig, RaftDiskStore, RaftNode, RaftRpcTransport, RaftTickAction,
    RequestVoteArgs, RequestVoteReply, Term,
};
pub use sharding::{ShardDistribution, ShardRange, ShardRouter, TOTAL_SHARD_SLOTS};
