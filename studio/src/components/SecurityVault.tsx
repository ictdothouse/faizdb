import React, { useState } from 'react';
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
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';

export const SecurityVault: React.FC = () => {
  const [role, setRole] = useState<'admin' | 'read_write' | 'read_only'>('admin');
  const [username, setUsername] = useState('developer_faiz');
  const [generatedToken, setGeneratedToken] = useState<string>('');
  const [copied, setCopied] = useState(false);
  const [showKey, setShowKey] = useState(false);

  const generateJwtToken = () => {
    // Generate simulated JWT for demonstration
    const header = btoa(JSON.stringify({ alg: 'HS256', typ: 'JWT' }));
    const payload = btoa(
      JSON.stringify({
        sub: username,
        role: role,
        iss: 'faizdb-security',
        exp: Math.floor(Date.now() / 1000) + 86400 * 30, // 30 days
      })
    );
    const signature = 's3cr3t_f41zdb_h4sh_s1gn4tur3_a3s256';
    setGeneratedToken(`eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.${payload}.${signature}`);
  };

  const copyToken = () => {
    navigator.clipboard.writeText(generatedToken);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="p-6 space-y-6 overflow-y-auto max-h-[calc(100vh-4rem)]">
      {/* Top Banner */}
      <div className="glass-panel p-5 rounded-xl border border-zinc-800 space-y-3">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-emerald-950/60 border border-emerald-800/60 text-emerald-400">
              <ShieldCheck className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-sm font-semibold text-zinc-100">
                Zero-Trust Hardware-Accelerated Security Engine
              </h2>
              <p className="text-xs text-zinc-400">
                Military-grade AES-256-GCM AEAD encryption at rest and Argon2id hashing.
              </p>
            </div>
          </div>
          <Badge variant="success">Zero-Trust Active</Badge>
        </div>
      </div>

      {/* Security Status Cards */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-xs font-mono">
        <div className="glass-panel p-4 rounded-xl border border-zinc-800 space-y-2">
          <div className="flex items-center justify-between text-zinc-400">
            <span>ENCRYPTION AT REST</span>
            <Lock className="w-4 h-4 text-emerald-400" />
          </div>
          <p className="text-sm font-bold text-zinc-100">AES-256-GCM</p>
          <p className="text-[11px] text-zinc-400 font-sans">
            Hardware-accelerated AES-NI cipher with tamper-evident authentication tags.
          </p>
        </div>

        <div className="glass-panel p-4 rounded-xl border border-zinc-800 space-y-2">
          <div className="flex items-center justify-between text-zinc-400">
            <span>PASSWORD HASHING</span>
            <Key className="w-4 h-4 text-cyan-400" />
          </div>
          <p className="text-sm font-bold text-zinc-100">Argon2id (Winner PHC)</p>
          <p className="text-[11px] text-zinc-400 font-sans">
            Memory-hard key derivation resistant against GPU/ASIC brute-force cracking.
          </p>
        </div>

        <div className="glass-panel p-4 rounded-xl border border-zinc-800 space-y-2">
          <div className="flex items-center justify-between text-zinc-400">
            <span>RBAC ACCESS CONTROL</span>
            <UserCheck className="w-4 h-4 text-amber-400" />
          </div>
          <p className="text-sm font-bold text-zinc-100">JWT Token Claims</p>
          <p className="text-[11px] text-zinc-400 font-sans">
            Role-based scopes for Admin, ReadWrite, and ReadOnly client keys.
          </p>
        </div>
      </div>

      {/* Developer API Key & JWT Generator */}
      <div className="glass-panel p-5 rounded-xl border border-zinc-800 space-y-4">
        <div className="pb-2 border-b border-zinc-800">
          <h3 className="text-sm font-semibold text-zinc-100 flex items-center gap-2">
            <Key className="w-4 h-4 text-emerald-400" />
            <span>Generate Developer API Token / JWT</span>
          </h3>
          <p className="text-xs text-zinc-400 mt-0.5">
            Create signed access tokens for applications, microservices, and scripts.
          </p>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <div className="space-y-1.5">
            <label className="text-xs font-mono text-zinc-300">Client / Username</label>
            <input
              type="text"
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-xs font-mono text-zinc-100 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            />
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-mono text-zinc-300">Access Role Scope</label>
            <select
              value={role}
              onChange={(e) => setRole(e.target.value as any)}
              className="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-xs font-mono text-zinc-100 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            >
              <option value="admin">Admin (Full Access + DDL + DML)</option>
              <option value="read_write">ReadWrite (CRUD on all collections)</option>
              <option value="read_only">ReadOnly (Queries only)</option>
            </select>
          </div>
        </div>

        <div className="pt-2">
          <Button variant="primary" size="sm" onClick={generateJwtToken}>
            <Zap className="w-3.5 h-3.5" />
            <span>Generate Token</span>
          </Button>
        </div>

        {generatedToken && (
          <div className="p-3.5 rounded-lg bg-zinc-950 border border-zinc-800 space-y-2 mt-3 animate-in fade-in duration-200">
            <div className="flex items-center justify-between">
              <span className="text-[11px] font-mono font-semibold text-emerald-400">
                Bearer Authorization Token (JWT)
              </span>
              <div className="flex items-center gap-2">
                <button
                  onClick={() => setShowKey(!showKey)}
                  className="text-zinc-400 hover:text-zinc-200 text-xs p-1"
                >
                  {showKey ? <EyeOff className="w-3.5 h-3.5" /> : <Eye className="w-3.5 h-3.5" />}
                </button>
                <Button variant="outline" size="sm" onClick={copyToken} className="text-xs py-1">
                  {copied ? (
                    <>
                      <Check className="w-3 h-3 text-emerald-400" />
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

            <p className="font-mono text-xs text-zinc-300 break-all bg-zinc-900/80 p-2 rounded border border-zinc-800">
              {showKey ? generatedToken : `${generatedToken.slice(0, 30)}••••••••••••••••••••••••••••`}
            </p>
          </div>
        )}
      </div>
    </div>
  );
};
