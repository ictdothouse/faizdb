import React, { useState } from 'react';
import {
  Play,
  Terminal as TermIcon,
  Clock,
  AlertCircle,
  Download,
  Code2,
  Sparkles,
  Zap,
  Layers,
  CheckCircle2,
  Activity,
  GitBranch,
  Flame,
  Copy,
  Check,
  AlertTriangle,
  Database,
  Network,
  Gauge,
  ChevronRight,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';
import { api } from '../api/client';

interface PlanNode {
  node_type: string;
  relation?: string;
  condition?: string;
  estimated_cost_start: number;
  estimated_cost_total: number;
  estimated_rows: number;
  actual_time_start_us?: number;
  actual_time_total_us?: number;
  actual_rows?: number;
  loops: number;
  details?: string;
  children?: PlanNode[];
}

interface ShardMetric {
  shard_id: number;
  partition_name: string;
  execution_time_us: number;
  rows_scanned: number;
  rows_emitted: number;
  cache_hit_pct: number;
  network_transfer_bytes: number;
  status: string;
}

interface ExplainData {
  plan_type: string;
  collection: string;
  index_used?: string;
  execution_time_us: number;
  documents_examined: number;
  documents_returned: number;
  is_unique: boolean;
  estimated_cost_score: number;
  estimated_selectivity_pct?: number;
  seq_scan_cost?: number;
  index_scan_cost?: number;
  optimization_rationale?: string;
  is_analyze?: boolean;
  join_strategy?: string;
  estimated_network_io_bytes?: number;
  actual_network_io_bytes?: number;
  cache_hits?: number;
  cache_misses?: number;
  shards_involved?: number;
  warning?: string;
  shard_metrics?: ShardMetric[];
  node_tree?: PlanNode;
  formatted_pg_tree?: string;
}

export const QueryConsole: React.FC = () => {
  const [query, setQuery] = useState('SELECT * FROM users');
  const [result, setResult] = useState<any>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [latency, setLatency] = useState<number | null>(null);
  const [viewMode, setViewMode] = useState<'table' | 'json'>('table');
  const [explainTab, setExplainTab] = useState<'tree' | 'flame' | 'raw'>('tree');
  const [copied, setCopied] = useState(false);

  const templates = [
    {
      label: 'EXPLAIN ANALYZE Join',
      q: 'EXPLAIN ANALYZE SELECT * FROM orders JOIN customers ON orders.customer_id = customers.id',
    },
    {
      label: 'Colocated Shard Join',
      q: 'EXPLAIN ANALYZE SELECT * FROM {tenant_1}:orders JOIN {tenant_1}:customers ON {tenant_1}:orders.customer_id = {tenant_1}:customers.id',
    },
    { label: 'SQL Select', q: 'SELECT * FROM users' },
    { label: 'EXPLAIN Plan', q: 'EXPLAIN SELECT * FROM users WHERE email = "faiz@ict.house"' },
    { label: 'Create Unique Index', q: 'CREATE UNIQUE INDEX idx_email ON users(email)' },
    { label: 'SQL Filter', q: 'SELECT * FROM users WHERE active = true' },
    {
      label: 'Mongo Aggregate',
      q: 'db.users.aggregate([\n  { "$match": { "active": true } },\n  { "$group": { "_id": "$country", "count": { "$sum": 1 } } },\n  { "$sort": { "count": -1 } }\n])',
    },
    { label: 'Mongo Find', q: 'db.users.find({ "country": "Malaysia" })' },
    { label: 'FaizQL Vector', q: 'FIND users VECTOR NEAR [0.12, 0.85, 0.43, 0.67] TOP 5' },
    { label: 'Begin Transaction', q: 'BEGIN' },
    { label: 'Commit Transaction', q: 'COMMIT' },
  ];

  const handleRunQuery = async (customQuery?: string) => {
    const q = (customQuery || query).trim();
    if (!q) return;
    setLoading(true);
    setError(null);
    try {
      const res = await api.query(q);
      setResult(res.data);
      setLatency(res.durationMs);
    } catch (err: any) {
      setError(err.message || 'Query execution failed');
      setResult(null);
    } finally {
      setLoading(false);
    }
  };

  const handleExplain = (analyze: boolean = false) => {
    const trimmed = query.trim();
    const clean = trimmed.replace(/^EXPLAIN\s+(ANALYZE\s+)?/i, '').replace(/^\(ANALYZE,\s*VERBOSE\)\s+/i, '');
    const prefix = analyze ? 'EXPLAIN ANALYZE ' : 'EXPLAIN ';
    const fullQuery = `${prefix}${clean}`;
    setQuery(fullQuery);
    handleRunQuery(fullQuery);
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

  const copyText = (text: string) => {
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const formatBytes = (bytes: number): string => {
    if (bytes === 0) return '0 B';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
  };

  const explainPlan: ExplainData | null =
    result && typeof result === 'object' && 'Explain' in result ? result.Explain : null;
  const isArrayResult = Array.isArray(result);
  const columns = isArrayResult && result.length > 0 ? Object.keys(result[0]) : [];

  // Recursive Tree Node Renderer
  const renderTreeNode = (node: PlanNode, depth: number = 0) => {
    const isColocated = node.node_type.includes('Colocated');
    const isFallback = node.node_type.includes('FALLBACK') || node.node_type.includes('Fallback');
    const isBroadcast = node.node_type.includes('Broadcast');

    return (
      <div key={depth + '-' + node.node_type} className="flex flex-col space-y-2">
        <div
          className={`p-3 rounded-lg border transition-all ${
            isColocated
              ? 'border-emerald-500/40 bg-emerald-950/20 dark:bg-emerald-950/30'
              : isFallback
              ? 'border-amber-500/50 bg-amber-950/20 dark:bg-amber-950/30'
              : isBroadcast
              ? 'border-cyan-500/40 bg-cyan-950/20 dark:bg-cyan-950/30'
              : 'border-slate-200 dark:border-zinc-800 bg-white dark:bg-zinc-900/90'
          }`}
          style={{ marginLeft: `${depth * 20}px` }}
        >
          <div className="flex items-center justify-between gap-2 flex-wrap">
            <div className="flex items-center gap-2">
              <ChevronRight className="w-3.5 h-3.5 text-emerald-500 shrink-0" />
              <span className="font-semibold text-slate-900 dark:text-zinc-100 text-xs">
                {node.node_type}
              </span>
              {node.relation && (
                <span className="px-1.5 py-0.5 rounded text-[10px] bg-slate-100 dark:bg-zinc-800 text-slate-700 dark:text-zinc-300 font-mono">
                  {node.relation}
                </span>
              )}
            </div>

            <div className="flex items-center gap-2 text-[11px] font-mono">
              <span className="text-slate-500 dark:text-zinc-400">
                Cost: {node.estimated_cost_start.toFixed(1)}..{node.estimated_cost_total.toFixed(1)}
              </span>
              <span className="text-emerald-600 dark:text-emerald-400 font-bold">
                Rows: {node.actual_rows ?? node.estimated_rows}
              </span>
              {node.actual_time_total_us !== undefined && (
                <span className="text-sky-600 dark:text-sky-400">
                  Time: {(node.actual_time_total_us / 1000).toFixed(2)} ms
                </span>
              )}
            </div>
          </div>

          {node.condition && (
            <div className="mt-1.5 text-[10px] text-slate-600 dark:text-zinc-400 font-mono bg-slate-50 dark:bg-zinc-950/60 p-1.5 rounded border border-slate-100 dark:border-zinc-800/80">
              Condition: {node.condition}
            </div>
          )}

          {node.details && (
            <div className="mt-1 text-[10px] text-slate-500 dark:text-zinc-400 italic">
              {node.details}
            </div>
          )}
        </div>

        {node.children && node.children.length > 0 && (
          <div className="space-y-2 border-l border-slate-200 dark:border-zinc-800 ml-4 pl-2">
            {node.children.map((child, idx) => renderTreeNode(child, depth + 1))}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] p-4 space-y-4 overflow-hidden bg-slate-50 dark:bg-zinc-950">
      {/* Top Query Editor Card */}
      <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-3 shrink-0">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <TermIcon className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <span className="text-xs font-semibold text-slate-900 dark:text-zinc-100 font-mono">
              Query Workspace (SQL / openCypher / FaizQL / EXPLAIN ANALYZE)
            </span>
          </div>

          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => handleExplain(false)}
              loading={loading}
              className="border-slate-300 dark:border-zinc-700 text-slate-700 dark:text-zinc-300 hover:bg-slate-100 dark:hover:bg-zinc-800"
            >
              <Zap className="w-3.5 h-3.5 text-amber-500" />
              <span>Dry Explain</span>
            </Button>

            <Button
              variant="outline"
              size="sm"
              onClick={() => handleExplain(true)}
              loading={loading}
              className="border-emerald-600/40 text-emerald-600 dark:text-emerald-400 hover:bg-emerald-50 dark:hover:bg-emerald-950/20 font-bold"
            >
              <Activity className="w-3.5 h-3.5 fill-current" />
              <span>Explain Analyze</span>
            </Button>

            <Button
              variant="primary"
              size="sm"
              onClick={() => handleRunQuery()}
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
              className="px-2 py-1 rounded bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 hover:border-emerald-500 text-slate-800 dark:text-zinc-200 text-[11px] font-mono whitespace-nowrap transition-colors cursor-pointer"
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
            rows={3}
            placeholder="Type SQL (SELECT...), MongoDB (db.users.find...), or EXPLAIN ANALYZE..."
            className="w-full p-3 rounded-lg bg-slate-50 dark:bg-zinc-950 border border-slate-200 dark:border-zinc-800 text-slate-900 dark:text-zinc-100 font-mono text-xs focus:ring-1 focus:ring-emerald-500 focus:border-emerald-500 outline-hidden resize-y"
          />
        </div>
      </div>

      {/* Query Result / Execution Plan Output Card */}
      <div className="flex-1 flex flex-col min-h-0 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm overflow-hidden">
        {/* Output Header Bar */}
        <div className="px-4 py-2.5 bg-slate-50/50 dark:bg-zinc-900/50 border-b border-slate-200 dark:border-zinc-800 flex items-center justify-between shrink-0">
          <div className="flex items-center gap-3">
            <span className="text-xs font-semibold text-slate-900 dark:text-zinc-100">
              Execution Result
            </span>
            {latency !== null && (
              <Badge variant="outline" className="flex items-center gap-1 font-mono text-[10px]">
                <Clock className="w-3 h-3 text-emerald-500" />
                {latency.toFixed(2)} ms
              </Badge>
            )}
            {isArrayResult && (
              <span className="text-xs text-slate-500 dark:text-zinc-400 font-mono">
                {result.length} document(s)
              </span>
            )}
            {explainPlan && (
              <Badge variant="success" className="font-mono text-[10px] flex items-center gap-1">
                <Zap className="w-3 h-3" />
                {explainPlan.is_analyze ? 'EXPLAIN ANALYZE ACTIVE' : 'EXPLAIN PLAN (DRY-RUN)'}
              </Badge>
            )}
          </div>

          <div className="flex items-center gap-2">
            {explainPlan && (
              <div className="flex items-center bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 rounded-lg p-0.5">
                <button
                  onClick={() => setExplainTab('tree')}
                  className={`px-2.5 py-1 rounded text-[11px] font-mono transition-colors flex items-center gap-1 cursor-pointer ${
                    explainTab === 'tree'
                      ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs font-bold'
                      : 'text-slate-500 dark:text-zinc-400 hover:text-slate-900 dark:hover:text-zinc-100'
                  }`}
                >
                  <GitBranch className="w-3 h-3 text-emerald-500" />
                  <span>Node Tree</span>
                </button>
                <button
                  onClick={() => setExplainTab('flame')}
                  className={`px-2.5 py-1 rounded text-[11px] font-mono transition-colors flex items-center gap-1 cursor-pointer ${
                    explainTab === 'flame'
                      ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs font-bold'
                      : 'text-slate-500 dark:text-zinc-400 hover:text-slate-900 dark:hover:text-zinc-100'
                  }`}
                >
                  <Flame className="w-3 h-3 text-amber-500" />
                  <span>Shard Flame</span>
                </button>
                <button
                  onClick={() => setExplainTab('raw')}
                  className={`px-2.5 py-1 rounded text-[11px] font-mono transition-colors flex items-center gap-1 cursor-pointer ${
                    explainTab === 'raw'
                      ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs font-bold'
                      : 'text-slate-500 dark:text-zinc-400 hover:text-slate-900 dark:hover:text-zinc-100'
                  }`}
                >
                  <TermIcon className="w-3 h-3 text-sky-500" />
                  <span>PostgreSQL Wire</span>
                </button>
              </div>
            )}

            {isArrayResult && result.length > 0 && (
              <div className="flex items-center bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 rounded-lg p-0.5">
                <button
                  onClick={() => setViewMode('table')}
                  className={`px-2 py-1 rounded text-[11px] font-mono transition-colors ${
                    viewMode === 'table'
                      ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs'
                      : 'text-slate-500 dark:text-zinc-400'
                  }`}
                >
                  Table
                </button>
                <button
                  onClick={() => setViewMode('json')}
                  className={`px-2 py-1 rounded text-[11px] font-mono transition-colors ${
                    viewMode === 'json'
                      ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs'
                      : 'text-slate-500 dark:text-zinc-400'
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
          ) : explainPlan ? (
            /* Visual Explain Plan Diagnostic Suite */
            <div className="space-y-4 max-w-4xl mx-auto py-2">
              {/* Executive Strategy Banner */}
              <div className="p-4 rounded-xl border border-emerald-500/30 bg-emerald-950/10 dark:bg-emerald-950/20 backdrop-blur-sm space-y-4">
                <div className="flex items-center justify-between border-b border-emerald-500/20 pb-3 flex-wrap gap-2">
                  <div className="flex items-center gap-2.5">
                    <Zap className="w-5 h-5 text-emerald-400 shrink-0" />
                    <div>
                      <h4 className="text-sm font-bold text-slate-900 dark:text-white flex items-center gap-2">
                        Distributed Query Execution Diagnostics
                        {explainPlan.is_analyze && (
                          <span className="px-2 py-0.5 rounded text-[10px] bg-emerald-500/20 text-emerald-400 border border-emerald-500/40">
                            ANALYZE
                          </span>
                        )}
                      </h4>
                      <p className="text-[11px] text-zinc-400">
                        FaizDB Cost-Based Distributed Optimizer & Physical Execution Plan
                      </p>
                    </div>
                  </div>

                  <div className="flex items-center gap-2">
                    {explainPlan.join_strategy === 'ColocatedHashJoin' ? (
                      <span className="px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider bg-emerald-500/20 text-emerald-300 border border-emerald-500/50 flex items-center gap-1">
                        <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
                        COLOCATED (0 NETWORK I/O)
                      </span>
                    ) : explainPlan.join_strategy === 'BroadcastHashJoin' ? (
                      <span className="px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider bg-cyan-500/20 text-cyan-300 border border-cyan-500/50 flex items-center gap-1">
                        <Network className="w-3.5 h-3.5 text-cyan-400" />
                        BROADCAST HASH JOIN
                      </span>
                    ) : explainPlan.join_strategy === 'DistributedIndexNestedLoopFallback' ? (
                      <span className="px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider bg-amber-500/20 text-amber-300 border border-amber-500/50 flex items-center gap-1">
                        <AlertTriangle className="w-3.5 h-3.5 text-amber-400" />
                        FALLBACK (CIRCUIT BREAKER)
                      </span>
                    ) : (
                      <span className="px-2.5 py-1 rounded-full text-xs font-bold uppercase tracking-wider bg-emerald-500/20 text-emerald-400 border border-emerald-500/40">
                        {explainPlan.plan_type}
                      </span>
                    )}
                  </div>
                </div>

                {/* 4 Essential Core Metrics */}
                <div className="grid grid-cols-2 sm:grid-cols-4 gap-3">
                  <div className="p-3 rounded-lg bg-white dark:bg-zinc-900/80 border border-zinc-200 dark:border-zinc-800">
                    <span className="text-[10px] text-zinc-400 uppercase tracking-wider block flex items-center gap-1">
                      <Clock className="w-3 h-3 text-emerald-400" /> Engine Latency
                    </span>
                    <span className="text-base font-bold text-emerald-400 font-mono">
                      {explainPlan.execution_time_us} µs
                    </span>
                    <span className="text-[10px] text-zinc-500 block">
                      {(explainPlan.execution_time_us / 1000).toFixed(3)} ms total
                    </span>
                  </div>

                  <div className="p-3 rounded-lg bg-white dark:bg-zinc-900/80 border border-zinc-200 dark:border-zinc-800">
                    <span className="text-[10px] text-zinc-400 uppercase tracking-wider block flex items-center gap-1">
                      <Network className="w-3 h-3 text-sky-400" /> Network Stream
                    </span>
                    <span className="text-base font-bold text-sky-400 font-mono">
                      {formatBytes(explainPlan.actual_network_io_bytes || 0)}
                    </span>
                    <span className="text-[10px] text-zinc-500 block">
                      Est: {formatBytes(explainPlan.estimated_network_io_bytes || 0)}
                    </span>
                  </div>

                  <div className="p-3 rounded-lg bg-white dark:bg-zinc-900/80 border border-zinc-200 dark:border-zinc-800">
                    <span className="text-[10px] text-zinc-400 uppercase tracking-wider block flex items-center gap-1">
                      <Gauge className="w-3 h-3 text-amber-400" /> Cache Efficiency
                    </span>
                    <span className="text-base font-bold text-amber-400 font-mono">
                      {explainPlan.cache_hits ?? 1}/
                      {(explainPlan.cache_hits ?? 1) + (explainPlan.cache_misses ?? 0)}
                    </span>
                    <span className="text-[10px] text-emerald-500 font-semibold block">
                      {(
                        ((explainPlan.cache_hits ?? 1) /
                          Math.max(1, (explainPlan.cache_hits ?? 1) + (explainPlan.cache_misses ?? 0))) *
                        100
                      ).toFixed(1)}
                      % Local Hit
                    </span>
                  </div>

                  <div className="p-3 rounded-lg bg-white dark:bg-zinc-900/80 border border-zinc-200 dark:border-zinc-800">
                    <span className="text-[10px] text-zinc-400 uppercase tracking-wider block flex items-center gap-1">
                      <Database className="w-3 h-3 text-indigo-400" /> Cluster Shards
                    </span>
                    <span className="text-base font-bold text-indigo-400 font-mono">
                      {explainPlan.shards_involved ?? 16} Virtual
                    </span>
                    <span className="text-[10px] text-zinc-500 block">
                      Docs: {explainPlan.documents_examined} ➔ {explainPlan.documents_returned}
                    </span>
                  </div>
                </div>

                {/* Smart Tuning & Optimization Advice Alert */}
                {explainPlan.warning ? (
                  <div className="p-3 rounded-lg bg-amber-500/10 border border-amber-500/40 text-amber-300 space-y-1">
                    <div className="flex items-center gap-1.5 font-bold text-xs">
                      <AlertTriangle className="w-4 h-4 text-amber-400 shrink-0" />
                      Distributed Spill Warning: High Network Overhead Detected
                    </div>
                    <p className="text-[11px] text-amber-200/90 font-mono">
                      {explainPlan.warning}
                    </p>
                    <p className="text-[11px] text-amber-300/80 pt-1 border-t border-amber-500/20">
                      💡 <strong>Cadangan Penalaan:</strong> Sisipkan tag kunci colocated menggunakan kurungan{' '}
                      <code className="bg-amber-950 px-1 py-0.5 rounded text-amber-200">{`{tenant_id}`}</code>{' '}
                      pada jadual kiri dan kanan (cth:{' '}
                      <code className="bg-amber-950 px-1 py-0.5 rounded text-amber-200">{`{tenant_1}:orders JOIN {tenant_1}:customers`}</code>
                      ) untuk menjamin pemprosesan 100% dalam memori tempatan dengan 0 bait I/O rangkaian.
                    </p>
                  </div>
                ) : explainPlan.join_strategy === 'ColocatedHashJoin' ? (
                  <div className="p-3 rounded-lg bg-emerald-500/10 border border-emerald-500/40 text-emerald-300">
                    <div className="flex items-center gap-1.5 font-bold text-xs">
                      <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                      Zero Network Serialization: Optimal Hash-Tag Colocated Shard
                    </div>
                    <p className="text-[11px] text-emerald-200/90 mt-0.5">
                      Kueri ini telah memanfaatkan sepenuhnya Colocated Hash Tag. Semua baris padanan diproses terus
                      dalam memori nod tempatan ($O(N + M)$) tanpa melalui lapisan rangkaian RPC.
                    </p>
                  </div>
                ) : null}

                {/* Sub-view Content */}
                {explainTab === 'tree' && (
                  <div className="space-y-3 pt-2">
                    <div className="flex items-center justify-between text-xs text-zinc-400 font-mono border-b border-zinc-800 pb-2">
                      <span className="flex items-center gap-1.5 font-semibold text-zinc-200">
                        <GitBranch className="w-3.5 h-3.5 text-emerald-400" />
                        Interactive Execution Node Tree
                      </span>
                      <span>Target: {explainPlan.collection}</span>
                    </div>

                    {explainPlan.node_tree ? (
                      renderTreeNode(explainPlan.node_tree)
                    ) : (
                      <div className="p-3 rounded-lg bg-zinc-900 border border-zinc-800 space-y-1">
                        <div className="flex justify-between">
                          <span>Execution Plan:</span>
                          <span className="text-white font-bold">{explainPlan.plan_type}</span>
                        </div>
                        <div className="flex justify-between">
                          <span>Index Applied:</span>
                          <span className="text-emerald-400 font-bold">
                            {explainPlan.index_used || 'None (Full Sequential Scan)'}
                          </span>
                        </div>
                        <div className="flex justify-between">
                          <span>Estimated Optimizer Cost:</span>
                          <span className="text-white font-bold">
                            {explainPlan.estimated_cost_score.toFixed(2)}
                          </span>
                        </div>
                      </div>
                    )}
                  </div>
                )}

                {explainTab === 'flame' && (
                  <div className="space-y-3 pt-2">
                    <div className="flex items-center justify-between text-xs text-zinc-400 font-mono border-b border-zinc-800 pb-2">
                      <span className="flex items-center gap-1.5 font-semibold text-zinc-200">
                        <Flame className="w-3.5 h-3.5 text-amber-400" />
                        Per-Shard Latency Breakdown & Straggler Identification
                      </span>
                      <span>{explainPlan.shard_metrics?.length || 4} Participating Shards</span>
                    </div>

                    <div className="space-y-2">
                      {(explainPlan.shard_metrics || [
                        {
                          shard_id: 0,
                          partition_name: 'shard-00',
                          execution_time_us: 15,
                          rows_scanned: 25,
                          rows_emitted: 5,
                          cache_hit_pct: 100,
                          network_transfer_bytes: 0,
                          status: 'LOCAL_FAST_PATH',
                        },
                        {
                          shard_id: 1,
                          partition_name: 'shard-01',
                          execution_time_us: 18,
                          rows_scanned: 25,
                          rows_emitted: 5,
                          cache_hit_pct: 100,
                          network_transfer_bytes: 0,
                          status: 'LOCAL_FAST_PATH',
                        },
                        {
                          shard_id: 2,
                          partition_name: 'shard-02',
                          execution_time_us: 22,
                          rows_scanned: 25,
                          rows_emitted: 5,
                          cache_hit_pct: 100,
                          network_transfer_bytes: 0,
                          status: 'LOCAL_FAST_PATH',
                        },
                        {
                          shard_id: 3,
                          partition_name: 'shard-03',
                          execution_time_us: 16,
                          rows_scanned: 25,
                          rows_emitted: 5,
                          cache_hit_pct: 100,
                          network_transfer_bytes: 0,
                          status: 'LOCAL_FAST_PATH',
                        },
                      ]).map((sm, idx, arr) => {
                        const maxTime = Math.max(...arr.map((s) => s.execution_time_us));
                        const pctWidth = Math.max(15, Math.round((sm.execution_time_us / maxTime) * 100));
                        const isStraggler = sm.execution_time_us === maxTime && arr.length > 1;

                        return (
                          <div
                            key={sm.shard_id}
                            className="p-2.5 rounded-lg bg-zinc-900/90 border border-zinc-800 space-y-1.5"
                          >
                            <div className="flex items-center justify-between text-xs">
                              <div className="flex items-center gap-2">
                                <span className="font-bold text-white">{sm.partition_name}</span>
                                <span
                                  className={`px-1.5 py-0.5 rounded text-[10px] font-mono ${
                                    sm.status === 'LOCAL_FAST_PATH'
                                      ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/40'
                                      : sm.status === 'STREAMING'
                                      ? 'bg-cyan-500/20 text-cyan-400 border border-cyan-500/40'
                                      : 'bg-amber-500/20 text-amber-400 border border-amber-500/40'
                                  }`}
                                >
                                  {sm.status}
                                </span>
                                {isStraggler && (
                                  <span className="px-1.5 py-0.5 rounded text-[9px] bg-rose-500/20 text-rose-400 border border-rose-500/40 font-bold animate-pulse">
                                    SLOWEST NODE
                                  </span>
                                )}
                              </div>
                              <div className="flex items-center gap-3 text-[11px] text-zinc-400">
                                <span>Cache: {sm.cache_hit_pct.toFixed(0)}%</span>
                                <span>Net: {formatBytes(sm.network_transfer_bytes)}</span>
                                <span className="font-bold text-emerald-400">{sm.execution_time_us} µs</span>
                              </div>
                            </div>

                            {/* Timeline Bar */}
                            <div className="w-full bg-zinc-950 h-2.5 rounded-full overflow-hidden border border-zinc-800">
                              <div
                                className={`h-full rounded-full transition-all ${
                                  isStraggler
                                    ? 'bg-gradient-to-r from-amber-500 to-rose-500'
                                    : 'bg-gradient-to-r from-emerald-500 to-teal-400'
                                }`}
                                style={{ width: `${pctWidth}%` }}
                              />
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                )}

                {explainTab === 'raw' && (
                  <div className="space-y-2 pt-2">
                    <div className="flex items-center justify-between text-xs text-zinc-400 font-mono border-b border-zinc-800 pb-2">
                      <span className="flex items-center gap-1.5 font-semibold text-zinc-200">
                        <TermIcon className="w-3.5 h-3.5 text-sky-400" />
                        PostgreSQL Wire Protocol Execution Plan Output
                      </span>
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() =>
                          copyText(explainPlan.formatted_pg_tree || 'No plan available')
                        }
                        className="h-6 px-2 text-[10px]"
                      >
                        {copied ? (
                          <>
                            <Check className="w-3 h-3 text-emerald-400" />
                            <span>Copied</span>
                          </>
                        ) : (
                          <>
                            <Copy className="w-3 h-3" />
                            <span>Copy Plan</span>
                          </>
                        )}
                      </Button>
                    </div>

                    <pre className="p-3 rounded-lg bg-zinc-950 border border-zinc-800 text-[11px] text-emerald-300 font-mono overflow-x-auto leading-relaxed whitespace-pre">
                      {explainPlan.formatted_pg_tree ||
                        `Distributed Plan: ${explainPlan.plan_type} on ${explainPlan.collection}\nExecution Time: ${explainPlan.execution_time_us} µs\nCost Score: ${explainPlan.estimated_cost_score.toFixed(2)}`}
                    </pre>
                  </div>
                )}
              </div>
            </div>
          ) : result === null ? (
            <div className="flex flex-col items-center justify-center h-full text-slate-400 dark:text-zinc-500 gap-2 text-center">
              <Code2 className="w-8 h-8 stroke-1 text-slate-300 dark:text-zinc-600" />
              <p>
                Enter a query above and click &quot;Run Query&quot;, &quot;Dry Explain&quot;, or &quot;Explain
                Analyze&quot; to see instant results.
              </p>
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
