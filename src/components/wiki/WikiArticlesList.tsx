import { useState } from 'react';
import { useWikiStore } from '../../stores/wiki';
import { WikiArticleCard } from './WikiArticleCard';
import { NewWikiModal } from './NewWikiModal';

export function WikiArticlesList() {
  const articles = useWikiStore(s => s.articles);
  const suggestedArticles = useWikiStore(s => s.suggestedArticles);
  const isLoadingList = useWikiStore(s => s.isLoadingList);
  const error = useWikiStore(s => s.error);
  const openArticle = useWikiStore(s => s.openArticle);
  const openAndGenerate = useWikiStore(s => s.openAndGenerate);
  const [isModalOpen, setIsModalOpen] = useState(false);

  if (isLoadingList && articles.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-[var(--color-text-secondary)]">
        Loading wiki articles...
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4 p-4">
        <p className="text-red-400">{error}</p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      {/* New Wiki Button */}
      <div className="flex-shrink-0 p-4 border-b border-[var(--color-border)]">
        <button
          onClick={() => setIsModalOpen(true)}
          className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-[var(--color-accent)] hover:bg-[var(--color-accent-hover)] text-white rounded-lg transition-colors"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          New Wiki Page
        </button>
      </div>

      {/* Scrollable content: articles + suggestions */}
      <div className="flex-1 overflow-y-auto">
        {articles.length === 0 && suggestedArticles.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full gap-4 p-8 text-center">
            <div className="w-16 h-16 rounded-full bg-[var(--color-bg-card)] flex items-center justify-center">
              <svg className="w-8 h-8 text-[var(--color-text-secondary)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
              </svg>
            </div>
            <div>
              <p className="text-[var(--color-text-primary)] font-medium mb-1">No wiki articles yet</p>
              <p className="text-[var(--color-text-secondary)] text-sm">
                Generate a wiki article from your atoms to synthesize knowledge
              </p>
            </div>
          </div>
        ) : (
          <>
            {/* Existing Articles */}
            {articles.length > 0 && (
              <div className="divide-y divide-[var(--color-border)]">
                {articles.map((article) => (
                  <WikiArticleCard
                    key={article.id}
                    article={article}
                    onClick={() => openArticle(article.tag_id, article.tag_name)}
                  />
                ))}
              </div>
            )}

            {/* Suggested Articles */}
            {suggestedArticles.length > 0 && (
              <div className="border-t border-[var(--color-border)]">
                <div className="px-4 pt-3 pb-1">
                  <h3 className="text-[10px] font-medium text-[var(--color-text-tertiary)] uppercase tracking-wider">
                    Suggested Articles
                  </h3>
                </div>
                <div className="divide-y divide-[var(--color-border)]">
                  {suggestedArticles.map((suggestion) => (
                    <button
                      key={suggestion.tag_id}
                      onClick={() => openAndGenerate(suggestion.tag_id, suggestion.tag_name)}
                      className="w-full group flex items-center justify-between px-4 py-2.5 hover:bg-[var(--color-bg-elevated)] transition-colors text-left"
                    >
                      <div className="min-w-0 flex-1">
                        <span className="text-sm text-[var(--color-text-primary)]">
                          {suggestion.tag_name}
                        </span>
                        <div className="flex items-center gap-2 mt-0.5">
                          <span className="text-[11px] text-[var(--color-text-tertiary)]">
                            {suggestion.atom_count} atom{suggestion.atom_count !== 1 ? 's' : ''}
                          </span>
                          {suggestion.mention_count > 0 && (
                            <span className="text-[11px] text-[var(--color-text-tertiary)]">
                              {suggestion.mention_count} mention{suggestion.mention_count !== 1 ? 's' : ''}
                            </span>
                          )}
                        </div>
                      </div>
                      <div className="flex-shrink-0 opacity-0 group-hover:opacity-100 transition-opacity ml-2">
                        <svg className="w-4 h-4 text-[var(--color-accent)]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                        </svg>
                      </div>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* New Wiki Modal */}
      <NewWikiModal isOpen={isModalOpen} onClose={() => setIsModalOpen(false)} />
    </div>
  );
}
