import React, { useState } from 'react';
import {
  Search,
  Filter,
  Trash2,
  Copy,
  Check,
  FileCode,
  Table as TableIcon,
  ChevronRight,
  Eye,
  Plus,
  RefreshCw,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';

interface TableExplorerProps {
  collectionName: string;
  documents: Record<string, any>[];
  loading: boolean;
  onRefresh: () => void;
  onOpenInsertModal: () => void;
  onDeleteDocument: (id: string) => void;
}

export const TableExplorer: React.FC<TableExplorerProps> = ({
  collectionName,
  documents,
  loading,
  onRefresh,
  onOpenInsertModal,
  onDeleteDocument,
}) => {
  const [searchTerm, setSearchTerm] = useState('');
  const [viewMode, setViewMode] = useState<'table' | 'json'>('table');
  const [selectedDoc, setSelectedDoc] = useState<Record<string, any> | null>(null);
  const [copiedId, setCopiedId] = useState<string | null>(null);

  const copyId = (id: string) => {
    navigator.clipboard.writeText(id);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  // Extract all unique column headers across all documents
  const allColumns = Array.from(
    new Set(
      documents.flatMap((doc) =>
        Object.keys(doc).filter((k) => k !== '_id')
      )
    )
  );

  // Filter documents by search term
  const filteredDocs = documents.filter((doc) => {
    if (!searchTerm) return true;
    const str = JSON.stringify(doc).toLowerCase();
    return str.includes(searchTerm.toLowerCase());
  });

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] overflow-hidden">
      {/* Top Toolbar */}
      <div className="p-4 border-b border-border bg-background/50 flex items-center justify-between gap-4">
        <div className="flex items-center gap-3 flex-1 max-w-md">
          <div className="relative w-full">
            <Search className="w-4 h-4 text-zinc-400 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              placeholder={`Filter in ${collectionName}...`}
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full bg-zinc-900 border border-zinc-700/80 rounded-lg pl-9 pr-4 py-1.5 text-xs text-zinc-100 placeholder:text-zinc-500 focus:outline-none focus:ring-1 focus:ring-emerald-500 font-mono"
            />
          </div>
        </div>

        <div className="flex items-center gap-2">
          {/* View mode toggle */}
          <div className="flex items-center bg-zinc-900 border border-zinc-800 rounded-lg p-0.5">
            <button
              onClick={() => setViewMode('table')}
              className={`p-1.5 rounded-md text-xs font-medium flex items-center gap-1.5 transition-colors ${
                viewMode === 'table'
                  ? 'bg-zinc-800 text-zinc-100 shadow-sm'
                  : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              <TableIcon className="w-3.5 h-3.5" />
              <span className="hidden sm:inline">Table</span>
            </button>
            <button
              onClick={() => setViewMode('json')}
              className={`p-1.5 rounded-md text-xs font-medium flex items-center gap-1.5 transition-colors ${
                viewMode === 'json'
                  ? 'bg-zinc-800 text-zinc-100 shadow-sm'
                  : 'text-zinc-400 hover:text-zinc-200'
              }`}
            >
              <FileCode className="w-3.5 h-3.5" />
              <span className="hidden sm:inline">JSON</span>
            </button>
          </div>

          <Button variant="outline" size="sm" onClick={onRefresh} loading={loading}>
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          </Button>

          <Button variant="primary" size="sm" onClick={onOpenInsertModal}>
            <Plus className="w-3.5 h-3.5" />
            <span>Insert Row</span>
          </Button>
        </div>
      </div>

      {/* Main Data View */}
      <div className="flex-1 overflow-auto">
        {loading ? (
          <div className="flex flex-col items-center justify-center h-64 gap-3">
            <div className="w-6 h-6 border-2 border-emerald-500 border-t-transparent rounded-full animate-spin" />
            <p className="text-xs text-zinc-400 font-mono">Fetching documents from FaizDB...</p>
          </div>
        ) : filteredDocs.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-80 gap-3 text-center p-6">
            <div className="w-12 h-12 rounded-xl bg-zinc-900 border border-zinc-800 flex items-center justify-center text-zinc-500">
              <TableIcon className="w-6 h-6" />
            </div>
            <div>
              <p className="text-sm font-medium text-zinc-200">No documents in {collectionName}</p>
              <p className="text-xs text-zinc-400 max-w-sm mt-1">
                Insert a document via the button above, or run an insert query in the FaizQL Console.
              </p>
            </div>
            <Button variant="primary" size="sm" onClick={onOpenInsertModal} className="mt-2">
              <Plus className="w-3.5 h-3.5" />
              <span>Insert First Document</span>
            </Button>
          </div>
        ) : viewMode === 'table' ? (
          <table className="w-full text-left text-xs border-collapse font-mono">
            <thead>
              <tr className="border-b border-border bg-zinc-900/90 sticky top-0 z-10">
                <th className="py-2.5 px-4 font-semibold text-zinc-400 w-48">_id</th>
                {allColumns.map((col) => (
                  <th key={col} className="py-2.5 px-4 font-semibold text-zinc-300">
                    {col}
                  </th>
                ))}
                <th className="py-2.5 px-4 text-right font-semibold text-zinc-400 w-24">
                  Actions
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/60">
              {filteredDocs.map((doc, idx) => {
                const id = doc._id || `row_${idx}`;
                return (
                  <tr
                    key={id}
                    className="table-cell-hover group hover:bg-zinc-800/20 transition-colors"
                  >
                    <td className="py-2.5 px-4 font-medium text-emerald-400 truncate max-w-xs flex items-center gap-1.5">
                      <span className="truncate">{id}</span>
                      <button
                        onClick={() => copyId(id)}
                        className="opacity-0 group-hover:opacity-100 p-0.5 text-zinc-500 hover:text-zinc-300"
                        title="Copy ID"
                      >
                        {copiedId === id ? (
                          <Check className="w-3 h-3 text-emerald-400" />
                        ) : (
                          <Copy className="w-3 h-3" />
                        )}
                      </button>
                    </td>

                    {allColumns.map((col) => {
                      const val = doc[col];
                      let displayVal = '-';
                      if (val !== undefined && val !== null) {
                        if (typeof val === 'object') {
                          displayVal = JSON.stringify(val);
                        } else {
                          displayVal = String(val);
                        }
                      }
                      return (
                        <td
                          key={col}
                          className="py-2.5 px-4 text-zinc-300 max-w-xs truncate"
                          title={displayVal}
                        >
                          {typeof val === 'boolean' ? (
                            <Badge variant={val ? 'success' : 'outline'}>
                              {String(val)}
                            </Badge>
                          ) : typeof val === 'number' ? (
                            <span className="text-amber-400">{val}</span>
                          ) : (
                            displayVal
                          )}
                        </td>
                      );
                    })}

                    <td className="py-2.5 px-4 text-right">
                      <div className="flex items-center justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                        <button
                          onClick={() => setSelectedDoc(doc)}
                          className="p-1 rounded text-zinc-400 hover:text-zinc-100 hover:bg-zinc-800"
                          title="View JSON"
                        >
                          <Eye className="w-3.5 h-3.5" />
                        </button>
                        <button
                          onClick={() => onDeleteDocument(id)}
                          className="p-1 rounded text-zinc-400 hover:text-rose-400 hover:bg-rose-950/40"
                          title="Delete Document"
                        >
                          <Trash2 className="w-3.5 h-3.5" />
                        </button>
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ) : (
          <div className="p-4 space-y-3">
            {filteredDocs.map((doc, idx) => (
              <div
                key={idx}
                className="p-3.5 rounded-lg bg-zinc-900/90 border border-zinc-800 font-mono text-xs text-zinc-300 overflow-x-auto"
              >
                <div className="flex items-center justify-between pb-2 mb-2 border-b border-zinc-800/80">
                  <span className="text-emerald-400 font-bold">_id: {doc._id}</span>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => copyId(JSON.stringify(doc, null, 2))}
                      className="text-zinc-500 hover:text-zinc-300 text-[11px] flex items-center gap-1"
                    >
                      <Copy className="w-3 h-3" />
                      <span>Copy JSON</span>
                    </button>
                  </div>
                </div>
                <pre className="text-zinc-300 leading-relaxed">
                  {JSON.stringify(doc, null, 2)}
                </pre>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Footer Info */}
      <div className="p-3 border-t border-border bg-background/80 flex items-center justify-between text-xs text-zinc-400 font-mono">
        <span>Showing {filteredDocs.length} of {documents.length} document(s)</span>
        <span>Collection: {collectionName}</span>
      </div>

      {/* Document Inspector Drawer/Modal */}
      {selectedDoc && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/75 backdrop-blur-sm">
          <div className="glass-panel w-full max-w-xl bg-zinc-900 border border-zinc-700 rounded-xl p-5 shadow-2xl space-y-4">
            <div className="flex items-center justify-between pb-2 border-b border-zinc-800">
              <h3 className="text-sm font-semibold text-zinc-100 font-mono">
                Document: {selectedDoc._id}
              </h3>
              <Button variant="ghost" size="sm" onClick={() => setSelectedDoc(null)}>
                Close
              </Button>
            </div>
            <div className="max-h-96 overflow-y-auto bg-zinc-950 p-3 rounded-lg border border-zinc-800 font-mono text-xs text-emerald-300">
              <pre>{JSON.stringify(selectedDoc, null, 2)}</pre>
            </div>
            <div className="flex justify-end gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  navigator.clipboard.writeText(JSON.stringify(selectedDoc, null, 2));
                  setSelectedDoc(null);
                }}
              >
                Copy to Clipboard
              </Button>
              <Button variant="primary" size="sm" onClick={() => setSelectedDoc(null)}>
                Done
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
