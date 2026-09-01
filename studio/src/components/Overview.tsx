import React from 'react';
import {
  Activity,
  Database,
  Cpu,
  ShieldCheck,
  Zap,
  HardDrive,
  Copy,
  Check,
  Terminal,
  Layers,
} from 'lucide-react';
import { Badge } from './ui/Badge';
import { Button } from './ui/Button';

interface OverviewProps {
  stats: {
    totalDocs: number;
    totalSize: number;
    collectionCount: number;
  };
  onNavigateToTab: (tab: any) => void;
}

export const Overview: React.FC<OverviewProps> = ({ stats, onNavigateToTab }) => {
  const [copied, setCopied] = React.useState<string | null>(null);

  const copyToClipboard = (text: string, id: string) => {
    navigator.clipboard.writeText(text);
    setCopied(id);
    setTimeout(() => setCopied(null), 2000);
  };

  const metricCards = [
    {
      label: 'Throughput (Insert)',
      value: '323,424 ops/s',
      sub: 'LSM-Tree Parallel Write Path',
      icon: <Zap className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />,
      badge: 'Benchmarked',
      badgeVariant: 'success' as const,
    },
    {
      label: 'Throughput (Read)',
      value: '671,327 ops/s',
      sub: 'BTreeMap MemTable + Bloom',
      icon: <Activity className="w-4 h-4 text-cyan-600 dark:text-cyan-400" />,
      badge: 'Sub-ms',
      badgeVariant: 'info' as const,
    },
    {
      label: 'Total Documents',
      value: stats.totalDocs.toLocaleString(),
      sub: `Across ${stats.collectionCount} Collections`,
      icon: <Database className="w-4 h-4 text-amber-600 dark:text-amber-400" />,
      badge: 'In Memory + WAL',
      badgeVariant: 'warning' as const,
    },
    {
      label: 'Security & Encryption',
      value: 'AES-256-GCM',
      sub: 'Zero-Trust Data At Rest',
      icon: <ShieldCheck className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />,
      badge: 'Encrypted',
      badgeVariant: 'success' as const,
    },
  ];

  const connectionStrings = [
    {
      id: 'mongo',
      label: '🍃 MongoDB Connection URI (Compass / Mongoose / Prisma)',
      code: 'mongodb://127.0.0.1:27017/faizdb',
    },
    {
      id: 'rest',
      label: '🌐 HTTP / REST API Endpoint',
      code: 'http://127.0.0.1:27018/v1/query',
    },
    {
      id: 'cli',
      label: '💻 Interactive CLI & Shell',
      code: './faizdb shell',
    },
  ];

  return (
    <div className="p-6 space-y-6 overflow-y-auto max-h-[calc(100vh-4rem)]">
      {/* Metric Cards Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
        {metricCards.map((m, idx) => (
          <div
            key={idx}
            className="glass-panel p-5 rounded-xl border border-border transition-all hover:shadow-md"
          >
            <div className="flex items-center justify-between mb-2">
              <span className="text-[11px] font-mono text-muted-foreground uppercase tracking-wider font-semibold">
                {m.label}
              </span>
              <div className="p-1.5 rounded-lg bg-muted border border-border">
                {m.icon}
              </div>
            </div>
            <div className="text-2xl font-bold text-foreground font-mono tracking-tight">
              {m.value}
            </div>
            <div className="mt-2.5 flex items-center justify-between text-[11px] text-muted-foreground">
              <span>{m.sub}</span>
              <Badge variant={m.badgeVariant}>{m.badge}</Badge>
            </div>
          </div>
        ))}
      </div>

      {/* Engine Architecture & Storage Flow */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Storage Engine Status */}
        <div className="lg:col-span-2 glass-panel p-5 rounded-xl border border-border space-y-4">
          <div className="flex items-center justify-between pb-3 border-b border-border">
            <div className="flex items-center gap-2">
              <HardDrive className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
              <h3 className="text-sm font-semibold text-foreground">
                Hybrid LSM-Tree Storage Engine Architecture
              </h3>
            </div>
            <Badge variant="success">Active (Crash-Safe)</Badge>
          </div>

          <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 text-xs">
            <div className="p-3.5 rounded-lg bg-muted/60 border border-border space-y-1.5">
              <div className="flex items-center justify-between">
                <span className="font-semibold text-foreground font-mono">1. WAL Log</span>
                <span className="w-2 h-2 rounded-full bg-emerald-500" />
              </div>
              <p className="text-[11px] text-muted-foreground leading-relaxed">
                Sequential write-ahead log with CRC32 integrity verification.
              </p>
              <div className="text-[10px] font-mono text-emerald-600 dark:text-emerald-400 pt-1 font-semibold">
                Zero Data Loss
              </div>
            </div>

            <div className="p-3.5 rounded-lg bg-muted/60 border border-border space-y-1.5">
              <div className="flex items-center justify-between">
                <span className="font-semibold text-foreground font-mono">2. MemTable</span>
                <span className="w-2 h-2 rounded-full bg-cyan-500" />
              </div>
              <p className="text-[11px] text-muted-foreground leading-relaxed">
                In-memory sorted buffer (BTreeMap + RwLock) for lock-free reads.
              </p>
              <div className="text-[10px] font-mono text-cyan-600 dark:text-cyan-400 pt-1 font-semibold">
                O(log N) Lookup
              </div>
            </div>

            <div className="p-3.5 rounded-lg bg-muted/60 border border-border space-y-1.5">
              <div className="flex items-center justify-between">
                <span className="font-semibold text-foreground font-mono">3. SSTable</span>
                <span className="w-2 h-2 rounded-full bg-amber-500" />
              </div>
              <p className="text-[11px] text-muted-foreground leading-relaxed">
                Immutable disk tables with Bloom filters & background compaction.
              </p>
              <div className="text-[10px] font-mono text-amber-600 dark:text-amber-400 pt-1 font-semibold">
                Leveled Merge
              </div>
            </div>
          </div>

          <div className="p-3 rounded-lg bg-muted/40 border border-border flex items-center justify-between text-xs">
            <div className="flex items-center gap-3">
              <Layers className="w-4 h-4 text-muted-foreground" />
              <div>
                <p className="font-medium text-foreground">Max Document Size Limit</p>
                <p className="text-[11px] text-muted-foreground">
                  FaizDB eliminates MongoDB's 16MB ceiling (up to 256MB per document).
                </p>
              </div>
            </div>
            <Badge variant="info">256 MB Max</Badge>
          </div>
        </div>

        {/* Quick Actions & AI Features */}
        <div className="glass-panel p-5 rounded-xl border border-border space-y-4">
          <div className="flex items-center gap-2 pb-3 border-b border-border">
            <Cpu className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <h3 className="text-sm font-semibold text-foreground">AI-Native Engines</h3>
          </div>

          <div className="space-y-2.5">
            <div
              onClick={() => onNavigateToTab('vector')}
              className="p-3 rounded-lg bg-muted/40 border border-border hover:border-emerald-500/50 hover:bg-muted/80 cursor-pointer transition-all space-y-1"
            >
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold text-emerald-600 dark:text-emerald-400">
                  HNSW Vector Index
                </span>
                <Badge variant="success">Built-in</Badge>
              </div>
              <p className="text-[11px] text-muted-foreground">
                Sub-millisecond similarity search for OpenAI, Gemini & LLaMA embeddings.
              </p>
            </div>

            <div
              onClick={() => onNavigateToTab('graph')}
              className="p-3 rounded-lg bg-muted/40 border border-border hover:border-amber-500/50 hover:bg-muted/80 cursor-pointer transition-all space-y-1"
            >
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold text-amber-600 dark:text-amber-400">
                  GraphRAG Engine
                </span>
                <Badge variant="warning">Traversal</Badge>
              </div>
              <p className="text-[11px] text-muted-foreground">
                Knowledge graph relationships & BFS context retrieval for AI reasoning.
              </p>
            </div>

            <div
              onClick={() => onNavigateToTab('query')}
              className="p-3 rounded-lg bg-muted/40 border border-border hover:border-cyan-500/50 hover:bg-muted/80 cursor-pointer transition-all space-y-1"
            >
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold text-cyan-600 dark:text-cyan-400">
                  FaizQL Multi-Dialect
                </span>
                <Badge variant="info">SQL + Mongo</Badge>
              </div>
              <p className="text-[11px] text-muted-foreground">
                Seamlessly execute SQL statements or MongoDB JSON commands in one engine.
              </p>
            </div>
          </div>
        </div>
      </div>

      {/* Connection Endpoints */}
      <div className="glass-panel p-5 rounded-xl border border-border space-y-3">
        <h3 className="text-sm font-semibold text-foreground flex items-center gap-2">
          <Terminal className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
          <span>Quick Connection URIs for Developers & Applications</span>
        </h3>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
          {connectionStrings.map((conn) => (
            <div
              key={conn.id}
              className="p-3.5 rounded-lg bg-muted/50 border border-border space-y-2 flex flex-col justify-between"
            >
              <div>
                <p className="text-[11px] font-medium text-foreground">{conn.label}</p>
                <p className="text-xs font-mono text-emerald-600 dark:text-emerald-400 bg-background p-2 rounded border border-border mt-1.5 truncate select-all">
                  {conn.code}
                </p>
              </div>
              <Button
                variant="outline"
                size="sm"
                className="w-full mt-2 text-[11px] py-1 bg-card hover:bg-muted"
                onClick={() => copyToClipboard(conn.code, conn.id)}
              >
                {copied === conn.id ? (
                  <>
                    <Check className="w-3.5 h-3.5 text-emerald-500" />
                    <span>Copied!</span>
                  </>
                ) : (
                  <>
                    <Copy className="w-3.5 h-3.5" />
                    <span>Copy String</span>
                  </>
                )}
              </Button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
