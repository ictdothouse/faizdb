"""
🔥 FaizDB Official Python SDK — The Universal High-Performance Database Client.

Supports Document CRUD, HNSW Vector ANN Search, Okapi BM25 Fuzzy Search,
Cost-Based EXPLAIN Query Plan, Secondary B-Tree Indexes, and JWT RBAC.

Usage:
    from faizdb import FaizDB

    db = FaizDB("http://localhost:27018", token="...")
    # Or login dynamically:
    db.login("admin", "faizdb-admin-2026")

    # 1. Document CRUD
    users = db.collection("users")
    user_id = users.insert({"name": "Ahmad Faiz", "email": "faiz@ict.house", "role": "Architect"})
    all_users = users.find({"role": "Architect"})

    # 2. HNSW Vector Similarity Search (< 1ms)
    matches = users.vector_search([0.95, 0.90, 0.10, 0.05], top_k=5)

    # 3. Okapi BM25 Fuzzy Full-Text Search
    search_results = users.search("Faiz", fuzzy=True, top_k=10)

    # 4. EXPLAIN Query Execution Plan
    plan = db.explain("SELECT * FROM users WHERE email = 'faiz@ict.house'")
    print(plan["plan_type"], plan["execution_time_us"], "µs")
"""

import json
from typing import Any, Dict, List, Optional
import urllib.request
import urllib.error


class FaizCollection:
    def __init__(self, client: "FaizDB", name: str):
        self.client = client
        self.name = name

    def insert(self, document: Dict[str, Any]) -> str:
        """Insert a document into this collection and return its ID."""
        res = self.client._request(f"/v1/collections/{self.name}/insert", method="POST", data=document)
        if not res.get("success"):
            raise RuntimeError(res.get("error", "Insert failed"))
        return res["data"]["id"]

    def insert_many(self, documents: List[Dict[str, Any]]) -> List[str]:
        """Bulk insert a list of documents into this collection."""
        res = self.client._request(f"/v1/collections/{self.name}/import", method="POST", data={"documents": documents})
        if not res.get("success"):
            raise RuntimeError(res.get("error", "Bulk insert failed"))
        return res["data"]["inserted_ids"]

    def find(self, filter_dict: Optional[Dict[str, Any]] = None) -> List[Dict[str, Any]]:
        """Find documents matching filter criteria."""
        filter_str = json.dumps(filter_dict) if filter_dict else "{}"
        query = f"db.{self.name}.find({filter_str})"
        return self.client.query(query)

    def find_one(self, filter_dict: Optional[Dict[str, Any]] = None) -> Optional[Dict[str, Any]]:
        """Find a single document matching filter criteria."""
        results = self.find(filter_dict)
        return results[0] if results else None

    def delete_by_id(self, doc_id: str) -> bool:
        """Delete a document by its ID."""
        res = self.client._request(f"/v1/collections/{self.name}/documents/{doc_id}", method="DELETE")
        if not res.get("success"):
            raise RuntimeError(res.get("error", "Delete failed"))
        return True

    def count(self, filter_dict: Optional[Dict[str, Any]] = None) -> int:
        """Count matching documents."""
        filter_str = json.dumps(filter_dict) if filter_dict else ""
        query = f"db.{self.name}.count({filter_str})"
        return self.client.query(query)

    def vector_search(self, vector: List[float], top_k: int = 10) -> List[Dict[str, Any]]:
        """AI-native HNSW vector similarity search (< 1ms)."""
        query = f"FIND {self.name} VECTOR NEAR {json.dumps(vector)} TOP {top_k}"
        return self.client.query(query)

    def search(self, query_text: str, fuzzy: bool = True, top_k: int = 10) -> List[Dict[str, Any]]:
        """Okapi BM25 full-text search with fuzzy typo-tolerance."""
        res = self.client._request(
            f"/v1/collections/{self.name}/search",
            method="POST",
            data={"query": query_text, "fuzzy": fuzzy, "top_k": top_k}
        )
        if not res.get("success"):
            raise RuntimeError(res.get("error", "Search failed"))
        return res["data"]

    def create_index(self, field: str, unique: bool = False) -> Dict[str, Any]:
        """Create a secondary B-Tree index on a field."""
        res = self.client._request(
            f"/v1/collections/{self.name}/indexes",
            method="POST",
            data={"field": field, "unique": unique}
        )
        if not res.get("success"):
            raise RuntimeError(res.get("error", "Index creation failed"))
        return res["data"]

    def stats(self) -> Dict[str, Any]:
        """Fetch statistics for this collection."""
        res = self.client._request(f"/v1/collections/{self.name}/stats", method="GET")
        if not res.get("success"):
            raise RuntimeError(res.get("error", "Stats fetch failed"))
        return res["data"]


class FaizDB:
    def __init__(self, endpoint: str = "http://localhost:27018", token: Optional[str] = None):
        self.endpoint = endpoint.rstrip("/")
        self.token = token

    def login(self, username: str, password: str) -> Dict[str, Any]:
        """Authenticate with username and password to obtain JWT token."""
        res = self._request("/v1/auth/login", method="POST", data={"username": username, "password": password})
        if not res.get("success"):
            raise RuntimeError(res.get("error", "Login failed"))
        self.token = res["data"]["token"]
        return res["data"]

    def collection(self, name: str) -> FaizCollection:
        """Get a handle to a collection."""
        return FaizCollection(self, name)

    def query(self, query_string: str) -> Any:
        """Execute any SQL, MongoDB JSON, or FaizQL query string."""
        res = self._request("/v1/query", method="POST", data={"query": query_string})
        if not res.get("success"):
            raise RuntimeError(res.get("error", "Query execution failed"))
        data = res.get("data")
        if isinstance(data, dict):
            for key in ["Documents", "Count", "Inserted", "Updated", "Deleted", "Success", "Explain"]:
                if key in data:
                    return data[key]
        return data

    def explain(self, query_string: str) -> Dict[str, Any]:
        """Generate a Cost-Based EXPLAIN Query Execution Plan."""
        clean_q = query_string.strip()
        if not clean_q.upper().startswith("EXPLAIN"):
            clean_q = f"EXPLAIN {clean_q}"
        return self.query(clean_q)

    def health(self) -> Dict[str, Any]:
        """Check database server health."""
        return self._request("/v1/health", method="GET")

    def metrics(self) -> str:
        """Fetch server Prometheus telemetries."""
        url = f"{self.endpoint}/v1/metrics"
        req = urllib.request.Request(url, method="GET")
        with urllib.request.urlopen(req) as response:
            return response.read().decode("utf-8")

    def _request(self, path: str, method: str = "GET", data: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        url = f"{self.endpoint}{path}"
        headers = {"Content-Type": "application/json", "Accept": "application/json"}
        if self.token:
            headers["Authorization"] = f"Bearer {self.token}"

        payload = json.dumps(data).encode("utf-8") if data is not None else None

        req = urllib.request.Request(url, data=payload, headers=headers, method=method)
        try:
            with urllib.request.urlopen(req) as response:
                return json.loads(response.read().decode("utf-8"))
        except urllib.error.HTTPError as e:
            err_body = e.read().decode("utf-8")
            try:
                return json.loads(err_body)
            except Exception:
                raise RuntimeError(f"HTTP {e.code}: {err_body}")
        except Exception as e:
            raise RuntimeError(f"Connection failed: {e}")


from .grpc_client import FaizDbGrpcClient
from .langgraph import FaizDbSaver

__all__ = ["FaizDB", "FaizCollection", "FaizDbGrpcClient", "FaizDbSaver"]

