import { useState, useMemo, useRef, useEffect } from 'react';
import { Modal } from '../ui/Modal';
import { useTagsStore, TagWithCount } from '../../stores/tags';
import { useWikiStore } from '../../stores/wiki';

interface NewWikiModalProps {
  isOpen: boolean;
  onClose: () => void;
}

// Fuzzy matching algorithm
function fuzzyMatch(text: string, query: string): { score: number; matchIndices: number[] } | null {
  const textLower = text.toLowerCase();
  const queryLower = query.toLowerCase();

  if (!queryLower) return { score: 1, matchIndices: [] };

  const matchIndices: number[] = [];
  let queryIndex = 0;
  let score = 0;
  let consecutiveMatches = 0;
  let lastMatchIndex = -1;

  for (let i = 0; i < textLower.length && queryIndex < queryLower.length; i++) {
    if (textLower[i] === queryLower[queryIndex]) {
      matchIndices.push(i);

      if (lastMatchIndex === i - 1) {
        consecutiveMatches++;
        score += consecutiveMatches * 2;
      } else {
        consecutiveMatches = 0;
      }

      if (i === 0) score += 10;
      if (i > 0 && /[\s\-_]/.test(text[i - 1])) score += 5;
      score += 1;

      lastMatchIndex = i;
      queryIndex++;
    }
  }

  if (queryIndex !== queryLower.length) return null;

  score += Math.max(0, 20 - text.length);
  if (textLower === queryLower) score += 50;
  if (textLower.startsWith(queryLower)) score += 25;

  return { score, matchIndices };
}

// Component to highlight matched characters
function HighlightedText({ text, matchIndices }: { text: string; matchIndices: number[] }) {
  if (matchIndices.length === 0) {
    return <>{text}</>;
  }

  const matchSet = new Set(matchIndices);
  const parts: JSX.Element[] = [];

  for (let i = 0; i < text.length; i++) {
    if (matchSet.has(i)) {
      parts.push(
        <span key={i} className="text-[var(--color-accent-light)] font-semibold">
          {text[i]}
        </span>
      );
    } else {
      parts.push(<span key={i}>{text[i]}</span>);
    }
  }

  return <>{parts}</>;
}

interface FlatTag {
  id: string;
  name: string;
  atomCount: number;
}

export function NewWikiModal({ isOpen, onClose }: NewWikiModalProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedTag, setSelectedTag] = useState<FlatTag | null>(null);
  const allTags = useTagsStore(s => s.tags);
  const articles = useWikiStore(s => s.articles);
  const openArticle = useWikiStore(s => s.openArticle);
  const openAndGenerate = useWikiStore(s => s.openAndGenerate);
  const isGenerating = useWikiStore(s => s.isGenerating);
  const inputRef = useRef<HTMLInputElement>(null);

  // Set of tag IDs that already have wiki articles
  const existingArticleTagIds = useMemo(() => new Set(articles.map((a) => a.tag_id)), [articles]);

  // Focus input when modal opens
  useEffect(() => {
    if (isOpen && inputRef.current) {
      inputRef.current.focus();
      setSearchQuery('');
      setSelectedTag(null);
    }
  }, [isOpen]);

  // Flatten tag tree
  const flattenTags = (tags: TagWithCount[]): FlatTag[] => {
    const result: FlatTag[] = [];
    const traverse = (tags: TagWithCount[]) => {
      for (const tag of tags) {
        result.push({ id: tag.id, name: tag.name, atomCount: tag.atom_count });
        if (tag.children?.length > 0) {
          traverse(tag.children);
        }
      }
    };
    traverse(tags);
    return result;
  };

  const allFlatTags = useMemo(() => flattenTags(allTags), [allTags]);

  // Default category tags to exclude from wiki generation
  const defaultCategoryTags = new Set(['Topics', 'People', 'Locations', 'Organizations', 'Events']);

  // Fuzzy filter and sort tags
  const filteredTags = useMemo(() => {
    // Only show tags with atoms, excluding default categories, sorted by atom count descending
    const tagsWithAtoms = allFlatTags
      .filter((tag) => tag.atomCount > 0 && !defaultCategoryTags.has(tag.name))
      .sort((a, b) => b.atomCount - a.atomCount);

    if (!searchQuery.trim()) {
      return tagsWithAtoms.slice(0, 20).map((tag) => ({ tag, matchIndices: [] as number[] }));
    }

    const matches: { tag: FlatTag; score: number; matchIndices: number[] }[] = [];

    for (const tag of tagsWithAtoms) {
      const match = fuzzyMatch(tag.name, searchQuery);
      if (match) {
        matches.push({ tag, score: match.score, matchIndices: match.matchIndices });
      }
    }

    // Sort by fuzzy match score, then by atom count for ties
    matches.sort((a, b) => {
      if (b.score !== a.score) return b.score - a.score;
      return b.tag.atomCount - a.tag.atomCount;
    });
    return matches.slice(0, 20);
  }, [allFlatTags, searchQuery]);

  const handleSelectTag = (tag: FlatTag) => {
    setSelectedTag(tag);
    setSearchQuery(tag.name);
  };

  const handleGenerate = () => {
    if (!selectedTag) return;

    // Open article view and start generation
    openAndGenerate(selectedTag.id, selectedTag.name);
    onClose();
  };

  const handleViewExisting = () => {
    if (!selectedTag) return;
    const article = articles.find((a) => a.tag_id === selectedTag.id);
    if (article) {
      openArticle(article.tag_id, article.tag_name);
      onClose();
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Escape') {
      onClose();
    } else if (e.key === 'Enter' && filteredTags.length > 0 && !selectedTag) {
      e.preventDefault();
      handleSelectTag(filteredTags[0].tag);
    }
  };

  const selectedTagHasArticle = selectedTag ? existingArticleTagIds.has(selectedTag.id) : false;

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="New Wiki Page"
      showFooter={false}
    >
      <div className="space-y-4">
        {/* Search input */}
        <div>
          <label className="block text-sm font-medium text-[var(--color-text-primary)] mb-2">
            Select a tag
          </label>
          <input
            ref={inputRef}
            type="text"
            value={searchQuery}
            onChange={(e) => {
              setSearchQuery(e.target.value);
              setSelectedTag(null); // Clear selection when typing
            }}
            onKeyDown={handleKeyDown}
            placeholder="Search tags..."
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            className="w-full bg-[var(--color-bg-main)] border border-[var(--color-border)] rounded-lg px-3 py-2 text-[var(--color-text-primary)] focus:outline-none focus:border-[var(--color-accent)] placeholder-[var(--color-text-tertiary)]"
          />
        </div>

        {/* Tag list or selection result */}
        {!selectedTag ? (
          <div className="max-h-64 overflow-y-auto border border-[var(--color-border)] rounded-lg">
            {filteredTags.length > 0 ? (
              filteredTags.map(({ tag, matchIndices }) => {
                const hasArticle = existingArticleTagIds.has(tag.id);
                return (
                  <button
                    key={tag.id}
                    onClick={() => handleSelectTag(tag)}
                    className="w-full px-3 py-2 text-left hover:bg-[var(--color-bg-hover)] transition-colors flex items-center justify-between"
                  >
                    <span className="text-sm text-[var(--color-text-primary)]">
                      <HighlightedText text={tag.name} matchIndices={matchIndices} />
                    </span>
                    <span className="flex items-center gap-2 text-xs text-[var(--color-text-tertiary)]">
                      {hasArticle && (
                        <span className="px-1.5 py-0.5 rounded bg-[var(--color-accent)]/20 text-[var(--color-accent-light)]">
                          Has article
                        </span>
                      )}
                      <span>{tag.atomCount} atoms</span>
                    </span>
                  </button>
                );
              })
            ) : (
              <div className="px-3 py-4 text-sm text-[var(--color-text-secondary)] text-center">
                No matching tags with atoms found
              </div>
            )}
          </div>
        ) : (
          <div className="border border-[var(--color-border)] rounded-lg p-4">
            {selectedTagHasArticle ? (
              // Tag already has an article
              <div className="space-y-3">
                <div className="flex items-center gap-2 text-amber-500">
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  </svg>
                  <span className="font-medium">Article already exists</span>
                </div>
                <p className="text-sm text-[var(--color-text-secondary)]">
                  A wiki article for "{selectedTag.name}" has already been generated. You can view it or choose a different tag.
                </p>
                <div className="flex gap-2">
                  <button
                    onClick={handleViewExisting}
                    className="flex-1 px-4 py-2 bg-[var(--color-accent)] hover:bg-[var(--color-accent-hover)] text-white rounded-lg transition-colors"
                  >
                    View Article
                  </button>
                  <button
                    onClick={() => {
                      setSelectedTag(null);
                      setSearchQuery('');
                    }}
                    className="px-4 py-2 border border-[var(--color-border)] text-[var(--color-text-primary)] rounded-lg hover:bg-[var(--color-bg-hover)] transition-colors"
                  >
                    Choose Different Tag
                  </button>
                </div>
              </div>
            ) : (
              // Tag doesn't have an article - show generate option
              <div className="space-y-3">
                <div className="flex items-center justify-between">
                  <span className="font-medium text-[var(--color-text-primary)]">{selectedTag.name}</span>
                  <span className="text-xs text-[var(--color-text-tertiary)]">{selectedTag.atomCount} atoms</span>
                </div>
                <p className="text-sm text-[var(--color-text-secondary)]">
                  Generate a wiki article synthesizing knowledge from atoms tagged with "{selectedTag.name}".
                </p>
                <div className="flex gap-2">
                  <button
                    onClick={handleGenerate}
                    disabled={isGenerating}
                    className="flex-1 flex items-center justify-center gap-2 px-4 py-2 bg-[var(--color-accent)] hover:bg-[var(--color-accent-hover)] text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {isGenerating ? (
                      <>
                        <svg className="w-4 h-4 animate-spin" fill="none" viewBox="0 0 24 24">
                          <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                          <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                        </svg>
                        Generating...
                      </>
                    ) : (
                      'Generate Article'
                    )}
                  </button>
                  <button
                    onClick={() => {
                      setSelectedTag(null);
                      setSearchQuery('');
                    }}
                    disabled={isGenerating}
                    className="px-4 py-2 border border-[var(--color-border)] text-[var(--color-text-primary)] rounded-lg hover:bg-[var(--color-bg-hover)] transition-colors disabled:opacity-50"
                  >
                    Back
                  </button>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}
