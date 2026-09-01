import React, { useState } from 'react';
import {
  Network,
  Share2,
  Play,
  RotateCcw,
  Sparkles,
  Info,
  ChevronRight,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';

interface GraphNode {
  id: string;
  label: string;
  type: string;
  x: number;
  y: number;
  color: string;
  properties: Record<string, any>;
}

interface GraphEdge {
  from: string;
  to: string;
  label: string;
  weight: number;
}

export const GraphExplorer: React.FC = () => {
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null);
  const [traversalDepth, setTraversalDepth] = useState<number>(2);
  const [activePath, setActivePath] = useState<string[]>([]);
  const [isTraversing, setIsTraversing] = useState<boolean>(false);

  // Sample Knowledge Graph nodes
  const nodes: GraphNode[] = [
    {
      id: 'faizdb_core',
      label: 'FaizDB Engine',
      type: 'Core System',
      x: 350,
      y: 180,
      color: '#10b981', // Emerald
      properties: { creator: 'Ahmad Faiz', version: '0.1.0', language: 'Rust' },
    },
    {
      id: 'lsm_storage',
      label: 'Hybrid LSM-Tree',
      type: 'Storage Engine',
      x: 200,
      y: 90,
      color: '#06b6d4', // Cyan
      properties: { memtable: 'BTreeMap', sstable: 'BloomFilter', wal: 'CRC32' },
    },
    {
      id: 'hnsw_vector',
      label: 'HNSW Vector Engine',
      type: 'AI Engine',
      x: 500,
      y: 90,
      color: '#8b5cf6', // Purple
      properties: { metric: 'Cosine', max_m: 16, ef_construction: 200 },
    },
    {
      id: 'mongo_wire',
      label: 'MongoDB Wire (27017)',
      type: 'Protocol',
      x: 180,
      y: 280,
      color: '#f59e0b', // Amber
      properties: { port: 27017, opcode: 'OP_MSG 2013', drop_in: true },
    },
    {
      id: 'graphrag',
      label: 'GraphRAG Context Engine',
      type: 'AI Engine',
      x: 520,
      y: 280,
      color: '#ec4899', // Pink
      properties: { traversal: 'BFS', use_case: 'AI Deep Reasoning' },
    },
  ];

  const edges: GraphEdge[] = [
    { from: 'faizdb_core', to: 'lsm_storage', label: 'PERSISTS_VIA', weight: 1.0 },
    { from: 'faizdb_core', to: 'hnsw_vector', label: 'INDEXES_EMBEDDINGS', weight: 1.0 },
    { from: 'faizdb_core', to: 'mongo_wire', label: 'EXPOSES_PROTOCOL', weight: 1.0 },
    { from: 'faizdb_core', to: 'graphrag', label: 'ENABLES_REASONING', weight: 1.0 },
    { from: 'hnsw_vector', to: 'graphrag', label: 'AUGMENTS_VECTORS', weight: 0.85 },
  ];

  const runGraphRAGTraversal = () => {
    setIsTraversing(true);
    setActivePath(['faizdb_core']);

    setTimeout(() => {
      setActivePath(['faizdb_core', 'lsm_storage', 'hnsw_vector', 'mongo_wire', 'graphrag']);
      setIsTraversing(false);
    }, 800);
  };

  const resetTraversal = () => {
    setActivePath([]);
    setSelectedNode(null);
  };

  return (
    <div className="flex h-[calc(100vh-4rem)] overflow-hidden">
      {/* Interactive Graph Canvas Area */}
      <div className="flex-1 flex flex-col p-4 space-y-4">
        {/* Canvas Toolbar */}
        <div className="glass-panel p-3.5 rounded-xl border border-zinc-800 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Network className="w-4 h-4 text-amber-400" />
            <span className="text-xs font-semibold text-zinc-100 font-mono">
              Knowledge Graph & GraphRAG Traversal Canvas
            </span>
          </div>

          <div className="flex items-center gap-2">
            <div className="flex items-center gap-2 text-xs font-mono text-zinc-400 mr-2">
              <span>Depth:</span>
              <select
                value={traversalDepth}
                onChange={(e) => setTraversalDepth(Number(e.target.value))}
                className="bg-zinc-900 border border-zinc-700 rounded px-2 py-0.5 text-zinc-100 text-xs"
              >
                <option value={1}>1-Hop</option>
                <option value={2}>2-Hops</option>
                <option value={3}>3-Hops</option>
              </select>
            </div>

            <Button
              variant="primary"
              size="sm"
              onClick={runGraphRAGTraversal}
              loading={isTraversing}
            >
              <Play className="w-3.5 h-3.5 fill-current" />
              <span>Simulate GraphRAG</span>
            </Button>

            <Button variant="outline" size="sm" onClick={resetTraversal}>
              <RotateCcw className="w-3.5 h-3.5" />
            </Button>
          </div>
        </div>

        {/* SVG Interactive Canvas */}
        <div className="glass-panel flex-1 rounded-xl border border-zinc-800 relative bg-zinc-950/80 overflow-hidden flex items-center justify-center">
          <svg className="w-full h-full min-h-[420px]" viewBox="0 0 700 360">
            {/* Draw Edges */}
            {edges.map((edge, idx) => {
              const source = nodes.find((n) => n.id === edge.from)!;
              const target = nodes.find((n) => n.id === edge.to)!;
              const isHighlighted =
                activePath.includes(edge.from) && activePath.includes(edge.to);

              const midX = (source.x + target.x) / 2;
              const midY = (source.y + target.y) / 2;

              return (
                <g key={idx}>
                  <line
                    x1={source.x}
                    y1={source.y}
                    x2={target.x}
                    y2={target.y}
                    stroke={isHighlighted ? '#10b981' : '#27272a'}
                    strokeWidth={isHighlighted ? 3 : 1.5}
                    strokeDasharray={isHighlighted ? '4,4' : undefined}
                    className="transition-all duration-300"
                  />
                  {/* Edge Label */}
                  <rect
                    x={midX - 35}
                    y={midY - 8}
                    width={70}
                    height={16}
                    rx={4}
                    fill="#09090b"
                    stroke="#27272a"
                  />
                  <text
                    x={midX}
                    y={midY + 3}
                    textAnchor="middle"
                    fill={isHighlighted ? '#34d399' : '#71717a'}
                    fontSize="8"
                    fontFamily="monospace"
                    fontWeight="600"
                  >
                    {edge.label}
                  </text>
                </g>
              );
            })}

            {/* Draw Nodes */}
            {nodes.map((node) => {
              const isSelected = selectedNode?.id === node.id;
              const isPathActive = activePath.includes(node.id);

              return (
                <g
                  key={node.id}
                  transform={`translate(${node.x}, ${node.y})`}
                  className="cursor-pointer group"
                  onClick={() => setSelectedNode(node)}
                >
                  {/* Outer glow ring if selected */}
                  {(isSelected || isPathActive) && (
                    <circle
                      r="34"
                      fill="none"
                      stroke={node.color}
                      strokeWidth="2"
                      opacity="0.6"
                      className="animate-pulse"
                    />
                  )}

                  {/* Node Circle */}
                  <circle
                    r="26"
                    fill="#121215"
                    stroke={node.color}
                    strokeWidth={isSelected ? 3 : 2}
                    className="transition-transform group-hover:scale-110"
                  />

                  {/* Icon or Type symbol */}
                  <text
                    textAnchor="middle"
                    y="4"
                    fill="#fafafa"
                    fontSize="10"
                    fontFamily="monospace"
                    fontWeight="bold"
                  >
                    {node.type.slice(0, 3).toUpperCase()}
                  </text>

                  {/* Label under node */}
                  <text
                    textAnchor="middle"
                    y="42"
                    fill="#e4e4e7"
                    fontSize="11"
                    fontFamily="sans-serif"
                    fontWeight="600"
                  >
                    {node.label}
                  </text>
                </g>
              );
            })}
          </svg>

          {/* Legend badge */}
          <div className="absolute bottom-3 left-3 bg-zinc-900/90 border border-zinc-800 rounded-lg p-2 text-[10px] font-mono text-zinc-400 space-y-1">
            <p className="text-zinc-200 font-semibold">GraphRAG Engine</p>
            <p>• Click any node to inspect properties</p>
            <p>• Run "Simulate GraphRAG" to animate traversal</p>
          </div>
        </div>
      </div>

      {/* Right Detail Inspector Panel */}
      <div className="w-80 border-l border-border bg-sidebar p-4 flex flex-col justify-between overflow-y-auto">
        <div className="space-y-4">
          <div className="flex items-center gap-2 pb-2 border-b border-border">
            <Info className="w-4 h-4 text-emerald-400" />
            <h3 className="text-sm font-semibold text-zinc-100">Vertex Inspector</h3>
          </div>

          {selectedNode ? (
            <div className="space-y-3">
              <div>
                <span className="text-[10px] font-mono uppercase tracking-wider text-zinc-400">
                  Node Identifier
                </span>
                <p className="text-sm font-mono font-bold text-emerald-400">
                  {selectedNode.id}
                </p>
              </div>

              <div>
                <span className="text-[10px] font-mono uppercase tracking-wider text-zinc-400">
                  Type & Category
                </span>
                <p className="text-xs font-semibold text-zinc-200 mt-0.5">
                  <Badge variant="info">{selectedNode.type}</Badge>
                </p>
              </div>

              <div>
                <span className="text-[10px] font-mono uppercase tracking-wider text-zinc-400">
                  Document Properties (JSON)
                </span>
                <pre className="mt-1 bg-zinc-950 p-2.5 rounded-lg border border-zinc-800 font-mono text-[11px] text-emerald-300 overflow-x-auto leading-relaxed">
                  {JSON.stringify(selectedNode.properties, null, 2)}
                </pre>
              </div>
            </div>
          ) : (
            <div className="py-12 text-center text-zinc-500 text-xs space-y-2">
              <Share2 className="w-8 h-8 mx-auto stroke-1 text-zinc-600" />
              <p>Click on any graph node to inspect its document schema and connected edges.</p>
            </div>
          )}
        </div>

        <div className="pt-4 border-t border-border/80 text-[11px] text-zinc-400 font-mono space-y-1">
          <p className="text-zinc-200 font-semibold">GraphRAG AI Traversal</p>
          <p>Enables LLM agents to perform multi-hop contextual reasoning over interconnected relational graphs.</p>
        </div>
      </div>
    </div>
  );
};
