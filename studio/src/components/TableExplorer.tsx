import React, { useState } from 'react';
import {
  Search,
  Trash2,
  Copy,
  Check,
  FileCode,
  Table as TableIcon,
  Eye,
  Plus,
  RefreshCw,
  Upload,
  FileText,
  AlertCircle,
  CheckCircle2,
} from 'lucide-react';
import { Button } from './ui/Button';
import { Badge } from './ui/Badge';
import { api } from '../api/client';

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

  // Import Modal State
  const [isImportModalOpen, setIsImportModalOpen] = useState(false);
  const [importText, setImportText] = useState('');
  const [importFormat, setImportFormat] = useState<'json' | 'csv'>('json');
  const [importLoading, setImportLoading] = useState(false);
  const [importResult, setImportResult] = useState<{ count: number; error?: string } | null>(null);

  const copyId = (id: string) => {
    navigator.clipboard.writeText(id);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const allColumns = Array.from(
    new Set(
      documents.flatMap((doc) =>
        Object.keys(doc).filter((k) => k !== '_id')
      )
    )
  );

  const filteredDocs = documents.filter((doc) => {
    if (!searchTerm) return true;
    const str = JSON.stringify(doc).toLowerCase();
    return str.includes(searchTerm.toLowerCase());
  });

  const handleFileUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const isCsv = file.name.endsWith('.csv');
    setImportFormat(isCsv ? 'csv' : 'json');

    const reader = new FileReader();
    reader.onload = (event) => {
      const content = event.target?.result as string;
      setImportText(content || '');
    };
    reader.readAsText(file);
  };

  const handleExecuteImport = async () => {
    if (!importText.trim()) return;
    setImportLoading(true);
    setImportResult(null);

    try {
      let payload: { documents?: any[]; csv?: string };
      if (importFormat === 'json') {
        const parsed = JSON.parse(importText);
        const docs = Array.isArray(parsed) ? parsed : [parsed];
        payload = { documents: docs };
      } else {
        payload = { csv: importText };
      }

      const res = await api.importData(collectionName, payload);
      setImportResult({ count: res.imported_count });
      onRefresh();
      setTimeout(() => {
        setIsImportModalOpen(false);
        setImportText('');
        setImportResult(null);
      }, 1500);
    } catch (err: any) {
      setImportResult({ count: 0, error: err.message || 'Import failed' });
    } finally {
      setImportLoading(false);
    }
  };

  return (
    <div className="flex flex-col h-[calc(100vh-4rem)] overflow-hidden bg-slate-50 dark:bg-zinc-950">
      {/* Top Toolbar */}
      <div className="p-4 border-b border-slate-200 dark:border-zinc-800 bg-white dark:bg-zinc-900/80 flex items-center justify-between gap-4">
        <div className="flex items-center gap-3 flex-1 max-w-md">
          <div className="relative w-full">
            <Search className="w-4 h-4 text-slate-400 absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              placeholder={`Filter in ${collectionName}...`}
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full bg-slate-50 dark:bg-zinc-950 border border-slate-300 dark:border-zinc-700 rounded-lg pl-9 pr-4 py-1.5 text-xs text-slate-900 dark:text-zinc-100 placeholder:text-slate-400 dark:placeholder:text-zinc-500 focus:outline-hidden focus:ring-1 focus:ring-emerald-500 font-mono"
            />
          </div>
        </div>

        <div className="flex items-center gap-2">
          {/* View mode toggle */}
          <div className="flex items-center bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 rounded-lg p-0.5">
            <button
              onClick={() => setViewMode('table')}
              className={`p-1.5 rounded-md text-xs font-medium flex items-center gap-1.5 transition-colors cursor-pointer ${
                viewMode === 'table'
                  ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs'
                  : 'text-slate-500 dark:text-zinc-400 hover:text-slate-900 dark:hover:text-zinc-200'
              }`}
            >
              <TableIcon className="w-3.5 h-3.5" />
              <span className="hidden sm:inline">Table</span>
            </button>
            <button
              onClick={() => setViewMode('json')}
              className={`p-1.5 rounded-md text-xs font-medium flex items-center gap-1.5 transition-colors cursor-pointer ${
                viewMode === 'json'
                  ? 'bg-white dark:bg-zinc-900 text-slate-900 dark:text-zinc-100 shadow-xs'
                  : 'text-slate-500 dark:text-zinc-400 hover:text-slate-900 dark:hover:text-zinc-200'
              }`}
            >
              <FileCode className="w-3.5 h-3.5" />
              <span className="hidden sm:inline">JSON</span>
            </button>
          </div>

          <Button variant="outline" size="sm" onClick={onRefresh} loading={loading}>
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
          </Button>

          <Button variant="outline" size="sm" onClick={() => setIsImportModalOpen(true)}>
            <Upload className="w-3.5 h-3.5" />
            <span className="hidden sm:inline">Import CSV/JSON</span>
          </Button>

          <Button variant="primary" size="sm" onClick={onOpenInsertModal}>
            <Plus className="w-3.5 h-3.5" />
            <span>Insert Row</span>
          </Button>
        </div>
      </div>

      {/* Main Data View */}
      <div className="flex-1 overflow-auto bg-white dark:bg-zinc-900">
        {loading ? (
          <div className="flex flex-col items-center justify-center h-64 gap-3">
            <div className="w-6 h-6 border-2 border-emerald-500 border-t-transparent rounded-full animate-spin" />
            <p className="text-xs text-slate-500 dark:text-zinc-400 font-mono">Fetching documents from FaizDB...</p>
          </div>
        ) : filteredDocs.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-80 gap-3 text-center p-6">
            <div className="w-12 h-12 rounded-xl bg-slate-100 dark:bg-zinc-800 border border-slate-200 dark:border-zinc-700 flex items-center justify-center text-slate-400">
              <TableIcon className="w-6 h-6" />
            </div>
            <div>
              <p className="text-sm font-medium text-slate-900 dark:text-zinc-100">No documents in {collectionName}</p>
              <p className="text-xs text-slate-500 dark:text-zinc-400 max-w-sm mt-1">
                Insert a document or import existing CSV/JSON datasets into FaizDB.
              </p>
            </div>
            <div className="flex items-center gap-2 mt-2">
              <Button variant="outline" size="sm" onClick={() => setIsImportModalOpen(true)}>
                <Upload className="w-3.5 h-3.5" />
                <span>Import Dataset</span>
              </Button>
              <Button variant="primary" size="sm" onClick={onOpenInsertModal}>
                <Plus className="w-3.5 h-3.5" />
                <span>Insert First Document</span>
              </Button>
            </div>
          </div>
        ) : viewMode === 'table' ? (
          <table className="w-full text-left text-xs border-collapse font-mono">
            <thead>
              <tr className="border-b border-slate-200 dark:border-zinc-800 bg-slate-100/80 dark:bg-zinc-950/80 sticky top-0 z-10">
                <th className="py-2.5 px-4 font-semibold text-slate-600 dark:text-zinc-400 w-48">_id</th>
                {allColumns.map((col) => (
                  <th key={col} className="py-2.5 px-4 font-semibold text-slate-600 dark:text-zinc-400 whitespace-nowrap">
                    {col}
                  </th>
                ))}
                <th className="py-2.5 px-4 font-semibold text-slate-600 dark:text-zinc-400 text-right w-24">Actions</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-100 dark:divide-zinc-800/60">
              {filteredDocs.map((doc) => {
                const id = doc._id || '-';
                return (
                  <tr key={id} className="hover:bg-slate-50 dark:hover:bg-zinc-800/40 transition-colors group">
                    <td className="py-2 px-4 font-mono text-emerald-600 dark:text-emerald-400 max-w-[180px] truncate">
                      <div className="flex items-center gap-1.5">
                        <span className="truncate">{id}</span>
                        <button
                          onClick={() => copyId(id)}
                          className="opacity-0 group-hover:opacity-100 text-slate-400 hover:text-slate-600 dark:hover:text-zinc-200 transition-opacity"
                        >
                          {copiedId === id ? <Check className="w-3 h-3 text-emerald-500" /> : <Copy className="w-3 h-3" />}
                        </button>
                      </div>
                    </td>

                    {allColumns.map((col) => {
                      const val = doc[col];
                      let displayVal = '-';
                      let badge = false;

                      if (typeof val === 'boolean') {
                        displayVal = val ? 'true' : 'false';
                        badge = true;
                      } else if (val !== undefined && val !== null) {
                        displayVal = typeof val === 'object' ? JSON.stringify(val) : String(val);
                      }

                      return (
                        <td key={col} className="py-2 px-4 text-slate-700 dark:text-zinc-300 max-w-xs truncate">
                          {badge ? (
                            <Badge variant={val ? 'success' : 'default'} className="text-[10px]">
                              {displayVal}
                            </Badge>
                          ) : (
                            <span className="truncate block">{displayVal}</span>
                          )}
                        </td>
                      );
                    })}

                    <td className="py-2 px-4 text-right whitespace-nowrap">
                      <div className="flex items-center justify-end gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                        <button
                          onClick={() => setSelectedDoc(doc)}
                          className="p-1 rounded hover:bg-slate-200 dark:hover:bg-zinc-700 text-slate-500 dark:text-zinc-400 hover:text-slate-900 dark:hover:text-zinc-100"
                        >
                          <Eye className="w-3.5 h-3.5" />
                        </button>
                        <button
                          onClick={() => onDeleteDocument(id)}
                          className="p-1 rounded hover:bg-rose-100 dark:hover:bg-rose-950/40 text-slate-500 dark:text-zinc-400 hover:text-rose-600 dark:hover:text-rose-400"
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
          <div className="p-4">
            <pre className="p-4 rounded-xl bg-slate-50 dark:bg-zinc-950 border border-slate-200 dark:border-zinc-800 text-xs font-mono text-slate-800 dark:text-emerald-300 overflow-x-auto">
              {JSON.stringify(filteredDocs, null, 2)}
            </pre>
          </div>
        )}
      </div>

      {/* Footer Info */}
      <div className="p-3 border-t border-slate-200 dark:border-zinc-800 bg-white dark:bg-zinc-950 flex items-center justify-between text-xs text-slate-500 dark:text-zinc-400 font-mono">
        <span>Showing {filteredDocs.length} of {documents.length} document(s)</span>
        <span>Collection: {collectionName}</span>
      </div>

      {/* Bulk Import Data Modal */}
      {isImportModalOpen && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/60 backdrop-blur-sm">
          <div className="w-full max-w-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-xl p-5 shadow-2xl space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-slate-200 dark:border-zinc-800">
              <div className="flex items-center gap-2">
                <Upload className="w-4 h-4 text-emerald-500" />
                <h3 className="text-sm font-semibold text-slate-900 dark:text-zinc-100 font-mono">
                  Bulk Import into '{collectionName}'
                </h3>
              </div>
              <Button variant="ghost" size="sm" onClick={() => setIsImportModalOpen(false)}>
                Close
              </Button>
            </div>

            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2 text-xs font-mono">
                <span className="text-zinc-400">Format:</span>
                <button
                  onClick={() => setImportFormat('json')}
                  className={`px-2.5 py-1 rounded text-xs transition cursor-pointer ${
                    importFormat === 'json' ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/40 font-bold' : 'text-zinc-400 bg-zinc-800'
                  }`}
                >
                  JSON Array
                </button>
                <button
                  onClick={() => setImportFormat('csv')}
                  className={`px-2.5 py-1 rounded text-xs transition cursor-pointer ${
                    importFormat === 'csv' ? 'bg-emerald-500/20 text-emerald-400 border border-emerald-500/40 font-bold' : 'text-zinc-400 bg-zinc-800'
                  }`}
                >
                  CSV
                </button>
              </div>

              <label className="flex items-center gap-1.5 px-3 py-1 bg-zinc-800 hover:bg-zinc-700 text-zinc-200 rounded text-xs font-mono cursor-pointer border border-zinc-700 transition">
                <FileText className="w-3.5 h-3.5 text-emerald-400" />
                <span>Upload File</span>
                <input type="file" accept=".csv,.json" onChange={handleFileUpload} className="hidden" />
              </label>
            </div>

            <textarea
              value={importText}
              onChange={(e) => setImportText(e.target.value)}
              rows={8}
              placeholder={
                importFormat === 'json'
                  ? '[\n  { "name": "Faiz", "role": "Architect", "active": true },\n  { "name": "Elena", "role": "Engineer", "active": true }\n]'
                  : 'name,role,country,active\nFaiz,Architect,Malaysia,true\nElena,Engineer,Singapore,true'
              }
              className="w-full p-3 rounded-lg bg-slate-50 dark:bg-zinc-950 border border-slate-200 dark:border-zinc-800 font-mono text-xs text-slate-800 dark:text-zinc-200 focus:ring-1 focus:ring-emerald-500 outline-hidden"
            />

            {importResult && (
              <div
                className={`p-3 rounded-lg text-xs font-mono flex items-center gap-2 ${
                  importResult.error
                    ? 'bg-rose-950/40 border border-rose-800 text-rose-300'
                    : 'bg-emerald-950/40 border border-emerald-800 text-emerald-300'
                }`}
              >
                {importResult.error ? (
                  <>
                    <AlertCircle className="w-4 h-4 text-rose-400 shrink-0" />
                    <span>{importResult.error}</span>
                  </>
                ) : (
                  <>
                    <CheckCircle2 className="w-4 h-4 text-emerald-400 shrink-0" />
                    <span>Successfully imported {importResult.count} records into '{collectionName}'!</span>
                  </>
                )}
              </div>
            )}

            <div className="flex justify-end gap-2 pt-2 border-t border-slate-200 dark:border-zinc-800">
              <Button variant="outline" size="sm" onClick={() => setIsImportModalOpen(false)}>
                Cancel
              </Button>
              <Button
                variant="primary"
                size="sm"
                onClick={handleExecuteImport}
                loading={importLoading}
                disabled={!importText.trim()}
              >
                <Upload className="w-3.5 h-3.5" />
                <span>Import Records</span>
              </Button>
            </div>
          </div>
        </div>
      )}

      {/* Document Inspector Drawer/Modal */}
      {selectedDoc && (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/50 backdrop-blur-sm">
          <div className="w-full max-w-xl bg-white dark:bg-zinc-900 border border-slate-200 dark:border-zinc-800 rounded-xl p-5 shadow-2xl space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-slate-200 dark:border-zinc-800">
              <h3 className="text-sm font-semibold text-slate-900 dark:text-zinc-100 font-mono">
                Document: {selectedDoc._id}
              </h3>
              <Button variant="ghost" size="sm" onClick={() => setSelectedDoc(null)}>
                Close
              </Button>
            </div>
            <div className="max-h-96 overflow-y-auto bg-slate-50 dark:bg-zinc-950 p-3.5 rounded-lg border border-slate-200 dark:border-zinc-800 font-mono text-xs text-slate-800 dark:text-emerald-300">
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
