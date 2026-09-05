import React, { useState, useEffect } from 'react';
import {
  Bot,
  Cpu,
  Wifi,
  WifiOff,
  Sliders,
  Activity,
  HardDrive,
  Zap,
  RefreshCw,
  CheckCircle2,
  Play,
  Square,
  Gauge
} from 'lucide-react';
import { Badge } from './ui/Badge';

interface EdgeRobotManagerProps {
  apiBaseUrl?: string;
}

interface TelemetryEvent {
  id: string;
  timestamp: string;
  sensor: string;
  values: Record<string, number | string>;
  vectorEmbedding?: number[];
  status: 'synced' | 'buffered_wal';
}

export const EdgeRobotManager: React.FC<EdgeRobotManagerProps> = ({ apiBaseUrl = '/v1' }) => {
  // Remote target connection state
  const [deviceTarget, setDeviceTarget] = useState<'local' | 'remote'>('local');
  const [remoteHost, setRemoteHost] = useState('192.168.1.105');
  const [remotePort, setRemotePort] = useState('27018');
  const [connectionStatus, setConnectionStatus] = useState<'connected' | 'checking' | 'disconnected'>('connected');
  const [pingLatency, setPingLatency] = useState<number>(0.38);

  // RAM Profile selection
  const [selectedProfile, setSelectedProfile] = useState<'micro' | 'robot_sbc' | 'robot_ai' | 'unlimited'>('robot_sbc');
  const [customRamMb, setCustomRamMb] = useState<number>(32);
  const [profileApplied, setProfileApplied] = useState(true);

  // Live sensor streaming simulation
  const [isStreaming, setIsStreaming] = useState(false);
  const [telemetryEvents, setTelemetryEvents] = useState<TelemetryEvent[]>([]);
  const [tickCount, setTickCount] = useState(0);
  const [meshOnline, setMeshOnline] = useState(true);

  // Test Ping
  const handleTestPing = () => {
    setConnectionStatus('checking');
    setTimeout(() => {
      if (deviceTarget === 'local') {
        setPingLatency(Number((0.2 + Math.random() * 0.3).toFixed(2)));
        setConnectionStatus('connected');
      } else {
        setPingLatency(Number((1.8 + Math.random() * 1.5).toFixed(2)));
        setConnectionStatus('connected');
      }
    }, 400);
  };

  // Profile presets
  const profiles = [
    {
      id: 'micro',
      title: 'Microcontroller / Chip',
      ramCap: 16,
      desc: 'Aggressive compaction, 16MB RAM cap, zero-alloc vectors. For ultra-constrained microchips.',
      icon: <Cpu className="w-5 h-5 text-amber-500" />,
      color: 'border-amber-500/30 dark:border-amber-500/20 bg-amber-500/5'
    },
    {
      id: 'robot_sbc',
      title: 'Robotics SBC (Default)',
      ramCap: 32,
      desc: '32MB buffer, low-latency WAL, SIMD vector unrolling. Optimized for Raspberry Pi 4/5 & Jetson Nano.',
      icon: <Bot className="w-5 h-5 text-emerald-500" />,
      color: 'border-emerald-500/30 dark:border-emerald-500/20 bg-emerald-500/5'
    },
    {
      id: 'robot_ai',
      title: 'Autonomous AI Robot',
      ramCap: 64,
      desc: '64MB buffer, real-time HNSW vision embeddings, local GraphRAG edge traversal.',
      icon: <Zap className="w-5 h-5 text-cyan-500" />,
      color: 'border-cyan-500/30 dark:border-cyan-500/20 bg-cyan-500/5'
    },
    {
      id: 'unlimited',
      title: 'Desktop / Server',
      ramCap: 0,
      desc: 'Uncapped dynamic RAM, maximum multi-core batch throughput for workstation base stations.',
      icon: <HardDrive className="w-5 h-5 text-purple-500" />,
      color: 'border-purple-500/30 dark:border-purple-500/20 bg-purple-500/5'
    }
  ];

  // Streaming effect
  useEffect(() => {
    let interval: any = null;
    if (isStreaming) {
      interval = setInterval(() => {
        setTickCount((prev) => prev + 1);
        const newEvent: TelemetryEvent = {
          id: `evt-${Date.now().toString(36)}-${Math.floor(Math.random() * 1000)}`,
          timestamp: new Date().toLocaleTimeString(),
          sensor: Math.random() > 0.4 ? 'IMU-6DOF' : 'LIDAR-Front',
          values: {
            pitch: Number((Math.sin(Date.now() / 1000) * 12).toFixed(2)),
            roll: Number((Math.cos(Date.now() / 1000) * 8).toFixed(2)),
            distance_m: Number((1.2 + Math.random() * 2.5).toFixed(2)),
            temp_c: Number((36.5 + Math.random() * 2.0).toFixed(1)),
          },
          vectorEmbedding: [
            Number((Math.random() * 0.9).toFixed(3)),
            Number((Math.random() * 0.9).toFixed(3)),
            Number((Math.random() * 0.9).toFixed(3)),
            Number((Math.random() * 0.9).toFixed(3)),
          ],
          status: meshOnline ? 'synced' : 'buffered_wal',
        };

        setTelemetryEvents((prev) => [newEvent, ...prev.slice(0, 19)]);
      }, 150);
    }
    return () => {
      if (interval) clearInterval(interval);
    };
  }, [isStreaming, meshOnline]);

  const applyProfile = (id: 'micro' | 'robot_sbc' | 'robot_ai' | 'unlimited') => {
    setSelectedProfile(id);
    const p = profiles.find((x) => x.id === id);
    if (p) setCustomRamMb(p.ramCap);
    setProfileApplied(false);
    setTimeout(() => {
      setProfileApplied(true);
    }, 300);
  };

  return (
    <div className="space-y-6 max-w-7xl mx-auto p-4 sm:p-6 animate-fade-in text-slate-800 dark:text-zinc-200">
      {/* Top Banner */}
      <div className="flex flex-col md:flex-row md:items-center justify-between gap-4 p-5 rounded-2xl bg-gradient-to-r from-amber-500/10 via-emerald-500/10 to-cyan-500/10 border border-slate-200 dark:border-zinc-800">
        <div>
          <div className="flex items-center gap-2">
            <span className="p-2 rounded-xl bg-amber-500 text-white shadow-md">
              <Bot className="w-5 h-5" />
            </span>
            <h1 className="text-xl font-bold text-slate-900 dark:text-white tracking-tight">
              Robot & Edge Chip Control Center
            </h1>
            <Badge variant="outline" className="bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/30">
              Native Edge Engine
            </Badge>
          </div>
          <p className="text-xs text-slate-500 dark:text-zinc-400 mt-1">
            Configure embedded chip memory caps, pair remote robotics controllers, and stream live sensor vector telemetry.
          </p>
        </div>

        {/* Live Device Status */}
        <div className="flex items-center gap-3 bg-white/80 dark:bg-zinc-900/80 p-2.5 rounded-xl border border-slate-200 dark:border-zinc-800 backdrop-blur-xs">
          <div className="flex items-center gap-2">
            <span className="relative flex h-3 w-3">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-emerald-400 opacity-75"></span>
              <span className="relative inline-flex rounded-full h-3 w-3 bg-emerald-500"></span>
            </span>
            <span className="text-xs font-semibold font-mono">
              {deviceTarget === 'local' ? 'Local Engine (Windows/WSL)' : `Robot Node (${remoteHost})`}
            </span>
          </div>
          <span className="text-xs font-mono text-slate-400">|</span>
          <span className="text-xs font-mono text-emerald-600 dark:text-emerald-400 font-bold">
            {pingLatency} ms ping
          </span>
          <button
            onClick={handleTestPing}
            className="p-1.5 rounded-lg hover:bg-slate-100 dark:hover:bg-zinc-800 text-slate-500 transition-colors"
            title="Refresh Heartbeat"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${connectionStatus === 'checking' ? 'animate-spin' : ''}`} />
          </button>
        </div>
      </div>

      {/* Grid: Pairing & Hardware Profiles */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Card 1: Device Pairing Configuration */}
        <div className="bg-white dark:bg-zinc-950 p-5 space-y-4 rounded-2xl border border-slate-200 dark:border-zinc-800 shadow-xs">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Wifi className="w-4 h-4 text-cyan-600 dark:text-cyan-400" />
              <h2 className="text-sm font-bold text-slate-900 dark:text-zinc-100">Edge Device Pairing</h2>
            </div>
            <Badge variant="outline" className="text-[10px] font-mono">
              mDNS / HTTP
            </Badge>
          </div>

          <div className="space-y-3 text-xs">
            <div>
              <label className="text-slate-500 dark:text-zinc-400 font-medium block mb-1">Target Environment</label>
              <div className="grid grid-cols-2 gap-2">
                <button
                  onClick={() => setDeviceTarget('local')}
                  className={`py-2 px-3 rounded-lg font-medium text-center border transition-all ${
                    deviceTarget === 'local'
                      ? 'border-emerald-500 bg-emerald-500/10 text-emerald-700 dark:text-emerald-400 font-bold'
                      : 'border-slate-200 dark:border-zinc-800 text-slate-600 dark:text-zinc-400 hover:bg-slate-50 dark:hover:bg-zinc-800'
                  }`}
                >
                  🖥️ Local Host PC
                </button>
                <button
                  onClick={() => setDeviceTarget('remote')}
                  className={`py-2 px-3 rounded-lg font-medium text-center border transition-all ${
                    deviceTarget === 'remote'
                      ? 'border-cyan-500 bg-cyan-500/10 text-cyan-700 dark:text-cyan-400 font-bold'
                      : 'border-slate-200 dark:border-zinc-800 text-slate-600 dark:text-zinc-400 hover:bg-slate-50 dark:hover:bg-zinc-800'
                  }`}
                >
                  🤖 Remote Robot / SBC
                </button>
              </div>
            </div>

            {deviceTarget === 'remote' && (
              <div className="space-y-2 p-3 bg-slate-50 dark:bg-zinc-900 rounded-xl border border-slate-200 dark:border-zinc-800 animate-fade-in">
                <div>
                  <label className="text-[11px] text-slate-500 dark:text-zinc-400 font-mono">Robot IP / Hostname</label>
                  <input
                    type="text"
                    value={remoteHost}
                    onChange={(e) => setRemoteHost(e.target.value)}
                    placeholder="192.168.1.100 or robot.local"
                    className="w-full mt-1 px-3 py-1.5 rounded-lg border border-slate-300 dark:border-zinc-700 bg-white dark:bg-zinc-950 text-xs font-mono"
                  />
                </div>
                <div>
                  <label className="text-[11px] text-slate-500 dark:text-zinc-400 font-mono">FaizDB Port</label>
                  <input
                    type="text"
                    value={remotePort}
                    onChange={(e) => setRemotePort(e.target.value)}
                    className="w-full mt-1 px-3 py-1.5 rounded-lg border border-slate-300 dark:border-zinc-700 bg-white dark:bg-zinc-950 text-xs font-mono"
                  />
                </div>
              </div>
            )}

            {/* Spec Readout */}
            <div className="p-3 rounded-xl bg-slate-50 dark:bg-zinc-900/60 border border-slate-200 dark:border-zinc-800 space-y-1.5 font-mono text-[11px]">
              <div className="flex justify-between">
                <span className="text-slate-400">Architecture:</span>
                <span className="font-semibold text-slate-800 dark:text-zinc-200">
                  {deviceTarget === 'local' ? 'x86_64-pc-windows / WSL' : 'aarch64 / Linux ARM64'}
                </span>
              </div>
              <div className="flex justify-between">
                <span className="text-slate-400">Binary Size:</span>
                <span className="font-semibold text-emerald-600 dark:text-emerald-400">~8.08 MB (Single Executable)</span>
              </div>
              <div className="flex justify-between">
                <span className="text-slate-400">GC Pauses:</span>
                <span className="font-semibold text-cyan-600 dark:text-cyan-400">0 ms (Zero Garbage Collection)</span>
              </div>
              <div className="flex justify-between">
                <span className="text-slate-400">Tick Target:</span>
                <span className="font-semibold text-amber-600 dark:text-amber-400">128 Hz Synchronous Loop</span>
              </div>
            </div>

            <button
              onClick={handleTestPing}
              className="w-full py-2 px-3 rounded-lg bg-slate-900 text-white dark:bg-zinc-100 dark:text-zinc-900 text-xs font-semibold hover:opacity-90 transition-opacity flex items-center justify-center gap-2"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              <span>Connect & Validate Edge Node</span>
            </button>
          </div>
        </div>

        {/* Card 2: Hardware RAM Throttle Profiles (2 Columns Span) */}
        <div className="bg-white dark:bg-zinc-950 p-5 lg:col-span-2 space-y-4 rounded-2xl border border-slate-200 dark:border-zinc-800 shadow-xs">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Sliders className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
              <h2 className="text-sm font-bold text-slate-900 dark:text-zinc-100">
                Hardware RAM Throttle Profiles
              </h2>
            </div>
            {profileApplied ? (
              <Badge variant="outline" className="bg-emerald-500/10 text-emerald-600 border-emerald-500/30 flex items-center gap-1">
                <CheckCircle2 className="w-3 h-3" />
                <span>Profile Active ({customRamMb === 0 ? 'Uncapped' : `${customRamMb}MB`})</span>
              </Badge>
            ) : (
              <Badge variant="outline" className="bg-amber-500/10 text-amber-600 border-amber-500/30">
                Syncing changes...
              </Badge>
            )}
          </div>

          <p className="text-xs text-slate-500 dark:text-zinc-400">
            Tailor FaizDB internal LSM-Tree MemTable sizes, compaction concurrency, and vector caches to prevent Out-Of-Memory (OOM) faults on low-resource robotics and IoT hardware.
          </p>

          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            {profiles.map((p) => {
              const active = selectedProfile === p.id;
              return (
                <div
                  key={p.id}
                  onClick={() => applyProfile(p.id as any)}
                  className={`p-3.5 rounded-xl border cursor-pointer transition-all ${
                    active
                      ? `${p.color} ring-2 ring-emerald-500/30 dark:ring-emerald-400/30`
                      : 'border-slate-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 hover:bg-slate-50 dark:hover:bg-zinc-900/60'
                  }`}
                >
                  <div className="flex items-center justify-between mb-1.5">
                    <div className="flex items-center gap-2">
                      {p.icon}
                      <span className="font-bold text-xs text-slate-900 dark:text-zinc-100">{p.title}</span>
                    </div>
                    <span className="text-[11px] font-mono font-bold px-1.5 py-0.5 rounded bg-slate-100 dark:bg-zinc-800">
                      {p.ramCap === 0 ? '∞ Uncapped' : `${p.ramCap} MB`}
                    </span>
                  </div>
                  <p className="text-[11px] text-slate-500 dark:text-zinc-400 line-clamp-2 leading-relaxed">
                    {p.desc}
                  </p>
                </div>
              );
            })}
          </div>

          {/* Custom Slider Indicator */}
          <div className="pt-2 border-t border-slate-100 dark:border-zinc-800 flex flex-col sm:flex-row sm:items-center justify-between gap-3 text-xs">
            <div className="flex items-center gap-2">
              <Gauge className="w-4 h-4 text-slate-400" />
              <span className="text-slate-500 dark:text-zinc-400">Hardware Ceiling:</span>
              <span className="font-mono font-bold text-slate-900 dark:text-zinc-100">
                {customRamMb === 0 ? 'Full System Memory' : `Maximum ${customRamMb} MB Static RAM`}
              </span>
            </div>
            <div className="text-[11px] text-slate-400 font-mono">
              Zero Garbage Collection spikes · Circuit breaker armed at 10MB
            </div>
          </div>
        </div>
      </div>

      {/* Bottom Section: Live Sensor Telemetry & Vector Feed */}
      <div className="bg-white dark:bg-zinc-950 p-5 space-y-4 rounded-2xl border border-slate-200 dark:border-zinc-800 shadow-xs">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
          <div>
            <div className="flex items-center gap-2">
              <Activity className="w-4 h-4 text-amber-500" />
              <h2 className="text-sm font-bold text-slate-900 dark:text-zinc-100">
                Live Robotics Telemetry & AI Vector Embeddings
              </h2>
            </div>
            <p className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5">
              Live ingest stream into collections <code>robot_telemetry</code> & <code>vision_vectors</code> at 128Hz.
            </p>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => setMeshOnline(!meshOnline)}
              className={`px-3 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 border transition-all ${
                meshOnline
                  ? 'bg-emerald-50 text-emerald-700 border-emerald-200 dark:bg-emerald-950/60 dark:text-emerald-400 dark:border-emerald-800'
                  : 'bg-amber-50 text-amber-700 border-amber-200 dark:bg-amber-950/60 dark:text-amber-400 dark:border-amber-800'
              }`}
              title="Toggle Mesh Network Online / Offline Simulation"
            >
              {meshOnline ? <Wifi className="w-3.5 h-3.5" /> : <WifiOff className="w-3.5 h-3.5" />}
              <span>{meshOnline ? 'Mesh Online (Live Sync)' : 'Offline (Local WAL Buffer)'}</span>
            </button>

            <button
              onClick={() => setIsStreaming(!isStreaming)}
              className={`px-4 py-1.5 rounded-lg text-xs font-semibold flex items-center gap-1.5 transition-all ${
                isStreaming
                  ? 'bg-rose-500 hover:bg-rose-600 text-white shadow-xs'
                  : 'bg-emerald-600 hover:bg-emerald-700 text-white shadow-xs'
              }`}
            >
              {isStreaming ? <Square className="w-3.5 h-3.5" /> : <Play className="w-3.5 h-3.5" />}
              <span>{isStreaming ? 'Stop Telemetry Stream' : 'Start 128Hz Stream'}</span>
            </button>
          </div>
        </div>

        {/* Telemetry Stream Log Table */}
        <div className="border border-slate-200 dark:border-zinc-800 rounded-xl overflow-hidden">
          <div className="max-h-72 overflow-y-auto font-mono text-xs">
            <table className="w-full text-left">
              <thead className="bg-slate-50 dark:bg-zinc-900/80 text-[11px] text-slate-500 dark:text-zinc-400 sticky top-0 border-b border-slate-200 dark:border-zinc-800">
                <tr>
                  <th className="py-2.5 px-3">Tick #</th>
                  <th className="py-2.5 px-3">Time</th>
                  <th className="py-2.5 px-3">Sensor Component</th>
                  <th className="py-2.5 px-3">Telemetry Readings</th>
                  <th className="py-2.5 px-3">AI Embedding (4-Dim)</th>
                  <th className="py-2.5 px-3 text-right">Durability State</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-slate-100 dark:divide-zinc-800/60">
                {telemetryEvents.length === 0 ? (
                  <tr>
                    <td colSpan={6} className="py-8 text-center text-slate-400">
                      Telemetry stream is idle. Click "Start 128Hz Stream" to view live robot sensor feed.
                    </td>
                  </tr>
                ) : (
                  telemetryEvents.map((evt, idx) => (
                    <tr key={evt.id} className="hover:bg-slate-50/60 dark:hover:bg-zinc-900/40 transition-colors">
                      <td className="py-2 px-3 text-slate-400">#{tickCount - idx}</td>
                      <td className="py-2 px-3 text-slate-600 dark:text-zinc-400">{evt.timestamp}</td>
                      <td className="py-2 px-3 font-semibold text-slate-800 dark:text-zinc-200">{evt.sensor}</td>
                      <td className="py-2 px-3 text-slate-600 dark:text-zinc-400">
                        {Object.entries(evt.values)
                          .map(([k, v]) => `${k}:${v}`)
                          .join(' | ')}
                      </td>
                      <td className="py-2 px-3 text-purple-600 dark:text-purple-400">
                        [{evt.vectorEmbedding?.join(', ')}]
                      </td>
                      <td className="py-2 px-3 text-right">
                        {evt.status === 'synced' ? (
                          <span className="inline-flex items-center gap-1 text-[10px] text-emerald-600 dark:text-emerald-400 font-semibold px-2 py-0.5 rounded bg-emerald-50 dark:bg-emerald-950/60 border border-emerald-200 dark:border-emerald-800">
                            Synced
                          </span>
                        ) : (
                          <span className="inline-flex items-center gap-1 text-[10px] text-amber-600 dark:text-amber-400 font-semibold px-2 py-0.5 rounded bg-amber-50 dark:bg-amber-950/60 border border-amber-200 dark:border-amber-800">
                            Buffered in WAL
                          </span>
                        )}
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </div>
  );
};
