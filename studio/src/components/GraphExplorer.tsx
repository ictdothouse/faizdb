import React, { useState } from 'react';
import {
  Network,
  Share2,
  Play,
  RotateCcw,
  Info,
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

  const nodes: GraphNode[] = [
    {
      id: 'faizdb_core',
      label: 'FaizDB Engine',
      type: 'Core System',
      x: 350,
      y: 180,
      color: '#10b981',
      properties: { creator: 'Ahmad Faiz', version: '0.1.0', language: 'Rust' },
    },
    {
      id: 'lsm_storage',
      label: 'Hybrid LSM-Tree',
      type: 'Storage Engine',
      x: 200,
      y: 90,
      color: '#06b6d4',
      properties: { memtable: 'BTreeMap', sstable: 'BloomFilter', wal: 'CRC32' },
    },
    {
      id: 'hnsw_vector',
      label: 'HNSW Vector Engine',
      type: 'AI Engine',
      x: 500,
      y: 90,
      color: '#8b5cf6',
      properties: { metric: 'Cosine', max_m: 16, ef_construction: 200 },
    },
    {
      id: 'mongo_wire',
      label: 'MongoDB Wire (27017)',
      type: 'Protocol',
      x: 180,
      y: 280,
      color: '#f59e0b',
      properties: { port: 27017, opcode: 'OP_MSG 2013', drop_in: true },
    },
    {
      id: 'graphrag',
      label: 'GraphRAG Context Engine',
      type: 'AI Engine',
      x: 520,
      y: 280,
      color: '#ec4899',
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
        <div className="glass-panel p-3.5 rounded-xl border border-border flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Network className="w-4 h-4 text-amber-500" />
            <span className="text-xs font-semibold text-foreground font-mono">
              Knowledge Graph & GraphRAG Traversal Canvas
            </span>
          </div>

          <div className="flex items-center gap-2">
            <div className="flex items-center gap-2 text-xs font-mono text-muted-foreground mr-2">
              <span>Depth:</span>
              <select
                value={traversalDepth}
                onChange={(e) => setTraversalDepth(Number(e.target.value))}
                className="bg-background border border-border rounded px-2 py-0.5 text-foreground text-xs"
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
        <div className="glass-panel flex-1 rounded-xl border border-border relative bg-card overflow-hidden flex items-center justify-center">
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
                    stroke={isHighlighted ? '#10b981' : '#cbd5e1'}
                    strokeWidth={isHighlighted ? 3 : 1.5}
                    strokeDasharray={isHighlighted ? '4,4' : undefined}
                    className="transition-all duration-300 dark:stroke-zinc-700"
                  />
                  <rect
                    x={midX - 38}
                    y={midY - 9}
                    width={76}
                    height={18}
                    rx={4}
                    className="fill-card stroke-border"
                  />
                  <text
                    x={midX}
                    y={midY + 3}
                    textAnchor="middle"
                    fill={isHighlighted ? '#059669' : '#64748b'}
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

                  <circle
                    r="26"
                    className="fill-card shadow-sm"
                    stroke={node.color}
                    strokeWidth={isSelected ? 3 : 2}
                  />

                  <text
                    textAnchor="middle"
                    y="4"
                    fill={node.color}
                    fontSize="10"
                    fontFamily="monospace"
                    fontWeight="bold"
                  >
                    {node.type.slice(0, 3).toUpperCase()}
                  </text>

                  <text
                    textAnchor="middle"
                    y="42"
                    className="fill-foreground font-semibold"
                    fontSize="11"
                    fontFamily="sans-serif"
                  >
                    {node.label}
                  </text>
                </g>
              );
            })}
          </svg>

          {/* Legend badge */}
          <div className="absolute bottom-3 left-3 bg-card/90 border border-border rounded-lg p-2.5 text-[10px] font-mono text-muted-foreground space-y-1 shadow-sm">
            <p className="text-foreground font-semibold">GraphRAG Engine</p>
            <p>• Click any node to inspect properties</p>
            <p>• Run "Simulate GraphRAG" to animate traversal</p>
          </div>
        </div>
      </div>

      {/* Right Detail Inspector Panel */}
      <div className="w-80 border-l border-border bg-sidebar p-4 flex flex-col justify-between overflow-y-auto">
        <div className="space-y-4">
          <div className="flex items-center gap-2 pb-2 border-b border-border">
            <Info className="w-4 h-4 text-emerald-600 dark:text-emerald-400" />
            <h3 className="text-sm font-semibold text-foreground">Vertex Inspector</h3>
          </div>

          {selectedNode ? (
            <div className="space-y-3">
              <div>
                <span className="text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
                  Node Identifier
                </span>
                <p className="text-sm font-mono font-bold text-emerald-700 dark:text-emerald-400">
                  {selectedNode.id}
                </p>
              </div>

              <div>
                <span className="text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
                  Type & Category
                </span>
                <p className="text-xs font-semibold text-foreground mt-0.5">
                  <Badge variant="info">{selectedNode.type}</Badge>
                </p>
              </div>

              <div>
                <span className="text-[10px] font-mono uppercase tracking-wider text-muted-foreground">
                  Document Properties (JSON)
                </span>
                <pre className="mt-1 bg-muted/40 p-3 rounded-lg border border-border font-mono text-[11px] text-foreground overflow-x-auto leading-relaxed shadow-xs">
                  {JSON.stringify(selectedNode.properties, null, 2)}
                </pre>
              </div>
            </div>
          ) : (
            <div className="py-12 text-center text-muted-foreground text-xs space-y-2">
              <Share2 className="w-8 h-8 mx-auto stroke-1 text-muted-foreground/60" />
              <p>Click on any graph node to inspect its document schema and connected edges.</p>
            </div>
          )}
        </div>

        <div className="pt-4 border-t border-border text-[11px] text-muted-foreground font-mono space-y-1">
          <p className="text-foreground font-semibold">GraphRAG AI Traversal</p>
          <p>Enables LLM agents to perform multi-hop contextual reasoning over interconnected relational graphs.</p>
        </div>
      </div>
    </div>
  );
};
