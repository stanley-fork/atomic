import { useEffect } from 'react';
import { useWikiStore } from '../../stores/wiki';
import { useUIStore } from '../../stores/ui';
import { WikiHeader } from './WikiHeader';
import { WikiEmptyState } from './WikiEmptyState';
import { WikiGenerating } from './WikiGenerating';
import { WikiArticleContent } from './WikiArticleContent';

interface WikiViewerProps {
  tagId: string;
  tagName: string;
}

export function WikiViewer({ tagId, tagName }: WikiViewerProps) {
  const currentArticle = useWikiStore(s => s.currentArticle);
  const articleStatus = useWikiStore(s => s.articleStatus);
  const isLoading = useWikiStore(s => s.isLoading);
  const isGenerating = useWikiStore(s => s.isGenerating);
  const isUpdating = useWikiStore(s => s.isUpdating);
  const error = useWikiStore(s => s.error);
  const fetchArticle = useWikiStore(s => s.fetchArticle);
  const fetchArticleStatus = useWikiStore(s => s.fetchArticleStatus);
  const generateArticle = useWikiStore(s => s.generateArticle);
  const updateArticle = useWikiStore(s => s.updateArticle);
  const clearArticle = useWikiStore(s => s.clearArticle);
  const clearError = useWikiStore(s => s.clearError);

  const closeDrawer = useUIStore(s => s.closeDrawer);
  const openDrawer = useUIStore(s => s.openDrawer);

  // Fetch article and status when component mounts or tagId changes
  useEffect(() => {
    fetchArticle(tagId);
    fetchArticleStatus(tagId);

    // Cleanup when unmounting
    return () => {
      clearArticle();
    };
  }, [tagId, fetchArticle, fetchArticleStatus, clearArticle]);

  const handleGenerate = () => {
    generateArticle(tagId, tagName);
  };

  const handleUpdate = () => {
    updateArticle(tagId, tagName);
  };

  const handleRegenerate = () => {
    generateArticle(tagId, tagName);
  };

  const handleViewAtom = (atomId: string) => {
    openDrawer('viewer', atomId);
  };

  // Loading state
  if (isLoading) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">{tagName}</h2>
          <button
            onClick={closeDrawer}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="flex-1 flex items-center justify-center">
          <div className="w-8 h-8 animate-spin">
            <svg className="w-full h-full text-[var(--color-accent)]" fill="none" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="3" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
            </svg>
          </div>
        </div>
      </div>
    );
  }

  // Error state
  if (error) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">{tagName}</h2>
          <button
            onClick={closeDrawer}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div className="flex-1 flex flex-col items-center justify-center px-6 text-center">
          <div className="w-12 h-12 mb-4 rounded-full bg-red-500/10 flex items-center justify-center">
            <svg className="w-6 h-6 text-red-500" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <p className="text-[var(--color-text-primary)] mb-2">Failed to generate article</p>
          <p className="text-sm text-[var(--color-text-secondary)] mb-4">{error}</p>
          <button
            onClick={() => {
              clearError();
              handleGenerate();
            }}
            className="px-4 py-2 bg-[var(--color-accent)] text-white rounded-lg hover:bg-[var(--color-accent-hover)] transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  // Generating state
  if (isGenerating) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">{tagName}</h2>
          <button
            onClick={closeDrawer}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <WikiGenerating tagName={tagName} atomCount={articleStatus?.current_atom_count || 0} />
      </div>
    );
  }

  // Empty state (no article exists)
  if (!currentArticle) {
    return (
      <div className="flex flex-col h-full">
        <div className="flex items-center justify-between px-6 py-4 border-b border-[var(--color-border)]">
          <h2 className="text-lg font-semibold text-[var(--color-text-primary)]">{tagName}</h2>
          <button
            onClick={closeDrawer}
            className="text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] transition-colors"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
        <WikiEmptyState
          tagName={tagName}
          atomCount={articleStatus?.current_atom_count || 0}
          onGenerate={handleGenerate}
          isGenerating={isGenerating}
        />
      </div>
    );
  }

  // Article exists - show content
  return (
    <div className="flex flex-col h-full">
      <WikiHeader
        tagName={tagName}
        updatedAt={currentArticle.article.updated_at}
        sourceCount={currentArticle.citations.length}
        newAtomsAvailable={articleStatus?.new_atoms_available || 0}
        onUpdate={handleUpdate}
        onRegenerate={handleRegenerate}
        onClose={closeDrawer}
        isUpdating={isUpdating}
      />
      <div className="flex-1 overflow-y-auto">
        <WikiArticleContent
          article={currentArticle.article}
          citations={currentArticle.citations}
          onViewAtom={handleViewAtom}
        />
      </div>
    </div>
  );
}

