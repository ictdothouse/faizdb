import React, { useState, useEffect } from 'react';
import {
  ShieldCheck,
  Lock,
  Key,
  Database,
  Terminal,
  Zap,
  Check,
  Copy,
  Server,
  Activity,
  Cpu,
  ArrowRight,
  Eye,
  EyeOff,
  AlertCircle,
  Code2,
} from 'lucide-react';
import { api } from '../api/client';

interface LoginPageProps {
  onLogin: () => void;
}

export const LoginPage: React.FC<LoginPageProps> = ({ onLogin }) => {
  const [authMode, setAuthMode] = useState<'credentials' | 'jwt'>('credentials');
  const [username, setUsername] = useState('admin');
  const [password, setPassword] = useState('faizdb-admin-2026');
  const [jwtToken, setJwtToken] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [showPass, setShowPass] = useState(false);
  const [serverOnline, setServerOnline] = useState(true);
  const [copiedSnippet, setCopiedSnippet] = useState(false);

  // Check backend server health on mount
  useEffect(() => {
    api.getHealth()
      .then(() => setServerOnline(true))
      .catch(() => setServerOnline(false));
  }, []);

  const handleCredentialsSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!username || !password) return;
    setError(null);
    setLoading(true);
    try {
      await api.login(username, password);
      onLogin();
    } catch (err: any) {
      setError(err.message || 'Invalid username or password. Please verify FAIZDB_ADMIN_USER / PASS.');
    } finally {
      setLoading(false);
    }
  };

  const handleJwtSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!jwtToken.trim()) return;
    setError(null);
    setLoading(true);
    try {
      // Decode or verify token directly
      api.saveSession({
        token: jwtToken.trim(),
        username: 'custom_token_user',
        role: 'Admin',
        expiresAt: Date.now() + 86400 * 1000,
      });
      // Validate with whoami
      await api.whoami();
      onLogin();
    } catch (err: any) {
      api.clearSession();
      setError('Invalid JWT Token signature or expired token.');
    } finally {
      setLoading(false);
    }
  };

  const fillDefaultAdmin = () => {
    setUsername('admin');
    setPassword('faizdb-admin-2026');
    setError(null);
  };

  const fillReadOnlyDemo = () => {
    setUsername('viewer');
    setPassword('viewer-demo-2026');
    setError(null);
  };

  const copyConnectionUri = () => {
    navigator.clipboard.writeText('mongodb://127.0.0.1:27017');
    setCopiedSnippet(true);
    setTimeout(() => setCopiedSnippet(false), 2000);
  };

  return (
    <div className="min-h-[100dvh] w-screen flex bg-[#0c0d0e] text-zinc-100 font-sans selection:bg-emerald-500/30 selection:text-emerald-300">
      
      {/* ── LEFT PANE: Supabase-Style Minimalist Auth Form ───────────────────── */}
      <div className="w-full lg:w-[480px] xl:w-[540px] flex-shrink-0 flex flex-col justify-between p-8 sm:p-12 z-10 border-r border-zinc-800/80 bg-[#121315]/95 backdrop-blur-md">
        
        {/* Top Branding */}
        <div>
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-9 h-9 rounded-xl bg-emerald-500/10 border border-emerald-500/30 flex items-center justify-center text-emerald-400 shadow-sm shadow-emerald-500/10">
                <Database className="w-5 h-5" />
              </div>
              <div>
                <span className="font-mono text-sm font-bold tracking-tight text-white flex items-center gap-1.5">
                  FaizDB <span className="text-emerald-400 font-semibold">Studio</span>
                </span>
                <span className="text-[10px] font-mono text-zinc-500 block leading-none mt-0.5">
                  v0.1.0 · Zero-Trust NoSQL
                </span>
              </div>
            </div>

            {/* Server Health Status Pill */}
            <div className="flex items-center gap-2 px-2.5 py-1 rounded-full bg-zinc-900 border border-zinc-800 text-[11px] font-mono">
              <span className={`w-2 h-2 rounded-full ${serverOnline ? 'bg-emerald-400 animate-pulse' : 'bg-rose-500'}`} />
              <span className="text-zinc-400">{serverOnline ? 'Cluster: Online' : 'Engine: Offline'}</span>
            </div>
          </div>
        </div>

        {/* Center: Auth Card */}
        <div className="py-8">
          <div className="space-y-1.5 mb-6">
            <h1 className="text-2xl font-bold tracking-tight text-white">
              Welcome back
            </h1>
            <p className="text-xs text-zinc-400 font-sans">
              Sign in to manage your cluster, collections, and AI vector streams.
            </p>
          </div>

          {/* Auth Method Toggle Tabs */}
          <div className="grid grid-cols-2 gap-1 p-1 bg-zinc-900/90 rounded-lg border border-zinc-800 mb-6 text-xs font-mono">
            <button
              type="button"
              onClick={() => setAuthMode('credentials')}
              className={`py-1.5 rounded-md transition-all font-medium ${
                authMode === 'credentials'
                  ? 'bg-zinc-800 text-white shadow-xs'
                  : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              Account Login
            </button>
            <button
              type="button"
              onClick={() => setAuthMode('jwt')}
              className={`py-1.5 rounded-md transition-all font-medium ${
                authMode === 'jwt'
                  ? 'bg-zinc-800 text-white shadow-xs'
                  : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              API Token (JWT)
            </button>
          </div>

          {/* Mode 1: Username / Password */}
          {authMode === 'credentials' ? (
            <form onSubmit={handleCredentialsSubmit} className="space-y-4 font-mono text-xs">
              <div>
                <label className="block text-zinc-300 text-xs font-medium mb-1.5">
                  Database Username
                </label>
                <div className="relative">
                  <input
                    id="faizdb-login-user"
                    type="text"
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                    placeholder="admin"
                    autoComplete="username"
                    autoFocus
                    required
                    className="w-full bg-[#0c0d0e] border border-zinc-800 rounded-lg px-3.5 py-2.5 text-zinc-100 placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-500 focus:border-emerald-500 transition text-xs font-mono"
                  />
                </div>
              </div>

              <div>
                <div className="flex items-center justify-between mb-1.5">
                  <label className="text-zinc-300 text-xs font-medium">
                    Master Password
                  </label>
                  <button
                    type="button"
                    onClick={() => setShowPass(!showPass)}
                    className="text-[11px] text-zinc-500 hover:text-zinc-300 flex items-center gap-1 transition"
                  >
                    {showPass ? <EyeOff className="w-3 h-3" /> : <Eye className="w-3 h-3" />}
                    <span>{showPass ? 'Hide' : 'Show'}</span>
                  </button>
                </div>
                <div className="relative">
                  <input
                    id="faizdb-login-pass"
                    type={showPass ? 'text' : 'password'}
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="••••••••••••••••"
                    autoComplete="current-password"
                    required
                    className="w-full bg-[#0c0d0e] border border-zinc-800 rounded-lg px-3.5 py-2.5 text-zinc-100 placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-500 focus:border-emerald-500 transition text-xs font-mono"
                  />
                </div>
              </div>

              {error && (
                <div className="p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 text-xs font-sans flex items-start gap-2 animate-in fade-in duration-150">
                  <AlertCircle className="w-4 h-4 flex-shrink-0 mt-0.5" />
                  <span>{error}</span>
                </div>
              )}

              {/* Supabase-style Primary CTA */}
              <button
                id="faizdb-submit-btn"
                type="submit"
                disabled={loading || !username || !password}
                className="w-full mt-2 py-2.5 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-bold text-xs shadow-sm hover:shadow-emerald-500/20 transition-all flex items-center justify-center gap-2 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {loading ? (
                  <>
                    <svg className="w-4 h-4 animate-spin text-zinc-950" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5">
                      <path d="M21 12a9 9 0 1 1-6.219-8.56" />
                    </svg>
                    <span>Authenticating…</span>
                  </>
                ) : (
                  <>
                    <span>Sign In to FaizDB</span>
                    <ArrowRight className="w-3.5 h-3.5" />
                  </>
                )}
              </button>

              {/* Quick-fill helper pills */}
              <div className="pt-2">
                <p className="text-[11px] text-zinc-500 mb-2">Quick autofill preset:</p>
                <div className="flex items-center gap-2">
                  <button
                    type="button"
                    onClick={fillDefaultAdmin}
                    className="px-2.5 py-1 rounded bg-zinc-900 hover:bg-zinc-800 border border-zinc-800 text-[11px] text-zinc-300 hover:text-white transition cursor-pointer"
                  >
                    ⚡ Admin (admin:faizdb-admin-2026)
                  </button>
                </div>
              </div>
            </form>
          ) : (
            /* Mode 2: JWT Bearer Token */
            <form onSubmit={handleJwtSubmit} className="space-y-4 font-mono text-xs">
              <div>
                <label className="block text-zinc-300 text-xs font-medium mb-1.5">
                  Bearer Authorization Token (JWT)
                </label>
                <textarea
                  rows={4}
                  value={jwtToken}
                  onChange={(e) => setJwtToken(e.target.value)}
                  placeholder="eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9..."
                  className="w-full bg-[#0c0d0e] border border-zinc-800 rounded-lg p-3 text-zinc-100 placeholder-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-500 focus:border-emerald-500 transition text-[11px] font-mono leading-relaxed"
                />
              </div>

              {error && (
                <div className="p-3 rounded-lg bg-rose-500/10 border border-rose-500/20 text-rose-400 text-xs font-sans flex items-start gap-2">
                  <AlertCircle className="w-4 h-4 flex-shrink-0 mt-0.5" />
                  <span>{error}</span>
                </div>
              )}

              <button
                type="submit"
                disabled={loading || !jwtToken.trim()}
                className="w-full py-2.5 rounded-lg bg-emerald-500 hover:bg-emerald-400 text-zinc-950 font-bold text-xs shadow-sm hover:shadow-emerald-500/20 transition-all flex items-center justify-center gap-2 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <span>Authorize Session</span>
                <ArrowRight className="w-3.5 h-3.5" />
              </button>
            </form>
          )}
        </div>

        {/* Footer Security Badges */}
        <div className="pt-4 border-t border-zinc-800/80 flex items-center justify-between text-[11px] font-mono text-zinc-500">
          <div className="flex items-center gap-2">
            <ShieldCheck className="w-3.5 h-3.5 text-emerald-400" />
            <span>Argon2id + AES-256</span>
          </div>
          <span>HTTP 27018 · Wire 27017</span>
        </div>
      </div>

      {/* ── RIGHT PANE: Supabase-Style Developer Cockpit Showcase ───────────── */}
      <div className="hidden lg:flex flex-1 flex-col justify-between p-12 relative overflow-hidden bg-gradient-to-b from-[#0c0d0e] via-[#101114] to-[#0c0d0e]">
        
        {/* Subtle grid background */}
        <div
          className="absolute inset-0 opacity-[0.07] pointer-events-none"
          style={{
            backgroundImage: `
              linear-gradient(to right, #3ecf8e 1px, transparent 1px),
              linear-gradient(to bottom, #3ecf8e 1px, transparent 1px)
            `,
            backgroundSize: '48px 48px',
          }}
        />

        {/* Glow ambient background */}
        <div className="absolute top-1/4 right-1/4 w-96 h-96 bg-emerald-500/10 rounded-full blur-3xl pointer-events-none" />
        <div className="absolute bottom-1/4 left-1/4 w-80 h-80 bg-cyan-500/10 rounded-full blur-3xl pointer-events-none" />

        {/* Top Section: Quick Value Pitch */}
        <div className="relative z-10 space-y-2 max-w-xl">
          <span className="text-[11px] font-mono uppercase tracking-widest text-emerald-400 font-bold">
            The AI-Native Unified Database
          </span>
          <h2 className="text-3xl font-extrabold text-white tracking-tight leading-tight">
            Drop-in MongoDB replacement with sub-millisecond Rust speed.
          </h2>
          <p className="text-zinc-400 text-sm leading-relaxed">
            Consolidate your stack. Document storage, vector embeddings, graph traversal, and full-text search in a single memory-safe binary.
          </p>
        </div>

        {/* Center: Live Terminal & Connection Snippet Card */}
        <div className="relative z-10 my-8 max-w-xl">
          <div className="rounded-xl border border-zinc-800 bg-[#121316]/90 shadow-2xl overflow-hidden backdrop-blur-md">
            
            {/* Terminal Window Bar */}
            <div className="px-4 py-2.5 bg-zinc-900/80 border-b border-zinc-800/80 flex items-center justify-between">
              <div className="flex items-center gap-2">
                <span className="w-2.5 h-2.5 rounded-full bg-rose-500/80 inline-block" />
                <span className="w-2.5 h-2.5 rounded-full bg-amber-500/80 inline-block" />
                <span className="w-2.5 h-2.5 rounded-full bg-emerald-500/80 inline-block" />
                <span className="ml-2 text-[11px] font-mono text-zinc-400">connection_string.env</span>
              </div>
              <button
                onClick={copyConnectionUri}
                className="text-[11px] font-mono text-zinc-400 hover:text-white flex items-center gap-1 transition"
              >
                {copiedSnippet ? (
                  <>
                    <Check className="w-3 h-3 text-emerald-400" />
                    <span className="text-emerald-400">Copied!</span>
                  </>
                ) : (
                  <>
                    <Copy className="w-3 h-3" />
                    <span>Copy URI</span>
                  </>
                )}
              </button>
            </div>

            {/* Code Body */}
            <div className="p-4 font-mono text-xs space-y-2 text-zinc-300">
              <div className="flex items-center gap-2 text-emerald-400">
                <span className="text-zinc-600"># Direct Drop-in with Mongoose, PyMongo, or MongoDB Compass:</span>
              </div>
              <p className="text-white bg-zinc-950 p-2.5 rounded border border-zinc-800/80 selection:bg-emerald-500/30">
                MONGODB_URI=<span className="text-emerald-400">mongodb://127.0.0.1:27017</span>
              </p>
              
              <div className="pt-2 text-[11px] text-zinc-400 grid grid-cols-2 gap-2">
                <div className="flex items-center gap-2">
                  <Activity className="w-3.5 h-3.5 text-emerald-400" />
                  <span>Latency: <strong className="text-white">&lt; 0.1ms</strong></span>
                </div>
                <div className="flex items-center gap-2">
                  <Cpu className="w-3.5 h-3.5 text-cyan-400" />
                  <span>Engine: <strong className="text-white">100% Rust</strong></span>
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Bottom Feature Highlights Ticker */}
        <div className="relative z-10 grid grid-cols-3 gap-4 border-t border-zinc-800/80 pt-6 max-w-xl text-xs font-mono">
          <div className="space-y-1">
            <p className="font-bold text-white">Native HNSW</p>
            <p className="text-zinc-500 text-[11px]">Vector ANN &lt; 1ms</p>
          </div>
          <div className="space-y-1">
            <p className="font-bold text-white">Okapi BM25</p>
            <p className="text-zinc-500 text-[11px]">Fuzzy Text Search</p>
          </div>
          <div className="space-y-1">
            <p className="font-bold text-white">Raft Sharding</p>
            <p className="text-zinc-500 text-[11px]">16,384 Hash Slots</p>
          </div>
        </div>

      </div>
    </div>
  );
};
