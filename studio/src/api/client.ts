/**
 * FaizDB Studio REST Client
 * Connects seamlessly via Vite Proxy or direct host with CORS fallback
 */

export interface ApiResponse<T = any> {
  success: boolean;
  data?: T;
  error?: string;
}

export interface ServerInfo {
  name: string;
  version: string;
  creator: string;
  features: string[];
}

export interface CollectionStats {
  name: string;
  document_count: number;
  total_size: number;
  avg_document_size: number;
  index_count: number;
}

class FaizApiClient {
  // Use relative URL so Vite proxy or direct CORS handles it cleanly
  private baseUrl: string = '';
  private apiKey: string = import.meta.env.VITE_FAIZDB_API_KEY || 'faizdb-secret-key';
  private clusterToken: string = import.meta.env.VITE_FAIZDB_CLUSTER_TOKEN || 'faizdb-cluster-secret';

  setEndpoint(url: string) {
    this.baseUrl = url.replace(/\/+$/, '');
  }

  getEndpoint(): string {
    return this.baseUrl || window.location.origin;
  }
  
  getApiKey(): string {
    return this.apiKey;
  }

  async fetch(url: string, init?: RequestInit): Promise<Response> {
    const headers = { 
      'Authorization': `Bearer ${this.apiKey}`,
      ...(init?.headers || {}) 
    };
    return fetch(url, { ...init, headers });
  }

  async clusterFetch(url: string, init?: RequestInit): Promise<Response> {
    const headers = { 
      'Authorization': `Bearer ${this.clusterToken}`,
      ...(init?.headers || {}) 
    };
    return fetch(url, { ...init, headers });
  }

  async getHealth(): Promise<{ status: string; engine: string; version: string }> {
    try {
      const res = await this.fetch(`${this.baseUrl}/v1/health`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch {
      // Fallback direct port 27018
      const res = await this.fetch('http://127.0.0.1:27018/v1/health');
      return await res.json();
    }
  }

  async getInfo(): Promise<ServerInfo> {
    try {
      const res = await this.fetch(`${this.baseUrl}/v1/info`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch {
      const res = await this.fetch('http://127.0.0.1:27018/v1/info');
      return await res.json();
    }
  }

  async query<T = any>(queryStr: string): Promise<{ data: T; durationMs: number }> {
    const start = performance.now();
    let res: Response;
    try {
      res = await this.fetch(`${this.baseUrl}/v1/query`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: queryStr }),
      });
    } catch {
      res = await this.fetch('http://127.0.0.1:27018/v1/query', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: queryStr }),
      });
    }

    const durationMs = Math.round((performance.now() - start) * 100) / 100;
    const json: ApiResponse<T> = await res.json();

    if (!json.success) {
      throw new Error(json.error || 'Query failed');
    }

    let payload: any = json.data;
    if (payload && typeof payload === 'object') {
      if ('Documents' in payload) payload = payload.Documents;
      else if ('Count' in payload) payload = payload.Count;
      else if ('Inserted' in payload) payload = payload.Inserted;
      else if ('Updated' in payload) payload = payload.Updated;
      else if ('Deleted' in payload) payload = payload.Deleted;
      else if ('Success' in payload) payload = payload.Success;
    }

    return { data: payload, durationMs };
  }

  async insertDocument(collection: string, doc: Record<string, any>): Promise<string> {
    let res: Response;
    try {
      res = await this.fetch(`${this.baseUrl}/v1/collections/${collection}/insert`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(doc),
      });
    } catch {
      res = await this.fetch(`http://127.0.0.1:27018/v1/collections/${collection}/insert`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(doc),
      });
    }

    const json = await res.json();
    if (!json.success) throw new Error(json.error || 'Insert failed');
    return json.data.id;
  }
}

export const api = new FaizApiClient();
