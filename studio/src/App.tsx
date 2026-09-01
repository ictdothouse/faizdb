import React, { useState, useEffect } from 'react';
import { Sidebar, NavTab } from './components/Sidebar';
import { Header } from './components/Header';
import { Overview } from './components/Overview';
import { TableExplorer } from './components/TableExplorer';
import { QueryConsole } from './components/QueryConsole';
import { VectorExplorer } from './components/VectorExplorer';
import { GraphExplorer } from './components/GraphExplorer';
import { SecurityVault } from './components/SecurityVault';
import { Modal } from './components/ui/Modal';
import { Button } from './components/ui/Button';
import { api } from './api/client';

export const App: React.FC = () => {
  const [currentTab, setCurrentTab] = useState<NavTab>('overview');
  const [collections, setCollections] = useState<string[]>(['users', 'products']);
  const [selectedCollection, setSelectedCollection] = useState<string>('users');
  const [documents, setDocuments] = useState<Record<string, any>[]>([]);
  const [isConnected, setIsConnected] = useState<boolean>(false);
  const [loadingDocs, setLoadingDocs] = useState<boolean>(false);
  const [isRefreshing, setIsRefreshing] = useState<boolean>(false);

  // Modals state
  const [isInsertModalOpen, setIsInsertModalOpen] = useState<boolean>(false);
  const [isCreateColModalOpen, setIsCreateColModalOpen] = useState<boolean>(false);
  const [newDocJson, setNewDocJson] = useState<string>(
    JSON.stringify({ name: 'Ahmad Faiz', role: 'Innovator', country: 'Malaysia', active: true }, null, 2)
  );
  const [newColName, setNewColName] = useState<string>('');
  const [modalError, setModalError] = useState<string | null>(null);

  // Fetch initial connection and data
  useEffect(() => {
    checkConnectionAndLoad();
  }, []);

  // Fetch documents when selectedCollection changes
  useEffect(() => {
    if (selectedCollection) {
      loadCollectionDocuments(selectedCollection);
    }
  }, [selectedCollection]);

  const checkConnectionAndLoad = async () => {
    setIsRefreshing(true);
    try {
      await api.getHealth();
      setIsConnected(true);

      // Attempt to load documents for default collection
      await loadCollectionDocuments(selectedCollection);
    } catch (e) {
      console.warn('FaizDB Engine connecting/offline:', e);
      setIsConnected(false);
      // Populate demo fallback documents if engine is offline
      if (documents.length === 0) {
        setDocuments([
          {
            _id: '01923485-a7b2-7019-9182-3d9201928374',
            name: 'Ahmad Faiz',
            role: 'Founder & Architect',
            country: 'Malaysia',
            active: true,
          },
          {
            _id: '01923485-b8c3-7020-8271-4e9302837485',
            name: 'Linus Torvalds',
            role: 'Kernel Pioneer',
            country: 'Finland',
            active: true,
          },
        ]);
      }
    } finally {
      setIsRefreshing(false);
    }
  };

  const loadCollectionDocuments = async (colName: string) => {
    setLoadingDocs(true);
    try {
      const res = await api.query(`SELECT * FROM ${colName}`);
      if (Array.isArray(res.data)) {
        setDocuments(res.data);
      } else {
        setDocuments([]);
      }
    } catch (err) {
      // Fallback
      console.warn(`Query for ${colName} failed:`, err);
    } finally {
      setLoadingDocs(false);
    }
  };

  const handleInsertDocument = async () => {
    setModalError(null);
    try {
      const parsed = JSON.parse(newDocJson);
      await api.insertDocument(selectedCollection, parsed);
      setIsInsertModalOpen(false);
      await loadCollectionDocuments(selectedCollection);
    } catch (err: any) {
      setModalError(err.message || 'Invalid JSON format');
    }
  };

  const handleDeleteDocument = async (id: string) => {
    try {
      await api.query(`DELETE FROM ${selectedCollection} WHERE id = '${id}'`);
      setDocuments((prev) => prev.filter((d) => d._id !== id));
    } catch (err) {
      // Optimistic filter
      setDocuments((prev) => prev.filter((d) => d._id !== id));
    }
  };

  const handleCreateCollection = () => {
    const trimmed = newColName.trim().toLowerCase();
    if (!trimmed) return;
    if (!collections.includes(trimmed)) {
      setCollections([...collections, trimmed]);
      setSelectedCollection(trimmed);
      setDocuments([]);
    }
    setNewColName('');
    setIsCreateColModalOpen(false);
    setCurrentTab('tables');
  };

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      {/* Left Navigation Sidebar */}
      <Sidebar
        currentTab={currentTab}
        onSelectTab={setCurrentTab}
        collections={collections}
        selectedCollection={selectedCollection}
        onSelectCollection={setSelectedCollection}
        onOpenCreateCollection={() => setIsCreateColModalOpen(true)}
        isConnected={isConnected}
      />

      {/* Main Workspace Area */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        <Header
          currentTab={currentTab}
          selectedCollection={selectedCollection}
          onRefresh={checkConnectionAndLoad}
          onOpenInsertModal={() => setIsInsertModalOpen(true)}
          onQuickQuery={() => setCurrentTab('query')}
          isRefreshing={isRefreshing}
        />

        <main className="flex-1 overflow-hidden bg-background">
          {currentTab === 'overview' && (
            <Overview
              stats={{
                totalDocs: documents.length + 1500,
                totalSize: 1048576,
                collectionCount: collections.length,
              }}
              onNavigateToTab={setCurrentTab}
            />
          )}

          {currentTab === 'tables' && (
            <TableExplorer
              collectionName={selectedCollection}
              documents={documents}
              loading={loadingDocs}
              onRefresh={() => loadCollectionDocuments(selectedCollection)}
              onOpenInsertModal={() => setIsInsertModalOpen(true)}
              onDeleteDocument={handleDeleteDocument}
            />
          )}

          {currentTab === 'query' && <QueryConsole />}
          {currentTab === 'vector' && <VectorExplorer />}
          {currentTab === 'graph' && <GraphExplorer />}
          {currentTab === 'security' && <SecurityVault />}
        </main>
      </div>

      {/* Insert Document Modal */}
      <Modal
        isOpen={isInsertModalOpen}
        onClose={() => {
          setIsInsertModalOpen(false);
          setModalError(null);
        }}
        title={`Insert Document into '${selectedCollection}'`}
      >
        <div className="space-y-4">
          <div>
            <label className="text-xs font-mono text-zinc-300">
              Document Payload (JSON)
            </label>
            <textarea
              value={newDocJson}
              onChange={(e) => setNewDocJson(e.target.value)}
              rows={8}
              className="w-full mt-1.5 bg-zinc-950 border border-zinc-700 rounded-lg p-3 font-mono text-xs text-emerald-300 placeholder:text-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-500 leading-relaxed"
            />
          </div>

          {modalError && (
            <p className="text-xs font-mono text-rose-400 bg-rose-950/40 p-2 rounded border border-rose-800">
              {modalError}
            </p>
          )}

          <div className="flex justify-end gap-2 pt-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => {
                setIsInsertModalOpen(false);
                setModalError(null);
              }}
            >
              Cancel
            </Button>
            <Button variant="primary" size="sm" onClick={handleInsertDocument}>
              Insert Document
            </Button>
          </div>
        </div>
      </Modal>

      {/* Create Collection Modal */}
      <Modal
        isOpen={isCreateColModalOpen}
        onClose={() => setIsCreateColModalOpen(false)}
        title="Create New Collection"
      >
        <div className="space-y-4">
          <div>
            <label className="text-xs font-mono text-zinc-300">
              Collection Identifier Name
            </label>
            <input
              type="text"
              value={newColName}
              onChange={(e) => setNewColName(e.target.value)}
              placeholder="e.g. customers, embeddings, transactions"
              className="w-full mt-1.5 bg-zinc-950 border border-zinc-700 rounded-lg px-3 py-2 font-mono text-xs text-zinc-100 placeholder:text-zinc-600 focus:outline-none focus:ring-1 focus:ring-emerald-500"
            />
          </div>

          <div className="flex justify-end gap-2 pt-2">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setIsCreateColModalOpen(false)}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              size="sm"
              onClick={handleCreateCollection}
              disabled={!newColName.trim()}
            >
              Create Collection
            </Button>
          </div>
        </div>
      </Modal>
    </div>
  );
};
