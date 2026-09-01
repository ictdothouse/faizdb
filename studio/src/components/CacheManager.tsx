import React, { useState, useEffect } from 'react';
import {
  Clock,
  Zap,
  Trash2,
  Plus,
  RefreshCw,
  Timer,
  Key,
  ShieldAlert,
  Flame,
  CheckCircle2,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';
import { Modal } from './ui/Modal';
import { api } from '../api/client';

interface ExpiringItem {
  id: string;
  key: string;
  value: string;
  ttl_seconds: number;
  remaining_seconds: number;
  created_at: number;
  category: 'otp' | 'session' | 'ratelimit';
}

export const CacheManager: React.FC = () => {
  const [collectionName, setCollectionName] = useState<string>('cache_store');
  const [items, setItems] = useState<ExpiringItem[]>([
    {
      id: 'otp_9921',
      key: 'otp:auth:user_9921',
      value: '849204',
      ttl_seconds: 15,
      remaining_seconds: 12,
      created_at: Date.now(),
      category: 'otp',
    },
    {
      id: 'sess_faiz',
      key: 'sess:jwt:faiz_enterprise',
      value: 'eyJhbGciOiJIUzI1NiIs...',
      ttl_seconds: 60,
      remaining_seconds: 48,
      created_at: Date.now(),
      category: 'session',
    },
    {
      id: 'rate_101',
      key: 'ratelimit:ip:192.168.1.101',
      value: 'requests_count: 42',
      ttl_seconds: 30,
      remaining_seconds: 22,
      created_at: Date.now(),
      category: 'ratelimit',
    },
  ]);

  const [totalPurged, setTotalPurged] = useState<number>(142);
  const [isCreateModalOpen, setIsCreateModalOpen] = useState<boolean>(false);
  const [newKey, setNewKey] = useState<string>('otp:sms:user_773');
  const [newVal, setNewVal] = useState<string>('582910');
  const [newTtl, setNewTtl] = useState<number>(10);
  const [isPurging, setIsPurging] = useState<boolean>(false);

  // Live countdown ticker every 1 second
  useEffect(() => {
    const timer = setInterval(() => {
      setItems((prev) => {
        const next = prev
          .map((item) => ({
            ...item,
            remaining_seconds: Math.max(0, item.remaining_seconds - 1),
          }))
          .filter((item) => {
            if (item.remaining_seconds <= 0) {
              setTotalPurged((c) => c + 1);
              return false;
            }
            return true;
          });
        return next;
      });
    }, 1000);

    return () => clearInterval(timer);
  }, []);

  const handleCreateCache = async () => {
    if (!newKey.trim()) return;

    try {
      await api.insertDocument(collectionName, {
        key: newKey,
        value: newVal,
        _ttl: newTtl,
      });

      setItems((prev) => [
        ...prev,
        {
          id: `item_${Date.now()}`,
          key: newKey,
          value: newVal,
          ttl_seconds: newTtl,
          remaining_seconds: newTtl,
          created_at: Date.now(),
          category: newKey.includes('otp') ? 'otp' : newKey.includes('sess') ? 'session' : 'ratelimit',
        },
      ]);
      setIsCreateModalOpen(false);
    } catch (e) {
      console.warn('TTL insert error:', e);
    }
  };

  const handleManualPurge = async () => {
    setIsPurging(true);
    try {
      await fetch(`${api.getEndpoint()}/v1/collections/${collectionName}/ttl/purge`, { method: 'POST' });
      setTotalPurged((prev) => prev + items.filter((i) => i.remaining_seconds <= 2).length);
      setItems((prev) => prev.filter((i) => i.remaining_seconds > 2));
    } catch (e) {
      console.warn('Purge error:', e);
    } finally {
      setTimeout(() => setIsPurging(false), 400);
    }
  };

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] overflow-y-auto p-6 space-y-6 bg-slate-50 dark:bg-zinc-950">
      {/* Top Banner Overview */}
      <div className="p-5 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm flex items-center justify-between">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-amber-50 dark:bg-amber-950/60 border border-amber-200 dark:border-amber-800 text-amber-700 dark:text-amber-400">
            <Clock className="w-5 h-5" />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold text-slate-900 dark:text-zinc-100">
                Time-To-Live (TTL) & High-Speed In-Memory Cache Engine
              </h2>
              <Badge variant="warning">Redis-Like Caching</Badge>
            </div>
            <p className="text-xs text-slate-500 dark:text-zinc-400">
              Sub-millisecond auto-expiring keys for user sessions, OTP tokens, and rate limiters with Min-Heap sweeper.
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" onClick={handleManualPurge} loading={isPurging}>
            <Trash2 className="w-3.5 h-3.5 text-rose-500" />
            <span>Purge Expired</span>
          </Button>

          <Button variant="primary" size="sm" onClick={() => setIsCreateModalOpen(true)}>
            <Plus className="w-3.5 h-3.5" />
            <span>Set TTL Key</span>
          </Button>
        </div>
      </div>

      {/* Metrics Grid */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4 text-xs font-mono">
        <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-1.5">
          <span className="text-slate-500 dark:text-zinc-400 uppercase tracking-wider font-semibold">
            Active Cached Keys
          </span>
          <div className="flex items-center gap-2">
            <Key className="w-4 h-4 text-amber-500" />
            <p className="text-lg font-bold text-slate-900 dark:text-zinc-100">{items.length} Keys</p>
          </div>
          <p className="text-[11px] text-emerald-600 dark:text-emerald-400">Lock-Free In-Memory Store</p>
        </div>

        <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-1.5">
          <span className="text-slate-500 dark:text-zinc-400 uppercase tracking-wider font-semibold">
            Total Auto-Purged
          </span>
          <div className="flex items-center gap-2">
            <Flame className="w-4 h-4 text-rose-500" />
            <p className="text-lg font-bold text-slate-900 dark:text-zinc-100">{totalPurged} Evicted</p>
          </div>
          <p className="text-[11px] text-slate-500 dark:text-zinc-400">Zero Memory Leaks</p>
        </div>

        <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-1.5">
          <span className="text-slate-500 dark:text-zinc-400 uppercase tracking-wider font-semibold">
            Sweeper Interval
          </span>
          <div className="flex items-center gap-2">
            <Timer className="w-4 h-4 text-indigo-500" />
            <p className="text-lg font-bold text-slate-900 dark:text-zinc-100">1.0 sec</p>
          </div>
          <p className="text-[11px] text-indigo-600 dark:text-indigo-400">Continuous Background Task</p>
        </div>

        <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-1.5">
          <span className="text-slate-500 dark:text-zinc-400 uppercase tracking-wider font-semibold">
            Eviction Mechanism
          </span>
          <div className="flex items-center gap-2">
            <Zap className="w-4 h-4 text-cyan-500" />
            <p className="text-lg font-bold text-slate-900 dark:text-zinc-100">Lazy + Sweeper</p>
          </div>
          <p className="text-[11px] text-slate-500 dark:text-zinc-400">Exact Redis Semantics</p>
        </div>
      </div>

      {/* Expiring Keys Live Table */}
      <div className="p-5 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-4">
        <div className="flex items-center justify-between pb-3 border-b border-slate-200 dark:border-zinc-800">
          <div className="flex items-center gap-2">
            <Clock className="w-4 h-4 text-amber-500" />
            <h3 className="text-sm font-semibold text-slate-900 dark:text-zinc-100">
              Live Expiring Cache Keys ({items.length} Active)
            </h3>
          </div>
          <span className="text-xs font-mono text-slate-500 dark:text-zinc-400">
            Keys disappear automatically when countdown reaches 0s
          </span>
        </div>

        <div className="space-y-3">
          {items.length === 0 ? (
            <div className="p-8 text-center text-slate-400 dark:text-zinc-500 font-mono text-xs">
              No active TTL keys currently tracked. Click "Set TTL Key" above to create an expiring key.
            </div>
          ) : (
            items.map((item) => {
              const progressPct = Math.max(0, Math.min(100, (item.remaining_seconds / item.ttl_seconds) * 100));
              return (
                <div
                  key={item.id}
                  className="p-4 rounded-xl border border-slate-200 dark:border-zinc-800 bg-slate-50 dark:bg-zinc-950 space-y-2.5 transition-all"
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <Key className="w-4 h-4 text-amber-500" />
                      <span className="text-xs font-bold font-mono text-slate-900 dark:text-zinc-100">
                        {item.key}
                      </span>
                    </div>

                    <div className="flex items-center gap-2">
                      <span
                        className={`text-xs font-mono font-bold px-2 py-0.5 rounded ${
                          item.remaining_seconds <= 5
                            ? 'bg-rose-100 text-rose-700 dark:bg-rose-950/60 dark:text-rose-300 animate-pulse'
                            : 'bg-amber-100 text-amber-800 dark:bg-amber-950/60 dark:text-amber-300'
                        }`}
                      >
                        ⏱️ {item.remaining_seconds}s remaining (TTL: {item.ttl_seconds}s)
                      </span>
                    </div>
                  </div>

                  <div className="text-xs font-mono text-slate-600 dark:text-zinc-400 truncate bg-white dark:bg-zinc-900 p-2 rounded-lg border border-slate-200 dark:border-zinc-800">
                    <span className="text-slate-400 dark:text-zinc-500 mr-2">Value:</span>
                    {item.value}
                  </div>

                  {/* Progress bar */}
                  <div className="w-full bg-slate-200 dark:bg-zinc-800 h-1.5 rounded-full overflow-hidden">
                    <div
                      className={`h-full transition-all duration-1000 ${
                        item.remaining_seconds <= 5 ? 'bg-rose-500' : 'bg-amber-500'
                      }`}
                      style={{ width: `${progressPct}%` }}
                    />
                  </div>
                </div>
              );
            })
          )}
        </div>
      </div>

      {/* Set TTL Key Modal */}
      <Modal
        isOpen={isCreateModalOpen}
        onClose={() => setIsCreateModalOpen(false)}
        title="Set Expiring Cache Key (TTL)"
      >
        <div className="space-y-4">
          <div>
            <label className="text-xs font-mono font-medium text-slate-700 dark:text-zinc-300">
              Cache Key
            </label>
            <input
              type="text"
              value={newKey}
              onChange={(e) => setNewKey(e.target.value)}
              className="w-full mt-1.5 bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg px-3 py-2 font-mono text-xs text-slate-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-amber-500"
            />
          </div>

          <div>
            <label className="text-xs font-mono font-medium text-slate-700 dark:text-zinc-300">
              Cache Value / Payload
            </label>
            <input
              type="text"
              value={newVal}
              onChange={(e) => setNewVal(e.target.value)}
              className="w-full mt-1.5 bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg px-3 py-2 font-mono text-xs text-slate-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-amber-500"
            />
          </div>

          <div>
            <label className="text-xs font-mono font-medium text-slate-700 dark:text-zinc-300">
              Time-To-Live Duration (Seconds)
            </label>
            <div className="flex items-center gap-2 mt-1.5">
              <input
                type="number"
                value={newTtl}
                onChange={(e) => setNewTtl(Number(e.target.value))}
                min={1}
                max={86400}
                className="w-28 bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg px-3 py-2 font-mono text-xs text-slate-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-amber-500"
              />
              <div className="flex gap-1.5">
                {[5, 10, 30, 60, 300].map((s) => (
                  <button
                    key={s}
                    type="button"
                    onClick={() => setNewTtl(s)}
                    className={`px-2.5 py-1 rounded text-xs font-mono border ${
                      newTtl === s
                        ? 'bg-amber-50 border-amber-300 text-amber-800 dark:bg-amber-950/60 dark:border-amber-800 dark:text-amber-300'
                        : 'bg-white border-slate-200 dark:bg-zinc-900 dark:border-zinc-800 text-slate-600 dark:text-zinc-400'
                    }`}
                  >
                    {s}s
                  </button>
                ))}
              </div>
            </div>
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <Button variant="outline" size="sm" onClick={() => setIsCreateModalOpen(false)}>
              Cancel
            </Button>
            <Button variant="primary" size="sm" onClick={handleCreateCache}>
              Save & Start Countdown
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};
