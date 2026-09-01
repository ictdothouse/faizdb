import React, { useState } from 'react';
import {
  Play,
  Terminal as TermIcon,
  Clock,
  AlertCircle,
  Download,
  Code2,
  Sparkles,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';
import { api } from '../api/client';

export const QueryConsole: React.FC = () => {
  const [query, setQuery] = useState('SELECT * FROM users');
  const [result, setResult] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [latency, setLatency] = useState<number | null>(null);
  const [viewMode, setViewMode] = useState<'table' | 'json'>('table');

  const templates = [
    { label: 'SQL Select', q: 'SELECT * FROM users' },
    { label: 'SQL Filter', q: 'SELECT * FROM users WHERE active = true' },
    { label: 'Mongo Aggregate ($group)', q: 'db.users.aggregate([\n  { "$match": { "active": true } },\n  { "$group": { "_id": "$country", "count": { "$sum": 1 } } },\n  { "$sort": { "count": -1 } }\n])' },
    { label: 'Mongo Find', q: 'db.users.find({ "country": "Malaysia" })' },
    { label: 'Mongo Insert', q: 'db.users.insert({ "name": "Faiz AI", "role": "Innovator", "country": "Malaysia", "active": true })' },
    { label: 'FaizQL Vector', q: 'FIND users VECTOR NEAR [0.12, 0.85, 0.43, 0.67] TOP 5' },
    { label: 'SQL Count', q: 'SELECT COUNT(*) FROM users' },
  ];

  const handleRunQuery = async () => {
    if (!query.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const res = await api.query(query.trim());
      setResult(res.data);
      setLatency(res.durationMs);
    } catch (err: any) {
      setError(err.message || 'Query execution failed');
      setResult(null);
    } finally {
      setLoading(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      handleRunQuery();
    }
  };

  const downloadJson = () => {
    if (!result) return;
    const blob = new Blob([JSON.stringify(result, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `faizdb_query_result_${Date.now()}.json`;
    a.click();
  };

  const isArrayResult = Array.isArray(result);
  const columns = isArrayResult && result.length > 0 ? Object.keys(result[0]) : [];

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] p-4 space-y-4 overflow-hidden bg-slate-50 dark:bg-zinc-950">
      {/* Top Query Editor Card */}
      <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-3 shrink-0">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <TermIcon className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <span className="text-xs font-semibold text-slate-900 dark:text-zinc-100 font-mono">
              Query Workspace (SQL / MongoDB / FaizQL)
            </span>
          </div>

          <div className="flex items-center gap-2">
            <span className="text-[11px] text-slate-500 dark:text-zinc-400 font-mono hidden sm:inline">
              Press ⌘+Enter or Ctrl+Enter to execute
            </span>
            <Button
              variant="primary"
              size="sm"
              onClick={handleRunQuery}
              loading={loading}
              className="px-4"
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>Run Query</span>
            </Button>
          </div>
        </div>

        {/* Quick Query Templates */}
        <div className="flex items-center gap-1.5 overflow-x-auto pb-1 text-xs">
          <span className="text-[11px] text-slate-500 dark:text-zinc-400 font-mono shrink-0 mr-1 flex items-center gap-1">
            <Sparkles className="w-3 h-3 text-amber-500" /> Templates:
          </span>
          {templates.map((t, i) => (
            <button
              key={i}
              onClick={() => setQuery(t.q)}
              className="px-2 py-1 rounded bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 hover:border-emerald-500 text-slate-800 dark:text-zinc-200 text-[11px] font-mono whitespace-nowrap transition-colors"
            >
              {t.label}
            </button>
          ))}
        </div>

        {/* Query Input Textarea */}
        <div className="relative">
          <textarea
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            rows={4}
            className="w-full bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg p-3 font-mono text-xs text-slate-900 dark:text-emerald-300 placeholder:text-slate-400 dark:placeholder:text-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-500 leading-relaxed resize-y"
            placeholder="Type SQL (SELECT * FROM table), MongoDB (db.table.find()), or FaizQL (FIND table VECTOR NEAR [...])..."
          />
        </div>
      </div>

      {/* Query Output / Results Panel */}
      <div className="flex-1 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm flex flex-col overflow-hidden">
        {/* Output Header Bar */}
        <div className="p-3 border-b border-slate-200 dark:border-zinc-800 bg-slate-50/80 dark:bg-zinc-900/90 flex items-center justify-between text-xs">
          <div className="flex items-center gap-3">
            <span className="font-semibold text-slate-900 dark:text-zinc-100">Execution Result</span>
            {latency !== null && (
              <Badge variant="success" className="gap-1">
                <Clock className="w-3 h-3" />
                <span>{latency} ms</span>
              </Badge>
            )}
            {error && (
              <Badge variant="warning" className="gap-1">
                <AlertCircle className="w-3 h-3 text-rose-500" />
                <span className="text-rose-600 dark:text-rose-300">Execution Error</span>
              </Badge>
            )}
            {result !== null && isArrayResult && (
              <span className="text-[11px] text-slate-500 dark:text-zinc-400 font-mono">
                {result.length} row(s) returned
              </span>
            )}
          </div>

          <div className="flex items-center gap-2">
            {isArrayResult && result.length > 0 && (
              <div className="flex items-center bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 rounded-lg p-0.5">
                <button
                  onClick={() => setViewMode('table')}
                  className={`px-2 py-1 rounded text-[11px] font-mono transition-colors ${
                    viewMode === 'table' ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs' : 'text-slate-500 dark:text-zinc-400'
                  }`}
                >
                  Table
                </button>
                <button
                  onClick={() => setViewMode('json')}
                  className={`px-2 py-1 rounded text-[11px] font-mono transition-colors ${
                    viewMode === 'json' ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs' : 'text-slate-500 dark:text-zinc-400'
                  }`}
                >
                  JSON
                </button>
              </div>
            )}

            {result !== null && (
              <Button variant="outline" size="sm" onClick={downloadJson}>
                <Download className="w-3.5 h-3.5" />
                <span className="hidden sm:inline">Export</span>
              </Button>
            )}
          </div>
        </div>

        {/* Output Body */}
        <div className="flex-1 overflow-auto p-4 font-mono text-xs bg-slate-50 dark:bg-zinc-950">
          {loading ? (
            <div className="flex flex-col items-center justify-center h-full gap-2 text-slate-500 dark:text-zinc-400">
              <div className="w-5 h-5 border-2 border-emerald-500 border-t-transparent rounded-full animate-spin" />
              <span>Executing against FaizDB engine...</span>
            </div>
          ) : error ? (
            <div className="p-4 rounded-lg bg-rose-50 dark:bg-rose-950/30 border border-rose-200 dark:border-rose-800/60 text-rose-700 dark:text-rose-300 space-y-1">
              <p className="font-semibold flex items-center gap-1.5">
                <AlertCircle className="w-4 h-4 text-rose-600" /> Error Executing Query
              </p>
              <p className="text-xs font-mono whitespace-pre-wrap">{error}</p>
            </div>
          ) : result === null ? (
            <div className="flex flex-col items-center justify-center h-full text-slate-400 dark:text-zinc-500 gap-2 text-center">
              <Code2 className="w-8 h-8 stroke-1 text-slate-300 dark:text-zinc-600" />
              <p>Enter a query above and click "Run Query" to see instant results.</p>
            </div>
          ) : isArrayResult && result.length > 0 && viewMode === 'table' ? (
            <table className="w-full text-left border-collapse font-mono">
              <thead>
                <tr className="border-b border-slate-200 dark:border-zinc-800 bg-slate-100 dark:bg-zinc-900 sticky top-0">
                  {columns.map((col) => (
                    <th key={col} className="py-2 px-3 font-semibold text-slate-800 dark:text-zinc-200">
                      {col}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-200 dark:divide-zinc-800">
                {result.map((row, idx) => (
                  <tr key={idx} className="hover:bg-slate-100/50 dark:hover:bg-zinc-800/30">
                    {columns.map((col) => (
                      <td key={col} className="py-2 px-3 text-slate-800 dark:text-zinc-200 max-w-xs truncate">
                        {typeof row[col] === 'object'
                          ? JSON.stringify(row[col])
                          : String(row[col] ?? '-')}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <pre className="text-slate-900 dark:text-emerald-300 bg-white dark:bg-zinc-900 p-4 rounded-lg border border-slate-200 dark:border-zinc-800 overflow-x-auto leading-relaxed shadow-xs">
              {JSON.stringify(result, null, 2)}
            </pre>
          )}
        </div>
      </div>
    </div>
  );
};
