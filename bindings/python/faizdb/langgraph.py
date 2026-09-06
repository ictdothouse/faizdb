"""
🔥 Native FaizDB Checkpointer for LangGraph.

Enables LangGraph to persist agent state directly into FaizDB's native
document storage engine (Port 27018 / gRPC 50051) without requiring
PostgreSQL, Redis, or any external wire bridge.

Usage:
    from faizdb import FaizDB
    from faizdb.langgraph import FaizDbSaver
    from langgraph.graph import StateGraph

    # 1. Connect natively to FaizDB
    db = FaizDB("http://localhost:27018")
    checkpointer = FaizDbSaver(db)

    # 2. Compile LangGraph workflow with 100% native FaizDB persistence
    app = workflow.compile(checkpointer=checkpointer)

    # 3. Sub-millisecond snapshot persistence & time-travel enabled!
    config = {"configurable": {"thread_id": "session_01"}}
    app.invoke({"input": "Hello"}, config)
"""

import json
import time
from typing import Any, Dict, Iterator, List, Optional, Sequence, Tuple

try:
    from langgraph.checkpoint.base import (
        BaseCheckpointSaver,
        Checkpoint,
        CheckpointMetadata,
        CheckpointTuple,
        ChannelVersions,
    )
    from langchain_core.runnables import RunnableConfig
    HAS_LANGGRAPH = True
except ImportError:
    # Graceful fallback if langgraph is not yet installed in runtime
    HAS_LANGGRAPH = False
    BaseCheckpointSaver = object  # type: ignore
    Checkpoint = Dict[str, Any]  # type: ignore
    CheckpointMetadata = Dict[str, Any]  # type: ignore
    CheckpointTuple = Any  # type: ignore
    ChannelVersions = Dict[str, Any]  # type: ignore
    RunnableConfig = Dict[str, Any]  # type: ignore


class FaizDbSaver(BaseCheckpointSaver):
    """
    100% Native FaizDB Checkpoint Saver for LangGraph.
    
    Persists agent thread checkpoints, channel versions, and pending writes
    directly into native FaizDB collections with sub-millisecond ACID durability.
    """

    def __init__(
        self,
        client: Any,
        collection_prefix: str = "langgraph",
    ):
        if HAS_LANGGRAPH:
            super().__init__()
        self.client = client
        self.checkpoints = client.collection(f"{collection_prefix}_checkpoints")
        self.writes = client.collection(f"{collection_prefix}_writes")
        
        # Ensure B-Tree index on thread_id and checkpoint_id for instant retrieval
        try:
            self.checkpoints.create_index("thread_id")
            self.checkpoints.create_index("checkpoint_id")
        except Exception:
            pass

    def get_tuple(self, config: RunnableConfig) -> Optional[CheckpointTuple]:
        """Fetch the latest or specified checkpoint tuple for a thread."""
        configurable = config.get("configurable", {})
        thread_id = configurable.get("thread_id")
        checkpoint_ns = configurable.get("checkpoint_ns", "")
        checkpoint_id = configurable.get("checkpoint_id")

        if not thread_id:
            return None

        query_filter: Dict[str, Any] = {
            "thread_id": thread_id,
            "checkpoint_ns": checkpoint_ns,
        }
        if checkpoint_id:
            query_filter["checkpoint_id"] = checkpoint_id

        docs = self.checkpoints.find(query_filter)
        if not docs:
            return None

        # Sort descending by checkpoint_id / timestamp to get latest
        docs.sort(key=lambda d: d.get("checkpoint_id", ""), reverse=True)
        doc = docs[0]

        # Retrieve pending writes for this checkpoint
        writes_filter = {
            "thread_id": thread_id,
            "checkpoint_ns": checkpoint_ns,
            "checkpoint_id": doc["checkpoint_id"],
        }
        pending_writes_docs = self.writes.find(writes_filter)
        pending_writes = [
            (w["task_id"], w["channel"], json.loads(w["value_json"]))
            for w in pending_writes_docs
        ]

        checkpoint_data = json.loads(doc["checkpoint_json"])
        metadata_data = json.loads(doc.get("metadata_json", "{}"))
        parent_config = None
        if doc.get("parent_checkpoint_id"):
            parent_config = {
                "configurable": {
                    "thread_id": thread_id,
                    "checkpoint_ns": checkpoint_ns,
                    "checkpoint_id": doc["parent_checkpoint_id"],
                }
            }

        if HAS_LANGGRAPH:
            return CheckpointTuple(
                config={
                    "configurable": {
                        "thread_id": thread_id,
                        "checkpoint_ns": checkpoint_ns,
                        "checkpoint_id": doc["checkpoint_id"],
                    }
                },
                checkpoint=checkpoint_data,
                metadata=metadata_data,
                parent_config=parent_config,
                pending_writes=pending_writes,
            )
        return {
            "config": {
                "configurable": {
                    "thread_id": thread_id,
                    "checkpoint_ns": checkpoint_ns,
                    "checkpoint_id": doc["checkpoint_id"],
                }
            },
            "checkpoint": checkpoint_data,
            "metadata": metadata_data,
            "parent_config": parent_config,
            "pending_writes": pending_writes,
        }

    def list(
        self,
        config: Optional[RunnableConfig],
        *,
        filter: Optional[Dict[str, Any]] = None,
        before: Optional[RunnableConfig] = None,
        limit: Optional[int] = None,
    ) -> Iterator[CheckpointTuple]:
        """List checkpoint history for audit, time-travel, or visualization."""
        query_filter: Dict[str, Any] = {}
        if config and "configurable" in config:
            configurable = config["configurable"]
            if "thread_id" in configurable:
                query_filter["thread_id"] = configurable["thread_id"]
            if "checkpoint_ns" in configurable:
                query_filter["checkpoint_ns"] = configurable["checkpoint_ns"]

        if filter:
            query_filter.update(filter)

        docs = self.checkpoints.find(query_filter)
        docs.sort(key=lambda d: d.get("checkpoint_id", ""), reverse=True)

        count = 0
        for doc in docs:
            if before and "configurable" in before:
                before_id = before["configurable"].get("checkpoint_id")
                if before_id and doc["checkpoint_id"] >= before_id:
                    continue

            checkpoint_data = json.loads(doc["checkpoint_json"])
            metadata_data = json.loads(doc.get("metadata_json", "{}"))
            parent_config = None
            if doc.get("parent_checkpoint_id"):
                parent_config = {
                    "configurable": {
                        "thread_id": doc["thread_id"],
                        "checkpoint_ns": doc.get("checkpoint_ns", ""),
                        "checkpoint_id": doc["parent_checkpoint_id"],
                    }
                }

            item = None
            if HAS_LANGGRAPH:
                item = CheckpointTuple(
                    config={
                        "configurable": {
                            "thread_id": doc["thread_id"],
                            "checkpoint_ns": doc.get("checkpoint_ns", ""),
                            "checkpoint_id": doc["checkpoint_id"],
                        }
                    },
                    checkpoint=checkpoint_data,
                    metadata=metadata_data,
                    parent_config=parent_config,
                )
            else:
                item = {
                    "config": {
                        "configurable": {
                            "thread_id": doc["thread_id"],
                            "checkpoint_ns": doc.get("checkpoint_ns", ""),
                            "checkpoint_id": doc["checkpoint_id"],
                        }
                    },
                    "checkpoint": checkpoint_data,
                    "metadata": metadata_data,
                    "parent_config": parent_config,
                }
            yield item
            count += 1
            if limit and count >= limit:
                break

    def put(
        self,
        config: RunnableConfig,
        checkpoint: Checkpoint,
        metadata: CheckpointMetadata,
        new_versions: ChannelVersions,
    ) -> RunnableConfig:
        """Persist a new checkpoint snapshot into FaizDB."""
        configurable = config.get("configurable", {})
        thread_id = configurable.get("thread_id")
        checkpoint_ns = configurable.get("checkpoint_ns", "")
        checkpoint_id = checkpoint.get("id")

        if not thread_id or not checkpoint_id:
            raise ValueError("Thread ID and Checkpoint ID are required to persist checkpoint")

        parent_checkpoint_id = configurable.get("checkpoint_id")

        doc = {
            "thread_id": thread_id,
            "checkpoint_ns": checkpoint_ns,
            "checkpoint_id": checkpoint_id,
            "parent_checkpoint_id": parent_checkpoint_id,
            "checkpoint_json": json.dumps(checkpoint),
            "metadata_json": json.dumps(metadata if metadata else {}),
            "created_at": time.time(),
        }

        self.checkpoints.insert(doc)

        return {
            "configurable": {
                "thread_id": thread_id,
                "checkpoint_ns": checkpoint_ns,
                "checkpoint_id": checkpoint_id,
            }
        }

    def put_writes(
        self,
        config: RunnableConfig,
        writes: Sequence[Tuple[str, Any]],
        task_id: str,
    ) -> None:
        """Store pending channel writes for this step."""
        configurable = config.get("configurable", {})
        thread_id = configurable.get("thread_id")
        checkpoint_ns = configurable.get("checkpoint_ns", "")
        checkpoint_id = configurable.get("checkpoint_id")

        if not thread_id or not checkpoint_id:
            return

        for channel, value in writes:
            write_doc = {
                "thread_id": thread_id,
                "checkpoint_ns": checkpoint_ns,
                "checkpoint_id": checkpoint_id,
                "task_id": task_id,
                "channel": channel,
                "value_json": json.dumps(value),
                "created_at": time.time(),
            }
            self.writes.insert(write_doc)
