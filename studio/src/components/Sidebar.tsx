import React from 'react';
import {
  LayoutDashboard,
  Table2,
  Terminal,
  BrainCircuit,
  Network,
  ShieldCheck,
  Plus,
  Layers,
  Database,
  Radio,
} from 'lucide-react';
import { Badge } from './ui/Badge';

export type NavTab = 'overview' | 'tables' | 'query' | 'vector' | 'graph' | 'security';

interface SidebarProps {
  currentTab: NavTab;
  onSelectTab: (tab: NavTab) => void;
  collections: string[];
  selectedCollection: string;
  onSelectCollection: (col: string) => void;
  onOpenCreateCollection: () => void;
  isConnected: boolean;
}

export const Sidebar: React.FC<SidebarProps> = ({
  currentTab,
  onSelectTab,
  collections,
  selectedCollection,
  onSelectCollection,
  onOpenCreateCollection,
  isConnected,
}) => {
  const navItems: { id: NavTab; label: string; icon: React.ReactNode; badge?: string }[] = [
    { id: 'overview', label: 'Overview', icon: <LayoutDashboard className="w-4 h-4" /> },
    { id: 'tables', label: 'Table Explorer', icon: <Table2 className="w-4 h-4" /> },
    { id: 'query', label: 'FaizQL Console', icon: <Terminal className="w-4 h-4" />, badge: 'Multi' },
    { id: 'vector', label: 'AI Vector Search', icon: <BrainCircuit className="w-4 h-4 text-emerald-400" />, badge: 'HNSW' },
    { id: 'graph', label: 'Knowledge Graph', icon: <Network className="w-4 h-4 text-amber-400" />, badge: 'RAG' },
    { id: 'security', label: 'Security Vault', icon: <ShieldCheck className="w-4 h-4 text-cyan-400" /> },
  ];

  return (
    <aside className="w-64 h-screen bg-sidebar border-r border-sidebar-border flex flex-col select-none">
      {/* Brand Header */}
      <div className="p-4 border-b border-sidebar-border flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-tr from-amber-600 via-orange-500 to-emerald-400 flex items-center justify-center shadow-glow">
            <span className="text-base font-bold">🔥</span>
          </div>
          <div>
            <div className="flex items-center gap-1.5">
              <span className="font-bold text-sm text-zinc-100 tracking-tight">FaizDB</span>
              <span className="text-[10px] uppercase font-mono px-1.5 py-0.2 bg-emerald-950/80 text-emerald-400 border border-emerald-800/60 rounded">
                Studio
              </span>
            </div>
            <p className="text-[11px] text-zinc-400">AI-Native NoSQL Engine</p>
          </div>
        </div>
      </div>

      {/* Main Navigation */}
      <div className="px-3 py-3 space-y-1">
        <p className="px-2 pb-1.5 text-[11px] font-mono font-semibold uppercase tracking-wider text-zinc-400">
          Core Engine
        </p>
        {navItems.map((item) => {
          const active = currentTab === item.id;
          return (
            <button
              key={item.id}
              onClick={() => onSelectTab(item.id)}
              className={`w-full flex items-center justify-between px-2.5 py-2 rounded-lg text-xs font-medium transition-colors ${
                active
                  ? 'bg-zinc-800/90 text-zinc-100 font-semibold border border-zinc-700/60 shadow-sm'
                  : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/40'
              }`}
            >
              <div className="flex items-center gap-2.5">
                <span className={active ? 'text-emerald-400' : 'text-zinc-400'}>{item.icon}</span>
                <span>{item.label}</span>
              </div>
              {item.badge && (
                <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-zinc-800 text-zinc-400 border border-zinc-700/60">
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Collections Section */}
      <div className="flex-1 overflow-y-auto px-3 py-2 border-t border-sidebar-border/60">
        <div className="flex items-center justify-between px-2 pb-2">
          <div className="flex items-center gap-1.5 text-[11px] font-mono font-semibold uppercase tracking-wider text-zinc-400">
            <Layers className="w-3.5 h-3.5" />
            <span>Collections ({collections.length})</span>
          </div>
          <button
            onClick={onOpenCreateCollection}
            title="Create Collection"
            className="p-1 rounded text-zinc-400 hover:text-emerald-400 hover:bg-zinc-800 transition-colors"
          >
            <Plus className="w-3.5 h-3.5" />
          </button>
        </div>

        <div className="space-y-0.5">
          {collections.map((col) => {
            const active = currentTab === 'tables' && selectedCollection === col;
            return (
              <button
                key={col}
                onClick={() => {
                  onSelectCollection(col);
                  onSelectTab('tables');
                }}
                className={`w-full flex items-center justify-between px-2.5 py-1.5 rounded-md text-xs transition-colors ${
                  active
                    ? 'bg-emerald-950/40 text-emerald-300 font-medium border border-emerald-800/40'
                    : 'text-zinc-400 hover:text-zinc-200 hover:bg-zinc-800/30'
                }`}
              >
                <div className="flex items-center gap-2 truncate">
                  <Database className="w-3.5 h-3.5 text-zinc-400" />
                  <span className="truncate font-mono">{col}</span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* Connection Footer */}
      <div className="p-3 border-t border-sidebar-border bg-sidebar/50 text-xs">
        <div className="flex items-center justify-between mb-1.5">
          <div className="flex items-center gap-2">
            <span
              className={`w-2 h-2 rounded-full ${
                isConnected ? 'bg-emerald-500 animate-pulse' : 'bg-rose-500'
              }`}
            />
            <span className="text-zinc-300 font-mono text-[11px]">
              {isConnected ? 'Engine Online' : 'Connecting...'}
            </span>
          </div>
          <Badge variant={isConnected ? 'success' : 'warning'}>v0.1.0</Badge>
        </div>
        <div className="text-[10px] text-zinc-400 font-mono space-y-0.5">
          <p>🍃 Mongo : 127.0.0.1:27017</p>
          <p>🌐 REST  : 127.0.0.1:27018</p>
        </div>
      </div>
    </aside>
  );
};
