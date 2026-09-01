/**
 * FaizDB Studio REST Client
 * JWT-aware — obtains a short-lived token via /v1/auth/login and refreshes automatically.
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

export interface AuthSession {
  token: string;
  username: string;
  role: string;
  expiresAt: number; // unix ms
}

const SESSION_KEY = 'faizdb_session';
const CLUSTER_TOKEN_KEY = 'faizdb_cluster_token';

class FaizApiClient {
  private baseUrl: string = '';

  // ── Session Management ────────────────────────────────────────────────────

  getSession(): AuthSession | null {
    try {
      const raw = sessionStorage.getItem(SESSION_KEY);
      if (!raw) return null;
      const session: AuthSession = JSON.parse(raw);
      if (Date.now() > session.expiresAt) {
        sessionStorage.removeItem(SESSION_KEY);
        return null;
      }
      return session;
    } catch {
      return null;
    }
  }

  saveSession(session: AuthSession) {
    sessionStorage.setItem(SESSION_KEY, JSON.stringify(session));
  }

  clearSession() {
    sessionStorage.removeItem(SESSION_KEY);
  }

  isAuthenticated(): boolean {
    return this.getSession() !== null;
  }

  getUsername(): string {
    return this.getSession()?.username ?? '';
  }

  getRole(): string {
    return this.getSession()?.role ?? '';
  }

  // ── Endpoint ─────────────────────────────────────────────────────────────

  setEndpoint(url: string) {
    this.baseUrl = url.replace(/\/+$/, '');
  }

  getEndpoint(): string {
    return this.baseUrl || window.location.origin;
  }

  // Legacy compat for cluster token (cluster routes still use static token)
  getClusterToken(): string {
    return (
      sessionStorage.getItem(CLUSTER_TOKEN_KEY) ||
      import.meta.env.VITE_FAIZDB_CLUSTER_TOKEN ||
      'faizdb-cluster-secret'
    );
  }

  // ── Core Fetch Helpers ────────────────────────────────────────────────────

  /** Authenticated fetch — uses JWT Bearer token from session */
  async fetch(url: string, init?: RequestInit): Promise<Response> {
    const session = this.getSession();
    const headers: Record<string, string> = {
      ...(init?.headers as Record<string, string> || {}),
    };
    if (session) {
      headers['Authorization'] = `Bearer ${session.token}`;
    }
    return window.fetch(url, { ...init, headers });
  }

  /** Cluster fetch — uses static cluster token */
  async clusterFetch(url: string, init?: RequestInit): Promise<Response> {
    const headers = {
      'Authorization': `Bearer ${this.getClusterToken()}`,
      ...(init?.headers || {}),
    };
    return window.fetch(url, { ...init, headers });
  }

  // ── Authentication ────────────────────────────────────────────────────────

  async login(username: string, password: string): Promise<AuthSession> {
    const base = this.baseUrl || '';
    const res = await window.fetch(`${base}/v1/auth/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, password }),
    });

    const json: ApiResponse<{ token: string; username: string; role: string; expires_in: number }> =
      await res.json();

    if (!res.ok || !json.success || !json.data) {
      throw new Error(json.error || 'Login failed');
    }

    const session: AuthSession = {
      token: json.data.token,
      username: json.data.username,
      role: json.data.role,
      expiresAt: Date.now() + json.data.expires_in * 1000,
    };
    this.saveSession(session);
    return session;
  }

  logout() {
    this.clearSession();
  }

  async whoami(): Promise<{ username: string; role: string }> {
    const res = await this.fetch(`${this.baseUrl}/v1/auth/whoami`);
    const json: ApiResponse<{ username: string; role: string }> = await res.json();
    if (!json.success || !json.data) throw new Error(json.error || 'Not authenticated');
    return json.data;
  }

  // ── API Methods ───────────────────────────────────────────────────────────

  async getHealth(): Promise<{ status: string; engine: string; version: string }> {
    try {
      const res = await window.fetch(`${this.baseUrl}/v1/health`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch {
      const res = await window.fetch('http://127.0.0.1:27018/v1/health');
      return await res.json();
    }
  }

  async getInfo(): Promise<ServerInfo> {
    try {
      const res = await window.fetch(`${this.baseUrl}/v1/info`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch {
      const res = await window.fetch('http://127.0.0.1:27018/v1/info');
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

    if (!json.success) throw new Error(json.error || 'Query failed');

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

  // ── Security & Audit Trail ────────────────────────────────────────────────

  async getAuditLogs(limit: number = 50): Promise<Array<{
    timestamp: string;
    event: string;
    ip: string;
    path: string;
    status: number;
    request_id: string;
    engine: string;
  }>> {
    const res = await this.fetch(`${this.baseUrl}/v1/audit/logs?limit=${limit}`);
    const json: ApiResponse<any[]> = await res.json();
    if (!json.success || !json.data) throw new Error(json.error || 'Failed to fetch audit logs');
    return json.data;
  }

  async generateToken(username: string, role: string, validSeconds: number = 86400 * 30): Promise<{
    token: string;
    username: string;
    role: string;
    valid_seconds: number;
  }> {
    const res = await this.fetch(`${this.baseUrl}/v1/auth/token`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ username, role, valid_seconds: validSeconds }),
    });
    const json: ApiResponse<any> = await res.json();
    if (!json.success || !json.data) throw new Error(json.error || 'Failed to generate token');
    return json.data;
  }

  // ── Backup & Disaster Recovery ───────────────────────────────────────────

  async createBackup(passphrase?: string): Promise<any> {
    const res = await this.fetch(`${this.baseUrl}/v1/backup/create`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ passphrase: passphrase || undefined }),
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || 'Backup creation failed');
    return json.data;
  }

  async listBackups(): Promise<any[]> {
    const res = await this.fetch(`${this.baseUrl}/v1/backup/list`);
    const json = await res.json();
    if (!json.success) throw new Error(json.error || 'Failed to list backups');
    return json.data || [];
  }

  async restoreBackup(filename?: string, passphrase?: string): Promise<any> {
    const res = await this.fetch(`${this.baseUrl}/v1/backup/restore`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ filename, passphrase }),
    });
    const json = await res.json();
    if (!json.success) throw new Error(json.error || 'Restore failed');
    return json.data;
  }
}

export const api = new FaizApiClient();
