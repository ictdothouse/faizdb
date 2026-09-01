import React from 'react';
import { RefreshCw, Terminal, Plus, Zap, Sun, Moon } from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';
import { NavTab } from './Sidebar';

interface HeaderProps {
  currentTab: NavTab;
  selectedCollection: string;
  onRefresh: () => void;
  onOpenInsertModal: () => void;
  onQuickQuery: () => void;
  isRefreshing: boolean;
  theme: 'light' | 'dark';
  onToggleTheme: () => void;
}

export const Header: React.FC<HeaderProps> = ({
  currentTab,
  selectedCollection,
  onRefresh,
  onOpenInsertModal,
  onQuickQuery,
  isRefreshing,
  theme,
  onToggleTheme,
}) => {
  const titles: Record<NavTab, { title: string; desc: string }> = {
    overview: {
      title: 'Cluster Overview & Telemetry',
      desc: 'Real-time performance metrics, LSM storage stats, and memory utilization',
    },
    tables: {
      title: `Collection: ${selectedCollection}`,
      desc: 'Visual document spreadsheet, schema inspector, and JSON tree editor',
    },
    query: {
      title: 'FaizQL Multi-Dialect Console',
      desc: 'Execute SQL queries, MongoDB queries, and AI vector instructions with sub-ms execution',
    },
    stream: {
      title: 'Real-Time Change Stream Monitor',
      desc: 'Live WebSocket Change Data Capture (CDC) events, resume tokens & reactive subscriptions',
    },
    cluster: {
      title: 'Distributed Cluster & Sharding Manager',
      desc: 'Raft consensus multi-node topology, automated failover & 16,384 virtual hash slots',
    },
    search: {
      title: 'Full-Text Search Engine (Okapi BM25)',
      desc: 'Inverted index, statistical relevance ranking, stop-word elimination & fuzzy typo matching',
    },
    cache: {
      title: 'Time-To-Live (TTL) & Cache Engine',
      desc: 'High-speed Redis-like auto-expiring in-memory keys, OTP tokens, and continuous Min-Heap sweeper',
    },
    vector: {
      title: 'AI Vector Search Engine',
      desc: 'HNSW sub-millisecond similarity matching & embedding visualizer (up to 4096 dimensions)',
    },
    graph: {
      title: 'Knowledge Graph & GraphRAG',
      desc: 'Interactive relationship explorer and BFS context traversal simulator',
    },
    security: {
      title: 'Zero-Trust Security Vault',
      desc: 'AES-256-GCM hardware encryption at rest, Argon2id auth, and JWT RBAC keys',
    },
  };

  const current = titles[currentTab];

  return (
    <header className="h-16 px-6 border-b border-slate-200 bg-white/95 dark:border-zinc-800 dark:bg-zinc-950/90 backdrop-blur-md flex items-center justify-between z-10 select-none transition-colors">
      <div>
        <div className="flex items-center gap-2.5">
          <h1 className="text-base font-semibold text-slate-900 dark:text-zinc-100 tracking-tight">
            {current.title}
          </h1>
          <Badge variant="success" className="gap-1">
            <Zap className="w-3 h-3 text-emerald-600 dark:text-emerald-400 fill-current" />
            <span>Sub-ms Latency</span>
          </Badge>
        </div>
        <p className="text-xs text-slate-500 dark:text-zinc-400">{current.desc}</p>
      </div>

      <div className="flex items-center gap-2.5">
        <Button
          variant="outline"
          size="sm"
          onClick={onRefresh}
          loading={isRefreshing}
          title="Refresh Data from FaizDB"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${isRefreshing ? 'animate-spin' : ''}`} />
          <span>Refresh</span>
        </Button>

        {currentTab === 'tables' && (
          <Button variant="primary" size="sm" onClick={onOpenInsertModal}>
            <Plus className="w-3.5 h-3.5" />
            <span>Insert Document</span>
          </Button>
        )}

        {currentTab !== 'query' && (
          <Button variant="secondary" size="sm" onClick={onQuickQuery}>
            <Terminal className="w-3.5 h-3.5" />
            <span>Open Query</span>
          </Button>
        )}

        {/* Light / Dark Mode Toggle */}
        <button
          onClick={onToggleTheme}
          title={theme === 'dark' ? 'Switch to Light Mode' : 'Switch to Dark Mode'}
          className="p-2 rounded-lg border border-slate-200 bg-slate-100 hover:bg-slate-200 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:bg-zinc-800 transition-all flex items-center justify-center cursor-pointer shadow-xs"
        >
          {theme === 'dark' ? (
            <Sun className="w-4 h-4 text-amber-400 hover:rotate-45 transition-transform" />
          ) : (
            <Moon className="w-4 h-4 text-slate-700 hover:-rotate-12 transition-transform" />
          )}
        </button>
      </div>
    </header>
  );
};
