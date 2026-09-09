/**
 * FaizDB Official Node.js SDK Quickstart Client
 * ============================================
 * AI-Native NoSQL Database Engine — Multi-Protocol, Vector Search & Graph Intelligence.
 * 
 * Works out-of-the-box in Node.js 18+ using native fetch.
 */

class FaizDBClient {
  /**
   * @param {string} [host="http://127.0.0.1:27018"]
   * @param {string} [authToken]
   */
  constructor(host = "http://127.0.0.1:27018", authToken = null) {
    this.host = host.replace(/\/+$/, "");
    this.headers = {
      "Content-Type": "application/json",
      ...(authToken ? { Authorization: `Bearer ${authToken}` } : {}),
    };
  }

  /**
   * Check if server is reachable
   * @returns {Promise<boolean>}
   */
  async ping() {
    try {
      const res = await fetch(`${this.host}/health`);
      return res.ok;
    } catch {
      return false;
    }
  }

  // ── Document Operations ──────────────────────────────────────────────────

  /**
   * Insert a document into a collection
   * @param {string} collection
   * @param {Object} document
   * @returns {Promise<string>} docId
   */
  async insert(collection, document) {
    const res = await fetch(`${this.host}/api/v1/collections/${collection}/documents`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify(document),
    });
    if (!res.ok) throw new Error(`Insert failed: ${res.statusText}`);
    const data = await res.json();
    return data.id;
  }

  /**
   * Find a document by ID
   * @param {string} collection
   * @param {string} id
   * @returns {Promise<Object|null>}
   */
  async findById(collection, id) {
    const res = await fetch(`${this.host}/api/v1/collections/${collection}/documents/${id}`, {
      headers: this.headers,
    });
    if (res.status === 404) return null;
    if (!res.ok) throw new Error(`Find failed: ${res.statusText}`);
    return res.json();
  }

  /**
   * Execute a SQL query
   * @param {string} sqlQuery
   * @returns {Promise<Array<Object>>}
   */
  async querySql(sqlQuery) {
    const res = await fetch(`${this.host}/api/v1/query/sql`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify({ query: sqlQuery }),
    });
    if (!res.ok) throw new Error(`Query failed: ${res.statusText}`);
    const data = await res.json();
    return data.results || [];
  }

  // ── AI Vector Search ─────────────────────────────────────────────────────

  /**
   * Create an HNSW vector index
   * @param {string} name
   * @param {number} dimensions
   * @param {"cosine"|"euclidean"|"dot"} [metric="cosine"]
   */
  async createVectorIndex(name, dimensions, metric = "cosine") {
    const res = await fetch(`${this.host}/api/v1/vector/indices`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify({ name, dimensions, metric }),
    });
    return res.ok;
  }

  /**
   * Insert vector embedding
   * @param {string} indexName
   * @param {string} id
   * @param {number[]} vector
   */
  async insertVector(indexName, id, vector) {
    const res = await fetch(`${this.host}/api/v1/vector/indices/${indexName}/insert`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify({ id, vector }),
    });
    return res.ok;
  }

  /**
   * Search nearest neighbors
   * @param {string} indexName
   * @param {number[]} queryVector
   * @param {number} [topK=5]
   */
  async vectorSearch(indexName, queryVector, topK = 5) {
    const res = await fetch(`${this.host}/api/v1/vector/indices/${indexName}/search`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify({ vector: queryVector, top_k: topK }),
    });
    if (!res.ok) throw new Error(`Search failed: ${res.statusText}`);
    const data = await res.json();
    return data.results || [];
  }
}

module.exports = { FaizDBClient };

if (require.main === module) {
  (async () => {
    console.log("✨ FaizDB Node.js SDK Quickstart");
    const client = new FaizDBClient();
    const alive = await client.ping();
    console.log(`Connected to FaizDB at ${client.host}: ${alive}`);
  })();
}
