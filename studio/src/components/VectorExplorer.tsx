import React, { useState } from 'react';
import {
  BrainCircuit,
  Search,
  Sparkles,
  Zap,
  Sliders,
  Layers,
  ArrowRight,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';

export const VectorExplorer: React.FC = () => {
  const [queryVector, setQueryVector] = useState<string>('0.85, 0.42, 0.15, 0.93');
  const [metric, setMetric] = useState<'cosine' | 'euclidean' | 'dot' | 'manhattan'>('cosine');
  const [topK, setTopK] = useState<number>(4);
  const [searching, setSearching] = useState(false);

  // Sample vector dataset indexed in FaizDB HNSW
  const vectorDataset = [
    {
      id: 'doc_ai_001',
      title: 'Neural Network Architecture & Backprop',
      category: 'Deep Learning',
      vector: [0.82, 0.41, 0.18, 0.91],
    },
    {
      id: 'doc_ai_002',
      title: 'Transformer Attention Mechanisms & LLMs',
      category: 'NLP & GenAI',
      vector: [0.88, 0.39, 0.12, 0.95],
    },
    {
      id: 'doc_ai_003',
      title: 'Distributed Systems & Raft Consensus',
      category: 'Infrastructure',
      vector: [0.12, 0.95, 0.74, 0.22],
    },
    {
      id: 'doc_ai_004',
      title: 'PostgreSQL vs NoSQL Storage Internals',
      category: 'Databases',
      vector: [0.35, 0.88, 0.62, 0.41],
    },
    {
      id: 'doc_ai_005',
      title: 'Reinforcement Learning with Human Feedback',
      category: 'AI Alignment',
      vector: [0.79, 0.45, 0.22, 0.88],
    },
  ];

  // Calculate similarity between query and dataset items
  const parseVector = (str: string): number[] => {
    return str
      .split(',')
      .map((s) => parseFloat(s.trim()))
      .filter((n) => !isNaN(n));
  };

  const parsedQuery = parseVector(queryVector);

  const calculateSimilarity = (v1: number[], v2: number[]): number => {
    if (v1.length === 0 || v2.length === 0) return 0;
    // Cosine similarity
    let dot = 0;
    let norm1 = 0;
    let norm2 = 0;
    for (let i = 0; i < Math.min(v1.length, v2.length); i++) {
      dot += v1[i] * v2[i];
      norm1 += v1[i] * v1[i];
      norm2 += v2[i] * v2[i];
    }
    if (norm1 === 0 || norm2 === 0) return 0;
    return dot / (Math.sqrt(norm1) * Math.sqrt(norm2));
  };

  const scoredResults = vectorDataset
    .map((item) => {
      const sim = calculateSimilarity(parsedQuery, item.vector);
      return {
        ...item,
        score: Math.round(sim * 1000) / 1000,
        percentage: Math.max(0, Math.min(100, Math.round(sim * 100))),
      };
    })
    .sort((a, b) => b.score - a.score)
    .slice(0, topK);

  return (
    <div className="p-6 space-y-6 overflow-y-auto max-h-[calc(100vh-4rem)]">
      {/* Top Banner */}
      <div className="glass-panel p-5 rounded-xl border border-zinc-800 space-y-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-lg bg-emerald-950/60 border border-emerald-800/60 text-emerald-400">
              <BrainCircuit className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-sm font-semibold text-zinc-100">
                Hierarchical Navigable Small World (HNSW) ANN Engine
              </h2>
              <p className="text-xs text-zinc-400">
                Native vector similarity indexing with zero external plugins required.
              </p>
            </div>
          </div>
          <Badge variant="success" className="gap-1">
            <Zap className="w-3 h-3 text-emerald-400" />
            <span>Sub-ms ANN Search</span>
          </Badge>
        </div>

        {/* Vector Query Inputs */}
        <div className="grid grid-cols-1 md:grid-cols-3 gap-4 pt-2">
          <div className="md:col-span-2 space-y-1.5">
            <label className="text-xs font-mono font-medium text-zinc-300">
              Query Embedding Vector (Float Array)
            </label>
            <div className="relative">
              <input
                type="text"
                value={queryVector}
                onChange={(e) => setQueryVector(e.target.value)}
                className="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-xs font-mono text-emerald-300 placeholder:text-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-500"
                placeholder="e.g. 0.85, 0.42, 0.15, 0.93"
              />
            </div>
            <div className="flex items-center gap-2 pt-1">
              <span className="text-[11px] text-zinc-400 font-mono">Quick vectors:</span>
              <button
                onClick={() => setQueryVector('0.85, 0.42, 0.15, 0.93')}
                className="text-[11px] font-mono text-emerald-400 hover:underline"
              >
                AI/NLP Topic
              </button>
              <span className="text-zinc-600">•</span>
              <button
                onClick={() => setQueryVector('0.15, 0.92, 0.70, 0.25')}
                className="text-[11px] font-mono text-emerald-400 hover:underline"
              >
                Distributed Systems
              </button>
            </div>
          </div>

          <div className="space-y-1.5">
            <label className="text-xs font-mono font-medium text-zinc-300">
              Distance Metric
            </label>
            <select
              value={metric}
              onChange={(e) => setMetric(e.target.value as any)}
              className="w-full bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 text-xs font-mono text-zinc-100 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            >
              <option value="cosine">Cosine Distance (Recommended)</option>
              <option value="euclidean">Euclidean (L2) Distance</option>
              <option value="dot">Dot Product</option>
              <option value="manhattan">Manhattan (L1) Distance</option>
            </select>
          </div>
        </div>
      </div>

      {/* Results Ranking */}
      <div className="glass-panel p-5 rounded-xl border border-zinc-800 space-y-4">
        <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
          <div className="flex items-center gap-2">
            <Sliders className="w-4 h-4 text-emerald-400" />
            <h3 className="text-sm font-semibold text-zinc-100">
              Nearest Neighbors (Top {topK} Matches)
            </h3>
          </div>
          <span className="text-xs text-zinc-400 font-mono">Indexed Embeddings: 5</span>
        </div>

        <div className="space-y-3">
          {scoredResults.map((item, idx) => (
            <div
              key={item.id}
              className="p-3.5 rounded-lg bg-zinc-900/90 border border-zinc-800 hover:border-zinc-700 transition-all space-y-2"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="text-xs font-bold font-mono px-2 py-0.5 rounded bg-zinc-800 text-zinc-300">
                    #{idx + 1}
                  </span>
                  <span className="text-xs font-semibold text-zinc-100 font-mono">
                    {item.title}
                  </span>
                  <Badge variant="outline">{item.category}</Badge>
                </div>
                <div className="text-right">
                  <span className="text-xs font-mono font-bold text-emerald-400">
                    Score: {item.score}
                  </span>
                </div>
              </div>

              {/* Similarity Bar */}
              <div className="space-y-1">
                <div className="w-full bg-zinc-950 rounded-full h-2 overflow-hidden border border-zinc-800">
                  <div
                    className="bg-gradient-to-r from-emerald-500 to-teal-400 h-2 rounded-full transition-all duration-300"
                    style={{ width: `${item.percentage}%` }}
                  />
                </div>
                <div className="flex items-center justify-between text-[10px] text-zinc-400 font-mono">
                  <span>ID: {item.id}</span>
                  <span>Embedding: [{item.vector.join(', ')}]</span>
                  <span>{item.percentage}% Match</span>
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};
