/**
 * 🔥 FaizDB Official Client SDK for Node.js, Bun, and TypeScript.
 *
 * @example
 * ```typescript
 * import { FaizClient } from 'faizdb';
 *
 * const db = new FaizClient('http://localhost:27018');
 *
 * // MongoDB-style syntax
 * const users = db.collection('users');
 * await users.insert({ name: 'Ahmad Faiz', role: 'DB Creator', age: 30 });
 * const results = await users.find({ age: { $gte: 25 } });
 *
 * // SQL-style syntax
 * const sqlResults = await db.query('SELECT * FROM users WHERE age >= 25');
 *
 * // AI Vector Search
 * const vectorMatches = await users.vectorSearch([0.95, 0.90, 0.10, 0.05], { topK: 5 });
 * ```
 */

export interface QueryResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface CollectionStats {
  name: string;
  document_count: number;
  total_size: number;
  avg_document_size: number;
  index_count: number;
}

export class FaizCollection<T extends Record<string, any> = Record<string, any>> {
  constructor(
    private client: FaizClient,
    public readonly name: string
  ) {}

  /**
   * Insert a document into this collection
   */
  async insert(doc: T): Promise<{ id: string }> {
    const res = await this.client.request<{ id: string }>(
      `/v1/collections/${this.name}/insert`,
      'POST',
      doc
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || 'Insert failed');
    }
    return res.data;
  }

  /**
   * Find documents with a MongoDB-style filter or empty for all
   */
  async find(filter?: Record<string, any>): Promise<T[]> {
    const filterJson = filter ? JSON.stringify(filter) : '{}';
    const query = `db.${this.name}.find(${filterJson})`;
    return this.client.query<T[]>(query);
  }

  /**
   * Count documents in this collection
   */
  async count(filter?: Record<string, any>): Promise<number> {
    const filterJson = filter ? JSON.stringify(filter) : '';
    const query = `db.${this.name}.count(${filterJson})`;
    return this.client.query<number>(query);
  }

  /**
   * Vector Similarity Search (AI-Native)
   */
  async vectorSearch(vector: number[], options: { topK?: number } = {}): Promise<T[]> {
    const topK = options.topK ?? 10;
    const query = `FIND ${this.name} VECTOR NEAR ${JSON.stringify(vector)} TOP ${topK}`;
    return this.client.query<T[]>(query);
  }

  /**
   * Get collection statistics
   */
  async stats(): Promise<CollectionStats> {
    const res = await this.client.request<CollectionStats>(
      `/v1/collections/${this.name}/stats`,
      'GET'
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || 'Failed to fetch collection stats');
    }
    return res.data;
  }
}

export class FaizClient {
  private baseUrl: string;

  constructor(endpoint: string = 'http://localhost:27018') {
    this.baseUrl = endpoint.replace(/\/+$/, '');
  }

  /**
   * Get a handle to a collection
   */
  collection<T extends Record<string, any> = Record<string, any>>(name: string): FaizCollection<T> {
    return new FaizCollection<T>(this, name);
  }

  /**
   * Execute any FaizDB query string (SQL, MongoDB JSON, or FaizQL)
   */
  async query<T = any>(queryString: string): Promise<T> {
    const res = await this.request<any>('/v1/query', 'POST', { query: queryString });
    if (!res.success) {
      throw new Error(res.error || 'Query execution failed');
    }
    // Unwrap inner QueryResult enum if present
    if (res.data && typeof res.data === 'object') {
      if ('Documents' in res.data) return res.data.Documents;
      if ('Count' in res.data) return res.data.Count;
      if ('Inserted' in res.data) return res.data.Inserted;
      if ('Updated' in res.data) return res.data.Updated;
      if ('Deleted' in res.data) return res.data.Deleted;
      if ('Success' in res.data) return res.data.Success;
    }
    return res.data;
  }

  /**
   * Check database server health
   */
  async health(): Promise<{ status: string; engine: string; version: string }> {
    const res = await this.request<any>('/v1/health', 'GET');
    return res.data;
  }

  /**
   * Internal HTTP request helper (uses standard fetch, works in Node, Bun, Deno, Browsers)
   */
  async request<T>(path: string, method: string, body?: any): Promise<QueryResponse<T>> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'Accept': 'application/json',
    };

    const response = await fetch(url, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
    });

    const data = await response.json();
    return data as QueryResponse<T>;
  }
}
