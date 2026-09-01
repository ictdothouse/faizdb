/**
 * 🔥 FaizDB Official Client SDK for Node.js, Bun, Deno, and Browser.
 *
 * Universal, Zero-Dependency, Type-Safe Client for FaizDB.
 * Supports Document Operations, HNSW Vector ANN, Okapi BM25 Search,
 * TTL In-Memory Cache, Real-Time Change Streams, and JWT RBAC.
 *
 * @example
 * ```typescript
 * import { FaizClient } from 'faizdb';
 *
 * const db = new FaizClient({
 *   endpoint: 'http://localhost:27018',
 *   token: process.env.FAIZDB_TOKEN,
 * });
 *
 * // Or login dynamically:
 * await db.auth.login('admin', 'faizdb-admin-2026');
 *
 * // 1. Document Operations (MongoDB style)
 * const users = db.collection('users');
 * await users.insert({ name: 'Ahmad Faiz', role: 'Architect', country: 'Malaysia' });
 * const results = await users.find({ role: 'Architect' });
 *
 * // 2. AI Vector Similarity Search (HNSW)
 * const vectorMatches = await users.vectorSearch([0.95, 0.90, 0.10, 0.05], { topK: 5 });
 *
 * // 3. Okapi BM25 Fuzzy Full-Text Search
 * const searchResults = await users.search('Ahmad Faiz', { fuzzy: true, topK: 10 });
 *
 * // 4. Raw FaizQL / SQL Queries
 * const sqlData = await db.query('SELECT * FROM users WHERE active = true');
 *
 * // 5. Real-Time Change Stream (WebSocket)
 * const unsubscribe = users.watch((event) => {
 *   console.log('Real-time mutation:', event.operation_type, event.document_id);
 * });
 * ```
 */

export interface FaizClientOptions {
  endpoint?: string;
  token?: string;
  apiKey?: string;
}

export interface ApiResponse<T = any> {
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

export interface SearchResult<T = any> {
  _id: string;
  _score: number;
  _matched_terms: string[];
  [key: string]: any;
}

export interface ChangeEvent<T = any> {
  resume_token: string;
  timestamp: string;
  collection: string;
  operation_type: 'insert' | 'update' | 'delete' | 'replace' | 'drop';
  document_id: string;
  full_document?: T;
  updated_fields?: Record<string, any>;
}

export class FaizCollection<T extends Record<string, any> = Record<string, any>> {
  constructor(
    private client: FaizClient,
    public readonly name: string
  ) {}

  /**
   * Insert a single document into this collection
   */
  async insert(doc: T): Promise<{ id: string }> {
    const res = await this.client.request<{ id: string }>(
      `/v1/collections/${this.name}/insert`,
      'POST',
      doc
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || `Failed to insert document into '${this.name}'`);
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
   * Find a single document by filter
   */
  async findOne(filter?: Record<string, any>): Promise<T | null> {
    const results = await this.find(filter);
    return results.length > 0 ? results[0] : null;
  }

  /**
   * Delete a document by ID
   */
  async deleteById(id: string): Promise<{ deleted: boolean; id: string }> {
    const res = await this.client.request<{ deleted: boolean; id: string }>(
      `/v1/collections/${this.name}/documents/${encodeURIComponent(id)}`,
      'DELETE'
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || `Failed to delete document '${id}' from '${this.name}'`);
    }
    return res.data;
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
   * Vector Similarity Search (HNSW Cosine/L2/Dot Product < 1ms)
   */
  async vectorSearch(vector: number[], options: { topK?: number } = {}): Promise<T[]> {
    const topK = options.topK ?? 10;
    const query = `FIND ${this.name} VECTOR NEAR ${JSON.stringify(vector)} TOP ${topK}`;
    return this.client.query<T[]>(query);
  }

  /**
   * Okapi BM25 Fuzzy Full-Text Search
   */
  async search(
    queryText: string,
    options: { fuzzy?: boolean; topK?: number } = {}
  ): Promise<SearchResult<T>[]> {
    const res = await this.client.request<SearchResult<T>[]>(
      `/v1/collections/${this.name}/search`,
      'POST',
      {
        query: queryText,
        fuzzy: options.fuzzy ?? true,
        topK: options.topK ?? 10,
      }
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || `Text search failed in '${this.name}'`);
    }
    return res.data;
  }

  /**
   * Complex Aggregation Pipeline ($match, $group, $sort, $project, $limit)
   */
  async aggregate<R = any>(pipeline: Record<string, any>[]): Promise<R[]> {
    const res = await this.client.request<R[]>(
      `/v1/collections/${this.name}/aggregate`,
      'POST',
      { pipeline }
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || `Aggregation failed in '${this.name}'`);
    }
    return res.data;
  }

  /**
   * Real-Time Change Stream Watcher for this collection (WebSocket)
   */
  watch(callback: (event: ChangeEvent<T>) => void): () => void {
    return this.client.watchCollection<T>(this.name, callback);
  }

  /**
   * Get collection storage & index statistics
   */
  async stats(): Promise<CollectionStats> {
    const res = await this.client.request<CollectionStats>(
      `/v1/collections/${this.name}/stats`,
      'GET'
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || `Failed to fetch stats for '${this.name}'`);
    }
    return res.data;
  }

  /**
   * In-Memory Cache TTL Statistics
   */
  async ttlStats(): Promise<any> {
    const res = await this.client.request<any>(
      `/v1/collections/${this.name}/ttl/stats`,
      'GET'
    );
    return res.data;
  }

  /**
   * Manually purge expired TTL entries
   */
  async purgeExpired(): Promise<{ purged_count: number; purged_ids: string[] }> {
    const res = await this.client.request<{ purged_count: number; purged_ids: string[] }>(
      `/v1/collections/${this.name}/ttl/purge`,
      'POST'
    );
    return res.data!;
  }
}

export class AuthNamespace {
  constructor(private client: FaizClient) {}

  /**
   * Login with username and password to obtain a signed JWT
   */
  async login(username: string, password: string): Promise<{
    token: string;
    username: string;
    role: string;
    expires_in: number;
  }> {
    const res = await this.client.request<{
      token: string;
      username: string;
      role: string;
      expires_in: number;
    }>('/v1/auth/login', 'POST', { username, password });

    if (!res.success || !res.data) {
      throw new Error(res.error || 'Authentication failed');
    }

    this.client.setToken(res.data.token);
    return res.data;
  }

  /**
   * Inspect currently authenticated identity & role
   */
  async whoami(): Promise<{ username: string; role: string }> {
    const res = await this.client.request<{ username: string; role: string }>(
      '/v1/auth/whoami',
      'GET'
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || 'Not authenticated');
    }
    return res.data;
  }

  /**
   * Generate an application JWT token (Admin role required)
   */
  async generateToken(username: string, role: 'Admin' | 'ReadWrite' | 'ReadOnly', validDays: number = 30): Promise<{
    token: string;
    username: string;
    role: string;
    valid_seconds: number;
  }> {
    const res = await this.client.request<any>(
      '/v1/auth/token',
      'POST',
      { username, role, valid_seconds: validDays * 86400 }
    );
    if (!res.success || !res.data) {
      throw new Error(res.error || 'Failed to generate token');
    }
    return res.data;
  }
}

export class FaizClient {
  private baseUrl: string;
  private token?: string;
  public readonly auth: AuthNamespace;

  constructor(options: string | FaizClientOptions = 'http://localhost:27018') {
    if (typeof options === 'string') {
      this.baseUrl = options.replace(/\/+$/, '');
    } else {
      this.baseUrl = (options.endpoint || 'http://localhost:27018').replace(/\/+$/, '');
      this.token = options.token || options.apiKey;
    }
    this.auth = new AuthNamespace(this);
  }

  setToken(token: string) {
    this.token = token;
  }

  getToken(): string | undefined {
    return this.token;
  }

  /**
   * Access a specific collection
   */
  collection<T extends Record<string, any> = Record<string, any>>(name: string): FaizCollection<T> {
    return new FaizCollection<T>(this, name);
  }

  /**
   * Execute any query string (SQL, MongoDB JSON, or FaizQL)
   */
  async query<T = any>(queryString: string): Promise<T> {
    const res = await this.request<any>('/v1/query', 'POST', { query: queryString });
    if (!res.success) {
      throw new Error(res.error || 'Query execution failed');
    }
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
   * Subscribe to global real-time change streams across all collections
   */
  watch(callback: (event: ChangeEvent) => void): () => void {
    const wsTokenParam = this.token ? `?token=${encodeURIComponent(this.token)}` : '';
    const wsUrl = `${this.baseUrl.replace(/^http/, 'ws')}/v1/subscribe${wsTokenParam}`;
    const ws = new (typeof WebSocket !== 'undefined' ? WebSocket : require('ws'))(wsUrl);

    ws.onmessage = (msg: any) => {
      try {
        const raw = typeof msg.data === 'string' ? msg.data : msg.data.toString();
        const data = JSON.parse(raw);
        if (data.status === 'connected') return;
        callback(data);
      } catch {}
    };

    return () => {
      try {
        ws.close();
      } catch {}
    };
  }

  /**
   * Watch mutations on a specific collection
   */
  watchCollection<T = any>(collection: string, callback: (event: ChangeEvent<T>) => void): () => void {
    const wsTokenParam = this.token ? `?token=${encodeURIComponent(this.token)}` : '';
    const wsUrl = `${this.baseUrl.replace(/^http/, 'ws')}/v1/collections/${collection}/watch${wsTokenParam}`;
    const ws = new (typeof WebSocket !== 'undefined' ? WebSocket : require('ws'))(wsUrl);

    ws.onmessage = (msg: any) => {
      try {
        const raw = typeof msg.data === 'string' ? msg.data : msg.data.toString();
        const data = JSON.parse(raw);
        if (data.status === 'connected') return;
        callback(data);
      } catch {}
    };

    return () => {
      try {
        ws.close();
      } catch {}
    };
  }

  /**
   * Check database server health
   */
  async health(): Promise<{ status: string; engine: string; version: string }> {
    const res = await this.request<any>('/v1/health', 'GET');
    return res.data;
  }

  /**
   * Fetch server telemetry & Prometheus metrics
   */
  async metrics(): Promise<string> {
    const url = `${this.baseUrl}/v1/metrics`;
    const response = await fetch(url);
    return response.text();
  }

  /**
   * Internal HTTP request dispatcher
   */
  async request<T>(path: string, method: string, body?: any): Promise<ApiResponse<T>> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
      'Accept': 'application/json',
    };

    if (this.token) {
      headers['Authorization'] = `Bearer ${this.token}`;
    }

    const response = await fetch(url, {
      method,
      headers,
      body: body ? JSON.stringify(body) : undefined,
    });

    const data = await response.json();
    return data as ApiResponse<T>;
  }
}

export default FaizClient;
export * from './grpc';
