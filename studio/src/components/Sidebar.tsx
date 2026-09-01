import React from 'react';
import {
  LayoutDashboard,
  Table2,
  Terminal,
  Radio,
  Globe,
  BrainCircuit,
  Network,
  ShieldCheck,
  Plus,
  Layers,
  Database,
} from 'lucide-react';
import { Badge } from './ui/Badge';

export type NavTab = 'overview' | 'tables' | 'query' | 'stream' | 'cluster' | 'vector' | 'graph' | 'security';

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
    { id: 'stream', label: 'Live Streams', icon: <Radio className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />, badge: 'WS' },
    { id: 'cluster', label: 'Cluster & Shards', icon: <Globe className="w-4 h-4 text-cyan-600 dark:text-cyan-400" />, badge: 'Raft' },
    { id: 'vector', label: 'AI Vector Search', icon: <BrainCircuit className="w-4 h-4 text-purple-600 dark:text-purple-400" />, badge: 'HNSW' },
    { id: 'graph', label: 'Knowledge Graph', icon: <Network className="w-4 h-4 text-amber-600 dark:text-amber-400" />, badge: 'RAG' },
    { id: 'security', label: 'Security Vault', icon: <ShieldCheck className="w-4 h-4 text-indigo-600 dark:text-indigo-400" /> },
  ];

  return (
    <aside className="w-64 h-screen bg-white border-r border-slate-200 dark:bg-zinc-950 dark:border-zinc-800 flex flex-col select-none transition-colors">
      {/* Brand Header */}
      <div className="p-4 border-b border-slate-200 dark:border-zinc-800 flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <div className="w-8 h-8 rounded-lg bg-gradient-to-tr from-amber-600 via-orange-500 to-emerald-400 flex items-center justify-center shadow-md">
            <span className="text-base font-bold">🔥</span>
          </div>
          <div>
            <div className="flex items-center gap-1.5">
              <span className="font-bold text-sm text-slate-900 dark:text-zinc-100 tracking-tight">FaizDB</span>
              <span className="text-[10px] uppercase font-mono px-1.5 py-0.2 bg-emerald-50 text-emerald-700 border border-emerald-200 dark:bg-emerald-950/80 dark:text-emerald-400 dark:border-emerald-800/60 rounded font-semibold">
                Studio
              </span>
            </div>
            <p className="text-[11px] text-slate-500 dark:text-zinc-400">AI-Native NoSQL Engine</p>
          </div>
        </div>
      </div>

      {/* Main Navigation */}
      <div className="px-3 py-3 space-y-1">
        <p className="px-2 pb-1.5 text-[11px] font-mono font-semibold uppercase tracking-wider text-slate-400 dark:text-zinc-500">
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
                  ? 'bg-slate-100 text-slate-900 font-semibold border border-slate-200 dark:bg-zinc-900 dark:text-zinc-100 dark:border-zinc-800 shadow-xs'
                  : 'text-slate-600 hover:text-slate-900 hover:bg-slate-50 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-900/60'
              }`}
            >
              <div className="flex items-center gap-2.5">
                <span className={active ? 'text-emerald-600 dark:text-emerald-400' : 'text-slate-400 dark:text-zinc-500'}>
                  {item.icon}
                </span>
                <span>{item.label}</span>
              </div>
              {item.badge && (
                <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-200 text-slate-700 dark:bg-zinc-800 dark:text-zinc-300 border border-slate-300 dark:border-zinc-700">
                  {item.badge}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Collections Section */}
      <div className="flex-1 overflow-y-auto px-3 py-2 border-t border-slate-200 dark:border-zinc-800">
        <div className="flex items-center justify-between px-2 pb-2">
          <div className="flex items-center gap-1.5 text-[11px] font-mono font-semibold uppercase tracking-wider text-slate-400 dark:text-zinc-500">
            <Layers className="w-3.5 h-3.5" />
            <span>Collections ({collections.length})</span>
          </div>
          <button
            onClick={onOpenCreateCollection}
            title="Create Collection"
            className="p-1 rounded text-slate-400 hover:text-emerald-600 hover:bg-slate-100 dark:hover:text-emerald-400 dark:hover:bg-zinc-800 transition-colors"
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
                    ? 'bg-emerald-50 text-emerald-800 font-semibold border border-emerald-200 dark:bg-emerald-950/40 dark:text-emerald-300 dark:border-emerald-800/40'
                    : 'text-slate-600 hover:text-slate-900 hover:bg-slate-50 dark:text-zinc-400 dark:hover:text-zinc-200 dark:hover:bg-zinc-900/40'
                }`}
              >
                <div className="flex items-center gap-2 truncate">
                  <Database className="w-3.5 h-3.5 text-slate-400 dark:text-zinc-500" />
                  <span className="truncate font-mono">{col}</span>
                </div>
              </button>
            );
          })}
        </div>
      </div>

      {/* Connection Footer */}
      <div className="p-3 border-t border-slate-200 bg-slate-50/80 dark:border-zinc-800 dark:bg-zinc-900/60 text-xs">
        <div className="flex items-center justify-between mb-1.5">
          <div className="flex items-center gap-2">
            <span
              className={`w-2 h-2 rounded-full ${
                isConnected ? 'bg-emerald-500 animate-pulse' : 'bg-rose-500'
              }`}
            />
            <span className="text-slate-800 dark:text-zinc-200 font-mono text-[11px] font-medium">
              {isConnected ? 'Engine Online' : 'Connecting...'}
            </span>
          </div>
          <Badge variant={isConnected ? 'success' : 'warning'}>v0.1.0</Badge>
        </div>
        <div className="text-[10px] text-slate-500 dark:text-zinc-400 font-mono space-y-0.5">
          <p>🍃 Mongo : 127.0.0.1:27017</p>
          <p>🌐 REST  : 127.0.0.1:27018</p>
        </div>
      </div>
    </aside>
  );
};
