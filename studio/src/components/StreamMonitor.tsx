import React, { useState, useEffect, useRef } from 'react';
import {
  Radio,
  Zap,
  Play,
  Pause,
  Trash2,
  Copy,
  Check,
  Filter,
  Layers,
  Sparkles,
  Clock,
  Send,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';
import { api } from '../api/client';

export interface StreamEvent {
  resume_token: string;
  timestamp: string;
  collection: string;
  operation_type: 'insert' | 'update' | 'delete' | 'replace' | 'drop';
  document_id: string;
  full_document?: Record<string, any>;
  updated_fields?: Record<string, any>;
}

export const StreamMonitor: React.FC = () => {
  const [events, setEvents] = useState<StreamEvent[]>([]);
  const [isConnected, setIsConnected] = useState<boolean>(false);
  const [isPaused, setIsPaused] = useState<boolean>(false);
  const [selectedCollection, setSelectedCollection] = useState<string>('*');
  const [copiedToken, setCopiedToken] = useState<string | null>(null);
  const [selectedEvent, setSelectedEvent] = useState<StreamEvent | null>(null);
  const [isSimulating, setIsSimulating] = useState<boolean>(false);

  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    connectWebSocket();
    return () => {
      if (wsRef.current) {
        wsRef.current.close();
      }
    };
  }, []);

  const connectWebSocket = () => {
    try {
      const host = window.location.hostname || '127.0.0.1';
      const wsUrl = `ws://${host}:27018/v1/subscribe`;
      const ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        setIsConnected(true);
      };

      ws.onmessage = (msg) => {
        if (isPaused) return;
        try {
          const data = JSON.parse(msg.data);
          if (data.status === 'connected') {
            // Welcome frame
            return;
          }
          setEvents((prev) => [data, ...prev.slice(0, 99)]); // Keep last 100 events
        } catch (e) {
          console.warn('Failed to parse WebSocket message:', e);
        }
      };

      ws.onclose = () => {
        setIsConnected(false);
        // Auto-reconnect after 3 seconds
        setTimeout(connectWebSocket, 3000);
      };

      ws.onerror = () => {
        setIsConnected(false);
      };

      wsRef.current = ws;
    } catch (e) {
      console.warn('WebSocket connection error:', e);
    }
  };

  const copyToken = (token: string) => {
    navigator.clipboard.writeText(token);
    setCopiedToken(token);
    setTimeout(() => setCopiedToken(null), 2000);
  };

  const triggerMockMutation = async () => {
    setIsSimulating(true);
    try {
      const randomId = Math.floor(Math.random() * 1000);
      await api.query(
        `INSERT INTO live_telemetry { "event_id": ${randomId}, "source": "Web Studio", "status": "Active", "timestamp": "${new Date().toISOString()}" }`
      );
    } catch (e) {
      console.warn('Simulation query error:', e);
    } finally {
      setIsSimulating(false);
    }
  };

  const filteredEvents = events.filter((ev) => {
    if (selectedCollection === '*') return true;
    return ev.collection === selectedCollection;
  });

  const getOpBadge = (op: string) => {
    switch (op) {
      case 'insert':
        return <Badge variant="success">INSERT</Badge>;
      case 'update':
        return <Badge variant="warning">UPDATE</Badge>;
      case 'delete':
        return <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-mono font-medium border bg-rose-500/10 text-rose-700 dark:text-rose-400 border-rose-500/30">DELETE</span>;
      case 'drop':
        return <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-mono font-medium border bg-purple-500/10 text-purple-700 dark:text-purple-400 border-purple-500/30">DROP</span>;
      default:
        return <Badge variant="default">{op.toUpperCase()}</Badge>;
    }
  };

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] overflow-hidden bg-slate-50 dark:bg-zinc-950">
      {/* Top Toolbar Banner */}
      <div className="p-4 border-b border-slate-200 dark:border-zinc-800 bg-white dark:bg-zinc-900/80 flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-emerald-50 dark:bg-emerald-950/60 border border-emerald-200 dark:border-emerald-800 text-emerald-700 dark:text-emerald-400 flex items-center justify-center">
            <Radio className={`w-4 h-4 ${isConnected ? 'animate-pulse' : ''}`} />
          </div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-semibold text-slate-900 dark:text-zinc-100">
                Real-Time Change Stream Monitor (WebSocket CDC)
              </h2>
              <Badge variant={isConnected ? 'success' : 'warning'}>
                {isConnected ? 'LIVE WS CONNECTED' : 'RECONNECTING...'}
              </Badge>
            </div>
            <p className="text-xs text-slate-500 dark:text-zinc-400">
              Sub-millisecond Change Data Capture (CDC) stream with resume token support.
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {/* Collection Filter */}
          <div className="flex items-center gap-1.5 bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 rounded-lg px-2.5 py-1 text-xs">
            <Filter className="w-3.5 h-3.5 text-slate-500 dark:text-zinc-400" />
            <select
              value={selectedCollection}
              onChange={(e) => setSelectedCollection(e.target.value)}
              className="bg-transparent text-slate-900 dark:text-zinc-100 font-mono text-xs focus:outline-none"
            >
              <option value="*">All Collections (*)</option>
              <option value="users">users</option>
              <option value="products">products</option>
              <option value="realtime_orders">realtime_orders</option>
              <option value="live_telemetry">live_telemetry</option>
            </select>
          </div>

          <Button
            variant="outline"
            size="sm"
            onClick={() => setIsPaused(!isPaused)}
            title={isPaused ? 'Resume Stream' : 'Pause Stream'}
          >
            {isPaused ? <Play className="w-3.5 h-3.5" /> : <Pause className="w-3.5 h-3.5" />}
            <span>{isPaused ? 'Resume' : 'Pause'}</span>
          </Button>

          <Button
            variant="primary"
            size="sm"
            onClick={triggerMockMutation}
            loading={isSimulating}
          >
            <Send className="w-3.5 h-3.5" />
            <span>Emit Test Mutation</span>
          </Button>

          <Button
            variant="outline"
            size="sm"
            onClick={() => setEvents([])}
            title="Clear Stream History"
          >
            <Trash2 className="w-3.5 h-3.5" />
          </Button>
        </div>
      </div>

      {/* Main Stream Activity Ticker */}
      <div className="flex-1 overflow-auto p-4 space-y-3">
        {filteredEvents.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-80 gap-3 text-center p-6">
            <div className="w-12 h-12 rounded-xl bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 flex items-center justify-center text-emerald-600 dark:text-emerald-400">
              <Sparkles className="w-6 h-6 animate-spin" />
            </div>
            <div>
              <p className="text-sm font-semibold text-slate-900 dark:text-zinc-100">
                Listening for Real-Time Mutations...
              </p>
              <p className="text-xs text-slate-500 dark:text-zinc-400 max-w-md mt-1">
                Perform any insert, update, or delete via MongoDB Driver (Port 27017) or REST API (Port 27018) — events will appear here in sub-millisecond real time.
              </p>
            </div>
            <Button
              variant="primary"
              size="sm"
              onClick={triggerMockMutation}
              loading={isSimulating}
              className="mt-2"
            >
              <Zap className="w-3.5 h-3.5" />
              <span>Trigger Test Event Now</span>
            </Button>
          </div>
        ) : (
          filteredEvents.map((ev, idx) => (
            <div
              key={ev.resume_token || idx}
              onClick={() => setSelectedEvent(ev)}
              className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-xs hover:border-emerald-400 dark:hover:border-emerald-600/50 cursor-pointer transition-all space-y-2.5 animate-in fade-in slide-in-from-top-2 duration-150"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2.5">
                  {getOpBadge(ev.operation_type)}
                  <span className="text-xs font-mono font-bold text-slate-900 dark:text-zinc-100">
                    {ev.collection}
                  </span>
                  <span className="text-xs font-mono text-slate-400 dark:text-zinc-500">
                    • ID: {ev.document_id || 'N/A'}
                  </span>
                </div>

                <div className="flex items-center gap-3 text-xs font-mono text-slate-500 dark:text-zinc-400">
                  <span className="flex items-center gap-1">
                    <Clock className="w-3 h-3 text-slate-400" />
                    {new Date(ev.timestamp).toLocaleTimeString()}
                  </span>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      copyToken(ev.resume_token);
                    }}
                    className="flex items-center gap-1 px-1.5 py-0.5 rounded bg-slate-100 dark:bg-zinc-800 hover:text-slate-900 dark:hover:text-zinc-100"
                    title="Copy Resume Token"
                  >
                    {copiedToken === ev.resume_token ? (
                      <Check className="w-3 h-3 text-emerald-500" />
                    ) : (
                      <Copy className="w-3 h-3" />
                    )}
                    <span className="text-[10px] truncate max-w-[80px]">{ev.resume_token.slice(0, 8)}...</span>
                  </button>
                </div>
              </div>

              {/* Event Payload Preview */}
              <div className="bg-slate-50 dark:bg-zinc-950 p-2.5 rounded-lg border border-slate-200 dark:border-zinc-800 font-mono text-xs text-slate-800 dark:text-zinc-200 overflow-x-auto">
                <pre className="truncate leading-relaxed">
                  {JSON.stringify(ev.full_document || ev.updated_fields || { status: 'deleted' })}
                </pre>
              </div>
            </div>
          ))
        )}
      </div>

      {/* Footer Info */}
      <div className="p-3 border-t border-slate-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 flex items-center justify-between text-xs text-slate-500 dark:text-zinc-400 font-mono">
        <span>Active Events in Buffer: {filteredEvents.length}</span>
        <span>WebSocket Endpoint: ws://127.0.0.1:27018/v1/subscribe</span>
      </div>

      {/* Event JSON Modal Inspector */}
      {selectedEvent && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm">
          <div className="w-full max-w-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-xl p-5 shadow-2xl space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-slate-200 dark:border-zinc-800">
              <div className="flex items-center gap-2">
                {getOpBadge(selectedEvent.operation_type)}
                <h3 className="text-sm font-semibold text-slate-900 dark:text-zinc-100 font-mono">
                  Change Stream Event Detail
                </h3>
              </div>
              <Button variant="ghost" size="sm" onClick={() => setSelectedEvent(null)}>
                Close
              </Button>
            </div>

            <div className="space-y-2 text-xs font-mono">
              <div className="flex justify-between text-slate-600 dark:text-zinc-400">
                <span>Resume Token:</span>
                <span className="text-emerald-700 dark:text-emerald-400 font-bold">{selectedEvent.resume_token}</span>
              </div>
              <div className="flex justify-between text-slate-600 dark:text-zinc-400">
                <span>Collection:</span>
                <span className="text-slate-900 dark:text-zinc-100">{selectedEvent.collection}</span>
              </div>
              <div className="flex justify-between text-slate-600 dark:text-zinc-400">
                <span>Document ID:</span>
                <span className="text-slate-900 dark:text-zinc-100">{selectedEvent.document_id}</span>
              </div>
            </div>

            <div className="max-h-80 overflow-y-auto bg-slate-50 dark:bg-zinc-950 p-3.5 rounded-lg border border-slate-200 dark:border-zinc-800 font-mono text-xs text-slate-900 dark:text-emerald-300">
              <pre>{JSON.stringify(selectedEvent, null, 2)}</pre>
            </div>

            <div className="flex justify-end gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  navigator.clipboard.writeText(JSON.stringify(selectedEvent, null, 2));
                  setSelectedEvent(null);
                }}
              >
                Copy Full Event JSON
              </Button>
              <Button variant="primary" size="sm" onClick={() => setSelectedEvent(null)}>
                Done
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
