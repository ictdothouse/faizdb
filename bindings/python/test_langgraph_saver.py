"""
Unit test for native FaizDbSaver LangGraph checkpointer.
"""
import unittest
from faizdb.langgraph import FaizDbSaver


class MockCollection:
    def __init__(self):
        self.docs = []

    def create_index(self, field, unique=False):
        pass

    def insert(self, doc):
        doc_copy = dict(doc)
        if "id" not in doc_copy:
            doc_copy["id"] = f"doc_{len(self.docs) + 1}"
        self.docs.append(doc_copy)
        return doc_copy["id"]

    def find(self, query):
        results = []
        for d in self.docs:
            match = True
            for k, v in query.items():
                if d.get(k) != v:
                    match = False
                    break
            if match:
                results.append(d)
        return results


class MockFaizClient:
    def __init__(self):
        self.collections = {}

    def collection(self, name):
        if name not in self.collections:
            self.collections[name] = MockCollection()
        return self.collections[name]


class TestFaizDbSaver(unittest.TestCase):
    def setUp(self):
        self.client = MockFaizClient()
        self.saver = FaizDbSaver(self.client, collection_prefix="test_lg")

    def test_put_and_get_checkpoint(self):
        config = {"configurable": {"thread_id": "thread_42"}}
        checkpoint = {
            "id": "chk_001",
            "v": 1,
            "ts": "2026-09-06T19:00:00Z",
            "channel_values": {"messages": ["Hello Agent"]},
        }
        metadata = {"source": "input", "step": 1}
        new_versions = {"messages": 1}

        # 1. Put checkpoint
        saved_config = self.saver.put(config, checkpoint, metadata, new_versions)
        self.assertEqual(saved_config["configurable"]["thread_id"], "thread_42")
        self.assertEqual(saved_config["configurable"]["checkpoint_id"], "chk_001")

        # 2. Get tuple
        chk_tuple = self.saver.get_tuple(saved_config)
        self.assertIsNotNone(chk_tuple)
        
        # Checkpoint tuple verification (dict or object)
        if isinstance(chk_tuple, dict):
            self.assertEqual(chk_tuple["checkpoint"]["id"], "chk_001")
            self.assertEqual(chk_tuple["checkpoint"]["channel_values"]["messages"], ["Hello Agent"])
        else:
            self.assertEqual(chk_tuple.checkpoint["id"], "chk_001")
            self.assertEqual(chk_tuple.checkpoint["channel_values"]["messages"], ["Hello Agent"])

    def test_list_checkpoints(self):
        config = {"configurable": {"thread_id": "thread_history"}}
        self.saver.put(config, {"id": "chk_1"}, {"step": 1}, {})
        self.saver.put(config, {"id": "chk_2"}, {"step": 2}, {})

        history = list(self.saver.list(config))
        self.assertEqual(len(history), 2)


if __name__ == "__main__":
    unittest.main()
