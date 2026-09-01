/**
 * FaizDB Studio REST Client
 * Directly connects to FaizDB Native Rust Engine on http://127.0.0.1:27018
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
  private baseUrl: string = 'http://127.0.0.1:27018';

  setEndpoint(url: string) {
    this.baseUrl = url.replace(/\/+$/, '');
  }

  getEndpoint(): string {
    return this.baseUrl;
  }

  async getHealth(): Promise<{ status: string; engine: string; version: string }> {
    const res = await fetch(`${this.baseUrl}/v1/health`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
  }

  async getInfo(): Promise<ServerInfo> {
    const res = await fetch(`${this.baseUrl}/v1/info`);
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    return res.json();
  }

  async query<T = any>(queryStr: string): Promise<{ data: T; durationMs: number }> {
    const start = performance.now();
    const res = await fetch(`${this.baseUrl}/v1/query`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ query: queryStr }),
    });

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

  async getCollectionStats(name: string): Promise<CollectionStats> {
    const res = await fetch(`${this.baseUrl}/v1/collections/${name}/stats`);
    const json = await res.json();
    if (!json.success) throw new Error(json.error || 'Failed to fetch collection stats');
    return json.data;
  }

  async insertDocument(collection: string, doc: Record<string, any>): Promise<string> {
    const res = await fetch(`${this.baseUrl}/v1/collections/${collection}/insert`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(doc),
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || 'Insert failed');
    return json.data.id;
  }
}

export const api = new FaizApiClient();
