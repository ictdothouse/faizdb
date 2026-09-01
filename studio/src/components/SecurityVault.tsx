import React, { useState, useEffect } from 'react';
import {
  ShieldCheck,
  Key,
  Lock,
  UserCheck,
  Copy,
  Check,
  Zap,
  Eye,
  EyeOff,
  RefreshCw,
  AlertTriangle,
  FileText,
  Activity,
  Terminal,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';
import { api } from '../api/client';

interface AuditLogEntry {
  timestamp: string;
  event: string;
  ip: string;
  path: string;
  status: number;
  request_id: string;
  engine: string;
}

export const SecurityVault: React.FC = () => {
  const [role, setRole] = useState<'admin' | 'read_write' | 'read_only'>('admin');
  const [username, setUsername] = useState('developer_faiz');
  const [validDays, setValidDays] = useState(30);
  const [generatedToken, setGeneratedToken] = useState<string>('');
  const [copied, setCopied] = useState(false);
  const [showKey, setShowKey] = useState(false);
  const [isGenerating, setIsGenerating] = useState(false);
  const [genError, setGenError] = useState<string | null>(null);

  // Audit Logs state
  const [auditLogs, setAuditLogs] = useState<AuditLogEntry[]>([]);
  const [loadingLogs, setLoadingLogs] = useState(false);
  const [filterEvent, setFilterEvent] = useState<string>('all');
  const [autoRefresh, setAutoRefresh] = useState(true);

  const fetchAuditLogs = async () => {
    setLoadingLogs(true);
    try {
      const logs = await api.getAuditLogs(100);
      setAuditLogs(logs);
    } catch (err) {
      console.warn('Could not fetch audit logs (requires Admin role):', err);
    } finally {
      setLoadingLogs(false);
    }
  };

  useEffect(() => {
    fetchAuditLogs();
  }, []);

  // Polling auto-refresh for live audit trail
  useEffect(() => {
    if (!autoRefresh) return;
    const interval = setInterval(() => {
      fetchAuditLogs();
    }, 5000);
    return () => clearInterval(interval);
  }, [autoRefresh]);

  const handleGenerateToken = async () => {
    setIsGenerating(true);
    setGenError(null);
    try {
      const roleMap: Record<string, string> = {
        admin: 'Admin',
        read_write: 'ReadWrite',
        read_only: 'ReadOnly',
      };
      const res = await api.generateToken(
        username.trim() || 'developer',
        roleMap[role] || 'Admin',
        validDays * 86400
      );
      setGeneratedToken(res.token);
    } catch (err: any) {
      setGenError(err.message || 'Token generation failed. Make sure you are logged in as Admin.');
    } finally {
      setIsGenerating(false);
    }
  };

  const copyToken = () => {
    navigator.clipboard.writeText(generatedToken);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const filteredLogs = auditLogs.filter((log) => {
    if (filterEvent === 'all') return true;
    return log.event === filterEvent;
  });

  const getEventBadge = (event: string, status: number) => {
    switch (event) {
      case 'auth_failure':
        return <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-red-500/10 text-red-500 border border-red-500/20">AUTH FAIL (401)</span>;
      case 'access_denied':
        return <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-rose-500/10 text-rose-500 border border-rose-500/20">FORBIDDEN (403)</span>;
      case 'rate_limited':
        return <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-amber-500/10 text-amber-500 border border-amber-500/20">RATE LIMIT (429)</span>;
      case 'payload_too_large':
        return <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-purple-500/10 text-purple-500 border border-purple-500/20">PAYLOAD LIMIT (413)</span>;
      case 'write_operation':
        return <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20">MUTATION ({status})</span>;
      default:
        return <span className="px-2 py-0.5 rounded text-[10px] font-mono font-bold bg-slate-500/10 text-slate-500 border border-slate-500/20">{event.toUpperCase()} ({status})</span>;
    }
  };

  return (
    <div className="p-6 space-y-6 overflow-y-auto max-h-[calc(100vh-4rem)] bg-slate-50 dark:bg-zinc-950">
      {/* Top Banner */}
      <div className="p-5 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-3">
        <div className="flex items-center justify-between flex-wrap gap-3">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-emerald-50 dark:bg-emerald-950/60 border border-emerald-200 dark:border-emerald-800 text-emerald-700 dark:text-emerald-400">
              <ShieldCheck className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-sm font-semibold text-slate-900 dark:text-zinc-100">
                Zero-Trust Hardware-Accelerated Security Suite
              </h2>
              <p className="text-xs text-slate-500 dark:text-zinc-400">
                AES-256-GCM AEAD encryption at rest, Argon2id password hashing, and live JSON-Lines audit logging.
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Badge variant="success">Zero-Trust Active</Badge>
            <Badge variant="outline">JWT RBAC Enforced</Badge>
          </div>
        </div>
      </div>

      {/* Security Pillars Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4 text-xs font-mono">
        <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-2">
          <div className="flex items-center justify-between text-slate-500 dark:text-zinc-400">
            <span>ENCRYPTION AT REST</span>
            <Lock className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
          </div>
          <p className="text-sm font-bold text-slate-900 dark:text-zinc-100">AES-256-GCM</p>
          <p className="text-[11px] text-slate-500 dark:text-zinc-400 font-sans">
            Hardware-accelerated AES-NI cipher with tamper-evident AEAD auth tags.
          </p>
        </div>

        <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-2">
          <div className="flex items-center justify-between text-slate-500 dark:text-zinc-400">
            <span>PASSWORD HASHING</span>
            <Key className="w-4 h-4 text-cyan-600 dark:text-cyan-400" />
          </div>
          <p className="text-sm font-bold text-slate-900 dark:text-zinc-100">Argon2id (Winner PHC)</p>
          <p className="text-[11px] text-slate-500 dark:text-zinc-400 font-sans">
            Memory-hard key derivation resistant against GPU/ASIC brute-force attacks.
          </p>
        </div>

        <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-2">
          <div className="flex items-center justify-between text-slate-500 dark:text-zinc-400">
            <span>RBAC ACCESS CONTROL</span>
            <UserCheck className="w-4 h-4 text-amber-600 dark:text-amber-400" />
          </div>
          <p className="text-sm font-bold text-slate-900 dark:text-zinc-100">JWT Token Claims</p>
          <p className="text-[11px] text-slate-500 dark:text-zinc-400 font-sans">
            Role-based scopes for Admin, ReadWrite, and ReadOnly client keys.
          </p>
        </div>

        <div className="p-4 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-2">
          <div className="flex items-center justify-between text-slate-500 dark:text-zinc-400">
            <span>DDOS & BRUTE-FORCE</span>
            <Activity className="w-4 h-4 text-indigo-600 dark:text-indigo-400" />
          </div>
          <p className="text-sm font-bold text-slate-900 dark:text-zinc-100">DashMap Rate Limiter</p>
          <p className="text-[11px] text-slate-500 dark:text-zinc-400 font-sans">
            In-memory sliding window + permanent IP auto-ban after 3 violations.
          </p>
        </div>
      </div>

      {/* ── Live Security Audit Trail Table ───────────────────────────────────── */}
      <div className="p-5 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-4">
        <div className="flex items-center justify-between flex-wrap gap-3 pb-3 border-b border-slate-200 dark:border-zinc-800">
          <div>
            <h3 className="text-sm font-semibold text-slate-900 dark:text-zinc-100 flex items-center gap-2">
              <FileText className="w-4 h-4 text-indigo-600 dark:text-indigo-400" />
              <span>Real-Time Security Event Audit Trail</span>
              <span className="px-2 py-0.5 rounded-full text-[10px] font-mono bg-indigo-500/10 text-indigo-500">
                {filteredLogs.length} Events
              </span>
            </h3>
            <p className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5">
              Live immutable stream logged asynchronously to <code className="bg-slate-100 dark:bg-zinc-800 px-1 py-0.5 rounded font-mono text-[11px]">./logs/audit.jsonl</code>.
            </p>
          </div>

          <div className="flex items-center gap-3">
            {/* Filter */}
            <select
              value={filterEvent}
              onChange={(e) => setFilterEvent(e.target.value)}
              className="bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg px-2.5 py-1.5 text-xs font-mono text-slate-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            >
              <option value="all">All Events</option>
              <option value="auth_failure">Auth Failures</option>
              <option value="access_denied">Forbidden Access</option>
              <option value="rate_limited">Rate Limited (429)</option>
              <option value="payload_too_large">Payload Too Large</option>
              <option value="write_operation">Data Mutations</option>
            </select>

            {/* Auto-refresh toggle */}
            <button
              onClick={() => setAutoRefresh(!autoRefresh)}
              className={`px-2.5 py-1.5 rounded-lg text-xs font-mono border transition flex items-center gap-1.5 ${
                autoRefresh
                  ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/30'
                  : 'bg-slate-100 dark:bg-zinc-800 text-slate-500 border-slate-300 dark:border-zinc-700'
              }`}
              title="Live 5s Polling"
            >
              <span className={`w-2 h-2 rounded-full ${autoRefresh ? 'bg-emerald-500 animate-pulse' : 'bg-slate-400'}`} />
              <span>Live {autoRefresh ? 'ON' : 'OFF'}</span>
            </button>

            {/* Manual Refresh */}
            <Button variant="outline" size="sm" onClick={fetchAuditLogs} disabled={loadingLogs}>
              <RefreshCw className={`w-3.5 h-3.5 ${loadingLogs ? 'animate-spin' : ''}`} />
              <span>Refresh</span>
            </Button>
          </div>
        </div>

        {/* Table / Log viewer */}
        <div className="overflow-x-auto rounded-lg border border-slate-200 dark:border-zinc-800">
          <table className="w-full text-left text-xs font-mono">
            <thead className="bg-slate-50 dark:bg-zinc-950 border-b border-slate-200 dark:border-zinc-800 text-slate-500 dark:text-zinc-400">
              <tr>
                <th className="py-2.5 px-3">Timestamp</th>
                <th className="py-2.5 px-3">Event</th>
                <th className="py-2.5 px-3">Client IP</th>
                <th className="py-2.5 px-3">Request Path</th>
                <th className="py-2.5 px-3">Trace Request ID</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-200 dark:divide-zinc-800/60 bg-white dark:bg-zinc-900">
              {filteredLogs.length === 0 ? (
                <tr>
                  <td colSpan={5} className="py-8 text-center text-slate-400 dark:text-zinc-500 font-sans">
                    No security events recorded yet. Mutations and security blocks will appear here automatically.
                  </td>
                </tr>
              ) : (
                filteredLogs.map((log, idx) => (
                  <tr key={idx} className="hover:bg-slate-50/70 dark:hover:bg-zinc-800/40 transition">
                    <td className="py-2.5 px-3 whitespace-nowrap text-slate-600 dark:text-zinc-300">
                      {new Date(log.timestamp).toLocaleTimeString()} · {new Date(log.timestamp).toLocaleDateString()}
                    </td>
                    <td className="py-2.5 px-3 whitespace-nowrap">
                      {getEventBadge(log.event, log.status)}
                    </td>
                    <td className="py-2.5 px-3 whitespace-nowrap text-slate-700 dark:text-zinc-300">
                      <span className="bg-slate-100 dark:bg-zinc-800 px-1.5 py-0.5 rounded">
                        {log.ip}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 text-slate-600 dark:text-zinc-400">
                      <code>{log.path}</code>
                    </td>
                    <td className="py-2.5 px-3 text-slate-400 dark:text-zinc-500 text-[11px] truncate max-w-[140px]">
                      {log.request_id}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </div>

      {/* ── Cryptographically Signed JWT Token Generator ────────────────────────── */}
      <div className="p-5 rounded-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 shadow-sm space-y-4">
        <div className="pb-2 border-b border-slate-200 dark:border-zinc-800">
          <h3 className="text-sm font-semibold text-slate-900 dark:text-zinc-100 flex items-center gap-2">
            <Key className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <span>Generate Cryptographically Signed JWT Token</span>
          </h3>
          <p className="text-xs text-slate-500 dark:text-zinc-400 mt-0.5">
            Signed by the database server's master key using Argon2id / HS256 algorithm.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
          <div className="space-y-1.5">
            <label className="text-xs font-mono text-slate-800 dark:text-zinc-200 font-semibold">Client / Subject</label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              placeholder="e.g. billing_service, game_server"
              className="w-full bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg px-3 py-2 text-xs font-mono text-slate-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-mono text-slate-800 dark:text-zinc-200 font-semibold">RBAC Role Scope</label>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as any)}
              className="w-full bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg px-3 py-2 text-xs font-mono text-slate-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            >
              <option value="admin">Admin (Full Access + Cluster + Backups)</option>
              <option value="read_write">ReadWrite (CRUD on all collections)</option>
              <option value="read_only">ReadOnly (Queries only)</option>
            </select>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-mono text-slate-800 dark:text-zinc-200 font-semibold">Expiration (Days)</label>
            <input
              type="number"
              min={1}
              max={365}
              value={validDays}
              onChange={(e) => setValidDays(Number(e.target.value))}
              className="w-full bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg px-3 py-2 text-xs font-mono text-slate-900 dark:text-zinc-100 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            />
          </div>
        </div>

        {genError && (
          <div className="p-3 rounded-lg bg-red-500/10 border border-red-500/30 text-red-500 text-xs flex items-center gap-2">
            <AlertTriangle className="w-4 h-4 flex-shrink-0" />
            <span>{genError}</span>
          </div>
        )}

        <div className="pt-2">
          <Button variant="primary" size="sm" onClick={handleGenerateToken} disabled={isGenerating}>
            <Zap className="w-3.5 h-3.5" />
            <span>{isGenerating ? 'Signing Token…' : 'Generate Signed JWT'}</span>
          </Button>
        </div>

        {generatedToken && (
          <div className="p-4 rounded-lg bg-slate-50 dark:bg-zinc-950 border border-slate-200 dark:border-zinc-800 space-y-2 mt-3 animate-in fade-in duration-200">
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-mono font-semibold text-emerald-700 dark:text-emerald-400">
                Server-Signed Bearer Token (JWT)
              </span>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setShowKey(!showKey)}
                  className="text-slate-500 hover:text-slate-900 dark:text-zinc-400 dark:hover:text-zinc-200 text-xs p-1"
                >
                  {showKey ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                </button>
                <Button variant="outline" size="sm" onClick={copyToken} className="text-xs py-1">
                  {copied ? (
                    <>
                      <Check className="w-3 h-3 text-emerald-500" />
                      <span>Copied!</span>
                    </>
                  ) : (
                    <>
                      <Copy className="w-3 h-3" />
                      <span>Copy</span>
                    </>
                  )}
                </Button>
              </div>
            </div>

            <p className="font-mono text-xs text-slate-900 dark:text-zinc-100 break-all bg-white dark:bg-zinc-900 p-3 rounded border border-slate-200 dark:border-zinc-800 shadow-xs">
              {showKey ? generatedToken : `${generatedToken.slice(0, 35)}••••••••••••••••••••••••••••••••`}
            </p>
          </div>
        )}
      </div>
    </div>
  );
};
