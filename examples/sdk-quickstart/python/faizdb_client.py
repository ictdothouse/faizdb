#!/usr/bin/env python3
"""
FaizDB Official Python SDK Quickstart Client
============================================
AI-Native NoSQL Database Engine — Multi-Protocol, Vector Search & Graph Intelligence.

Requirements:
    pip install requests
"""

import json
import time
from typing import Any, Dict, List, Optional
import requests

class FaizDBClient:
    """High-performance Python client for FaizDB HTTP & REST APIs."""

    def __init__(self, host: str = "http://127.0.0.1:27018", auth_token: Optional[str] = None):
        self.host = host.rstrip("/")
        self.session = requests.Session()
        if auth_token:
            self.session.headers.update({"Authorization": f"Bearer {auth_token}"})

    def ping(self) -> bool:
        """Check if FaizDB server is alive and responding."""
        try:
            r = self.session.get(f"{self.host}/health", timeout=2)
            return r.status_code == 200
        except Exception:
            return False

    # ── Document CRUD ────────────────────────────────────────────────────────

    def insert(self, collection: str, document: Dict[str, Any]) -> str:
        """Insert a document into a collection and return the generated ID."""
        url = f"{self.host}/api/v1/collections/{collection}/documents"
        r = self.session.post(url, json=document)
        r.raise_for_status()
        return r.json().get("id", "")

    def find_by_id(self, collection: str, doc_id: str) -> Optional[Dict[str, Any]]:
        """Find a document by its unique ID."""
        url = f"{self.host}/api/v1/collections/{collection}/documents/{doc_id}"
        r = self.session.get(url)
        if r.status_code == 404:
            return None
        r.raise_for_status()
        return r.json()

    def query_sql(self, sql_query: str) -> List[Dict[str, Any]]:
        """Execute a SQL statement against FaizDB collections."""
        url = f"{self.host}/api/v1/query/sql"
        r = self.session.post(url, json={"query": sql_query})
        r.raise_for_status()
        return r.json().get("results", [])

    # ── AI Vector Search ─────────────────────────────────────────────────────

    def create_vector_index(self, name: str, dimensions: int, metric: str = "cosine") -> bool:
        """Create an HNSW high-dimensional vector index."""
        url = f"{self.host}/api/v1/vector/indices"
        r = self.session.post(url, json={"name": name, "dimensions": dimensions, "metric": metric})
        return r.status_code in (200, 201)

    def insert_vector(self, index_name: str, doc_id: str, vector: List[float]) -> bool:
        """Insert an embedding vector associated with a document ID."""
        url = f"{self.host}/api/v1/vector/indices/{index_name}/insert"
        r = self.session.post(url, json={"id": doc_id, "vector": vector})
        return r.status_code == 200

    def vector_search(self, index_name: str, query_vector: List[float], top_k: int = 5) -> List[Dict[str, Any]]:
        """Search nearest neighbors using HNSW similarity search."""
        url = f"{self.host}/api/v1/vector/indices/{index_name}/search"
        r = self.session.post(url, json={"vector": query_vector, "top_k": top_k})
        r.raise_for_status()
        return r.json().get("results", [])

    # ── Decoupled Graph Companion & GraphRAG ─────────────────────────────────

    def add_graph_vertex(self, vertex_id: str, label: str, properties: Dict[str, Any]) -> bool:
        """Add a vertex node to the knowledge graph."""
        url = f"{self.host}/api/v1/graph/vertices"
        r = self.session.post(url, json={"id": vertex_id, "label": label, "properties": properties})
        return r.status_code in (200, 201)

    def add_graph_edge(self, from_id: str, to_id: str, relation: str, weight: float = 1.0) -> bool:
        """Add a relationship edge between two vertices."""
        url = f"{self.host}/api/v1/graph/edges"
        r = self.session.post(url, json={"from": from_id, "to": to_id, "relation": relation, "weight": weight})
        return r.status_code in (200, 201)

    def extract_graphrag_context(self, root_id: str, max_depth: int = 2) -> Dict[str, Any]:
        """Extract multi-hop graph context formatted for LLM GraphRAG prompt injection."""
        url = f"{self.host}/api/v1/graph/rag-context"
        r = self.session.get(url, params={"root": root_id, "depth": max_depth})
        r.raise_for_status()
        return r.json()


if __name__ == "__main__":
    print("✨ FaizDB Python SDK Quickstart")
    client = FaizDBClient()
    print(f"Connected to FaizDB at {client.host}: {client.ping()}")
