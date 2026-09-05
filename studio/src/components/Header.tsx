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
  username?: string;
  role?: string;
  onLogout?: () => void;
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
  username,
  role,
  onLogout,
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
    backup: {
      title: 'Automated Backup & Disaster Recovery (PITR)',
      desc: 'Non-blocking point-in-time snapshots, cryptographic SHA256 verification & instant restore',
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
    robot: {
      title: 'Robot & Edge Chip Control Center',
      desc: 'Hardware RAM throttling, edge node telemetry, and robotics AI embeddings',
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

        <a
          href="/docs-site/index.html"
          target="_blank"
          rel="noreferrer"
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-slate-200 bg-slate-50 hover:bg-slate-100 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:bg-zinc-800 text-xs font-semibold text-slate-700 dark:text-zinc-300 transition-all cursor-pointer text-decoration-none"
          title="Open Documentation & Knowledge Base"
        >
          <span>📖 Docs</span>
        </a>

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

        {/* User Session Pill */}
        {username && (
          <div className="flex items-center gap-2 pl-2 border-l border-slate-200 dark:border-zinc-800">
            <div className="flex items-center gap-1.5">
              <div className="w-6 h-6 rounded-full bg-gradient-to-br from-blue-500 to-indigo-600 flex items-center justify-center text-white text-xs font-bold">
                {username.charAt(0).toUpperCase()}
              </div>
              <div className="hidden sm:block">
                <p className="text-xs font-semibold text-slate-700 dark:text-zinc-200 leading-none">{username}</p>
                <p className="text-[10px] text-slate-400 dark:text-zinc-500 mt-0.5 capitalize">{role}</p>
              </div>
            </div>
            {onLogout && (
              <button
                id="faizdb-logout-btn"
                onClick={onLogout}
                title="Sign Out"
                className="p-1.5 rounded-lg text-slate-400 hover:text-red-500 hover:bg-red-50 dark:hover:bg-red-950/30 transition-all"
              >
                <svg className="w-3.5 h-3.5" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
                  <polyline points="16 17 21 12 16 7" />
                  <line x1="21" y1="12" x2="9" y2="12" />
                </svg>
              </button>
            )}
          </div>
        )}
      </div>
    </header>
  );
};
