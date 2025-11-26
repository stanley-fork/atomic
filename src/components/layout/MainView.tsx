import { useMemo } from 'react';
import { AtomGrid } from '../atoms/AtomGrid';
import { AtomList } from '../atoms/AtomList';
import { CanvasView } from '../canvas/CanvasView';
import { FAB } from '../ui/FAB';
import { SemanticSearch } from '../search/SemanticSearch';
import { useAtomsStore, SemanticSearchResult } from '../../stores/atoms';
import { useUIStore } from '../../stores/ui';

export function MainView() {
  const {
    atoms,
    semanticSearchResults,
    semanticSearchQuery,
    retryEmbedding,
  } = useAtomsStore();
  const { viewMode, setViewMode, searchQuery, selectedTagId, openDrawer } = useUIStore();

  // Determine what to display
  const displayAtoms = useMemo(() => {
    // If semantic search is active, use those results
    if (semanticSearchResults !== null) {
      return semanticSearchResults;
    }

    // Otherwise, filter by text search
    if (!searchQuery.trim()) return atoms;
    const query = searchQuery.toLowerCase();
    return atoms.filter(
      (atom) =>
        atom.content.toLowerCase().includes(query) ||
        atom.tags.some((tag) => tag.name.toLowerCase().includes(query))
    );
  }, [atoms, searchQuery, semanticSearchResults]);

  // Check if we're showing semantic search results
  const isSemanticSearch = semanticSearchResults !== null;

  // Get search result IDs for canvas view
  const searchResultIds = useMemo(() => {
    if (!isSemanticSearch) return null;
    return semanticSearchResults.map((r) => r.id);
  }, [isSemanticSearch, semanticSearchResults]);

  // Get matching chunk content for semantic search results
  const getMatchingChunkContent = (atomId: string): string | undefined => {
    if (!isSemanticSearch) return undefined;
    const result = semanticSearchResults.find((r) => r.id === atomId) as
      | SemanticSearchResult
      | undefined;
    return result?.matching_chunk_content;
  };

  const handleAtomClick = (atomId: string) => {
    openDrawer('viewer', atomId);
  };

  const handleNewAtom = () => {
    openDrawer('editor');
  };

  const handleRetryEmbedding = async (atomId: string) => {
    try {
      await retryEmbedding(atomId);
    } catch (error) {
      console.error('Failed to retry embedding:', error);
    }
  };

  return (
    <main className="flex-1 flex flex-col h-full bg-[#1e1e1e] overflow-hidden">
      {/* Header */}
      <header className="flex items-center gap-4 px-4 py-3 border-b border-[#3d3d3d]">
        {/* Semantic Search */}
        <SemanticSearch />

        {/* View Mode Toggle */}
        <div className="flex items-center bg-[#2d2d2d] rounded-md border border-[#3d3d3d]">
          <button
            onClick={() => setViewMode('canvas')}
            className={`px-3 py-1.5 text-sm rounded-l-md transition-colors ${
              viewMode === 'canvas'
                ? 'bg-[#7c3aed] text-white'
                : 'text-[#888888] hover:text-[#dcddde]'
            }`}
            title="Canvas view"
          >
            Canvas
          </button>
          <button
            onClick={() => setViewMode('grid')}
            className={`p-2 transition-colors ${
              viewMode === 'grid'
                ? 'bg-[#7c3aed] text-white'
                : 'text-[#888888] hover:text-[#dcddde]'
            }`}
            title="Grid view"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M4 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2V6zM14 6a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V6zM4 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zM14 16a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z"
              />
            </svg>
          </button>
          <button
            onClick={() => setViewMode('list')}
            className={`p-2 rounded-r-md transition-colors ${
              viewMode === 'list'
                ? 'bg-[#7c3aed] text-white'
                : 'text-[#888888] hover:text-[#dcddde]'
            }`}
            title="List view"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M4 6h16M4 12h16M4 18h16"
              />
            </svg>
          </button>
        </div>

        {/* Atom count - only show for grid/list views */}
        {viewMode !== 'canvas' && (
          <span className="text-sm text-[#888888]">
            {displayAtoms.length} atom{displayAtoms.length !== 1 ? 's' : ''}
          </span>
        )}
      </header>

      {/* Search results header - only show for grid/list views */}
      {isSemanticSearch && viewMode !== 'canvas' && (
        <div className="px-4 py-2 text-sm text-[#888888] border-b border-[#3d3d3d]">
          {semanticSearchResults.length > 0 ? (
            <span>
              {semanticSearchResults.length} results for "{semanticSearchQuery}"
            </span>
          ) : (
            <span>No atoms match your search</span>
          )}
        </div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-hidden">
        {viewMode === 'canvas' ? (
          <CanvasView
            atoms={atoms}
            selectedTagId={selectedTagId}
            searchResultIds={searchResultIds}
            onAtomClick={handleAtomClick}
          />
        ) : viewMode === 'grid' ? (
          <div className="h-full overflow-y-auto">
            <AtomGrid
              atoms={displayAtoms}
              onAtomClick={handleAtomClick}
              getMatchingChunkContent={isSemanticSearch ? getMatchingChunkContent : undefined}
              onRetryEmbedding={handleRetryEmbedding}
            />
          </div>
        ) : (
          <div className="h-full overflow-y-auto">
            <AtomList
              atoms={displayAtoms}
              onAtomClick={handleAtomClick}
              getMatchingChunkContent={isSemanticSearch ? getMatchingChunkContent : undefined}
              onRetryEmbedding={handleRetryEmbedding}
            />
          </div>
        )}
      </div>

      {/* FAB */}
      <FAB onClick={handleNewAtom} title="Create new atom" />
    </main>
  );
}

