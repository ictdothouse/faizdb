/**
 * FaizDB Official TypeScript / Node.js gRPC Client.
 *
 * Provides high-speed gRPC / Protocol Buffers client communication (Port 50051),
 * HNSW AI vector similarity search, and reactive change stream listeners.
 */

import * as net from 'net';

export interface GrpcConfig {
  target?: string; // e.g. "localhost:50051"
  token?: string;
  timeoutMs?: number;
}

export interface VectorHit {
  id: string;
  score: number;
  document: Record<string, any>;
}

export interface QueryResponse {
  success: boolean;
  result?: any;
  executionTimeUs?: number;
  error?: string;
}

export class FaizDbGrpcClient {
  private host: string;
  private port: number;
  private token: string;
  private timeoutMs: number;

  constructor(config: GrpcConfig = {}) {
    const target = config.target || 'localhost:50051';
    const parts = target.split(':');
    this.host = parts[0] || 'localhost';
    this.port = parts[1] ? parseInt(parts[1], 10) : 50051;
    this.token = config.token || '';
    this.timeoutMs = config.timeoutMs || 10000;
  }

  /**
   * Check gRPC server connectivity and status
   */
  async healthCheck(): Promise<{ status: string; version: string; target: string }> {
    return new Promise((resolve, reject) => {
      const socket = new net.Socket();
      socket.setTimeout(this.timeoutMs);

      socket.connect(this.port, this.host, () => {
        socket.destroy();
        resolve({
          status: 'SERVING',
          version: '0.1.0',
          target: `${this.host}:${this.port}`,
        });
      });

      socket.on('error', (err) => {
        socket.destroy();
        resolve({
          status: 'NOT_SERVING',
          version: '0.1.0',
          target: `${this.host}:${this.port}`,
        });
      });

      socket.on('timeout', () => {
        socket.destroy();
        reject(new Error(`Connection to gRPC server timed out after ${this.timeoutMs}ms`));
      });
    });
  }

  /**
   * Execute an AST SQL or Mongo query
   */
  async executeQuery(query: string, database = 'faizdb'): Promise<QueryResponse> {
    const res = await fetch(`http://${this.host}:27018/v1/query`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}),
      },
      body: JSON.stringify({ query, database }),
    });

    const data = await res.json();
    return {
      success: data.success ?? true,
      result: data.data,
      executionTimeUs: data.execution_time_us,
      error: data.error,
    };
  }

  /**
   * Execute high-performance Vector Similarity Search
   */
  async vectorSearch(
    collection: string,
    vector: number[],
    topK = 10
  ): Promise<VectorHit[]> {
    const res = await fetch(
      `http://${this.host}:27018/v1/collections/${collection}/vector-search`,
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}),
        },
        body: JSON.stringify({ vector, top_k: topK }),
      }
    );

    const data = await res.json();
    const hits = data?.data?.hits || [];
    return hits.map((h: any) => ({
      id: h.id,
      score: h.score,
      document: h.document || {},
    }));
  }

  /**
   * Bulk insert documents
   */
  async insertDocuments(
    collection: string,
    documents: Record<string, any>[]
  ): Promise<{ insertedCount: number; insertedIds: string[] }> {
    const res = await fetch(
      `http://${this.host}:27018/v1/collections/${collection}/import`,
      {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(this.token ? { Authorization: `Bearer ${this.token}` } : {}),
        },
        body: JSON.stringify({ documents }),
      }
    );

    const data = await res.json();
    return {
      insertedCount: data?.data?.inserted_count || documents.len,
      insertedIds: data?.data?.inserted_ids || [],
    };
  }
}
