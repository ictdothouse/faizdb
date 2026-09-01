import React, { useState, useEffect } from 'react';
import {
  Search,
  Sparkles,
  SlidersHorizontal,
  FileText,
  Tag,
  CheckCircle2,
  Database,
  ArrowRight,
  RefreshCw,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';
import { api } from '../api/client';

interface SearchDocResult {
  _id: string;
  _score: number;
  _matched_terms: string[];
  title?: string;
  category?: string;
  content?: string;
  description?: string;
  tags?: string[];
  [key: string]: any;
}

export const SearchExplorer: React.FC = () => {
  const [collectionName, setCollectionName] = useState<string>('articles');
  const [query, setQuery] = useState<string>('database rust');
  const [fuzzy, setFuzzy] = useState<boolean>(true);
  const [results, setResults] = useState<SearchDocResult[]>([]);
  const [loading, setLoading] = useState<boolean>(false);
  const [durationMs, setDurationMs] = useState<number | null>(null);

  const sampleArticles = [
    {
      title: 'FaizDB: The Next-Gen NoSQL & AI Database',
      category: 'Database Engineering',
      content: 'FaizDB combines hybrid LSM-Tree and B-Tree storage with native HNSW vector search and Raft consensus clustering in Rust.',
      tags: ['database', 'rust', 'nosql', 'faizdb'],
    },
    {
      title: 'Distributed Consensus with Raft Protocol',
      category: 'Distributed Systems',
      content: 'How leader election, term logs, and heartbeat replication guarantee linearizable consistency across cluster nodes.',
      tags: ['raft', 'consensus', 'clustering', 'distributed'],
    },
    {
      title: 'Real-Time Change Data Capture using WebSockets',
      category: 'Streaming Architecture',
      content: 'Sub-millisecond event streaming and UUIDv7 resume tokens for reactive real-time database subscribers.',
      tags: ['streams', 'websockets', 'cdc', 'realtime'],
    },
    {
      title: 'High-Performance GraphRAG & Vector Similarity',
      category: 'Artificial Intelligence',
      content: 'Empowering autonomous AI agents with bidirectional knowledge graphs and 4096-dimension cosine embeddings.',
      tags: ['ai', 'vector', 'graph', 'hnsw', 'rag'],
    },
    {
      title: 'Comparing WiredTiger B-Tree and LSM-Tree Engines',
      category: 'Storage Engines',
      content: 'Deep dive into write amplification, memory tables, WAL write-ahead logging, and SSTable compaction performance.',
      tags: ['storage', 'lsm', 'btree', 'database'],
    },
  ];

  const handleSeedData = async () => {
    setLoading(true);
    try {
      for (const article of sampleArticles) {
        await api.insertDocument(collectionName, article);
      }
      handleSearch();
    } catch (e) {
      console.warn('Seed error:', e);
    } finally {
      setLoading(false);
    }
  };

  const handleSearch = async () => {
    if (!query.trim()) return;
    setLoading(true);
    const start = performance.now();
    try {
      const res = await api.fetch(`${api.getEndpoint()}/v1/collections/${collectionName}/search`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ query: query.trim(), fuzzy, top_k: 10 }),
      });
      if (res.ok) {
        const json = await res.json();
        if (json.success && Array.isArray(json.data)) {
          setResults(json.data);
        }
      }
    } catch (e) {
      console.warn('Search error:', e);
    } finally {
      setDurationMs(Math.round((performance.now() - start) * 100) / 100);
      setLoading(false);
    }
  };

  useEffect(() => {
    handleSearch();
  }, [fuzzy]);

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] overflow-y-auto p-6 space-y-6 bg-slate-50 dark:bg-zinc-950">
      {/* Top Header Card */}
      <div className="p-5 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-indigo-50 dark:bg-indigo-950/60 border border-indigo-200 dark:border-indigo-800 text-indigo-700 dark:text-indigo-400">
            <Search className="w-5 h-5" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold text-slate-900 dark:text-zinc-100">
                Full-Text Search Engine (Okapi BM25 + Inverted Index)
              </h2>
              <Badge variant="info">Sub-ms BM25 Ranker</Badge>
            </div>
            <p className="text-xs text-slate-500 dark:text-zinc-400">
              High-throughput keyword search with stop-word filtering, term weighting, and fuzzy typo tolerance.
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={handleSeedData} loading={loading}>
            <Sparkles className="w-3.5 h-3.5 text-amber-500" />
            <span>Seed Knowledge Base</span>
          </Button>
        </div>
      </div>

      {/* Search Bar & Filter Controls */}
      <div className="p-5 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-4">
        <div className="flex flex-col md:flex-row gap-3">
          <div className="relative flex-1">
            <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 w-4 h-4 text-slate-400 dark:text-zinc-500" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
              placeholder="Search documents by keywords, title, or content (e.g. 'database rust', 'consensus', 'databse')..."
              className="w-full bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg pl-10 pr-4 py-2.5 font-mono text-xs text-slate-900 dark:text-zinc-100 placeholder-slate-400 dark:placeholder-zinc-500 focus:outline-none focus:ring-2 focus:ring-indigo-500"
            />
          </div>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setFuzzy(!fuzzy)}
              className={`flex items-center gap-2 px-3 py-2 rounded-lg border text-xs font-mono transition-all ${
                fuzzy
                  ? 'bg-indigo-50 border-indigo-300 text-indigo-700 dark:bg-indigo-950/40 dark:border-indigo-800 dark:text-indigo-300'
                  : 'bg-white border-slate-200 text-slate-600 dark:bg-zinc-900 dark:border-zinc-800 dark:text-zinc-400'
              }`}
            >
              <SlidersHorizontal className="w-3.5 h-3.5" />
              <span>Fuzzy Typo Tolerance: {fuzzy ? 'ON' : 'OFF'}</span>
            </button>

            <Button variant="primary" size="md" onClick={handleSearch} loading={loading}>
              <span>Search BM25</span>
              <ArrowRight className="w-3.5 h-3.5 ml-1.5" />
            </Button>
          </div>
        </div>

        {/* Quick Suggestion Chips */}
        <div className="flex flex-wrap items-center gap-2 text-xs font-mono text-slate-500 dark:text-zinc-400">
          <span>Try searches:</span>
          {['database rust', 'raft consensus', 'databse (typo test)', 'websockets real-time', 'hnsw graph'].map((term) => (
            <button
              key={term}
              onClick={() => {
                setQuery(term.replace(' (typo test)', ''));
                setTimeout(() => handleSearch(), 50);
              }}
              className="px-2.5 py-1 rounded-md bg-slate-100 dark:bg-zinc-800 text-slate-700 dark:text-zinc-300 hover:bg-indigo-50 hover:text-indigo-600 dark:hover:bg-indigo-950/50 dark:hover:text-indigo-300 transition-colors"
            >
              "{term}"
            </button>
          ))}
        </div>
      </div>

      {/* Search Results Feed */}
      <div className="space-y-3">
        <div className="flex items-center justify-between text-xs font-mono text-slate-500 dark:text-zinc-400 px-1">
          <span>Found {results.length} ranked results</span>
          {durationMs !== null && <span>Query Latency: {durationMs} ms</span>}
        </div>

        {results.length === 0 ? (
          <div className="p-12 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm text-center space-y-3">
            <FileText className="w-8 h-8 text-slate-300 dark:text-zinc-600 mx-auto" />
            <h4 className="text-sm font-semibold text-slate-700 dark:text-zinc-300">
              No matching documents found
            </h4>
            <p className="text-xs text-slate-500 dark:text-zinc-400 max-w-sm mx-auto">
              Try clicking "Seed Knowledge Base" above to populate sample technical articles, or toggle Fuzzy Typo Tolerance.
            </p>
          </div>
        ) : (
          results.map((res, index) => (
            <div
              key={res._id}
              className="p-5 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm hover:border-indigo-300 dark:hover:border-indigo-800 transition-all space-y-2.5"
            >
              <div className="flex items-start justify-between">
                <div className="space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-mono font-bold text-slate-400 dark:text-zinc-500">
                      #{index + 1}
                    </span>
                    <h3 className="text-sm font-bold text-slate-900 dark:text-zinc-100">
                      {res.title || `Document ${res._id}`}
                    </h3>
                  </div>
                  {res.category && (
                    <span className="text-xs font-mono text-indigo-600 dark:text-indigo-400 font-medium">
                      {res.category}
                    </span>
                  )}
                </div>

                <div className="flex items-center gap-2">
                  <Badge variant="success">BM25 Score: {res._score}</Badge>
                </div>
              </div>

              <p className="text-xs text-slate-600 dark:text-zinc-300 leading-relaxed font-sans">
                {res.content || res.description || JSON.stringify(res)}
              </p>

              <div className="flex flex-wrap items-center justify-between pt-2 border-t border-slate-100 dark:border-zinc-800/60 text-xs font-mono">
                <div className="flex items-center gap-1.5">
                  <span className="text-slate-400 dark:text-zinc-500">Matched Terms:</span>
                  {res._matched_terms?.map((term) => (
                    <span
                      key={term}
                      className="px-2 py-0.5 rounded bg-indigo-50 dark:bg-indigo-950/60 text-indigo-700 dark:text-indigo-300 text-[11px] font-semibold"
                    >
                      {term}
                    </span>
                  ))}
                </div>

                <span className="text-[11px] text-slate-400 dark:text-zinc-500">ID: {res._id}</span>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
