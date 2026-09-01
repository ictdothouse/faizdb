import React from 'react';
import { RefreshCw, Terminal, Plus, Shield, Zap } from 'lucide-react';
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
}

export const Header: React.FC<HeaderProps> = ({
  currentTab,
  selectedCollection,
  onRefresh,
  onOpenInsertModal,
  onQuickQuery,
  isRefreshing,
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
    <header className="h-16 px-6 border-b border-border bg-background/90 backdrop-blur-md flex items-center justify-between z-10 select-none">
      <div>
        <div className="flex items-center gap-2.5">
          <h1 className="text-base font-semibold text-zinc-100 tracking-tight">
            {current.title}
          </h1>
          <Badge variant="success" className="gap-1">
            <Zap className="w-3 h-3 text-emerald-400 fill-emerald-400" />
            <span>Sub-ms Latency</span>
          </Badge>
        </div>
        <p className="text-xs text-zinc-400">{current.desc}</p>
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
      </div>
    </header>
  );
};
