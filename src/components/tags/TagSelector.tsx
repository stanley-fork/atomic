import { useState, useMemo } from 'react';
import { TagChip } from './TagChip';
import { Input } from '../ui/Input';
import { useTagsStore, TagWithCount } from '../../stores/tags';
import { Tag } from '../../stores/atoms';

interface TagSelectorProps {
  selectedTags: Tag[];
  onTagsChange: (tags: Tag[]) => void;
}

// Fuzzy match result with score and match indices
interface FuzzyMatch {
  tag: Tag;
  score: number;
  matchIndices: number[];
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

      // Bonus for consecutive matches
      if (lastMatchIndex === i - 1) {
        consecutiveMatches++;
        score += consecutiveMatches * 2;
      } else {
        consecutiveMatches = 0;
      }

      // Bonus for matching at start
      if (i === 0) score += 10;

      // Bonus for matching after separator (space, -, _)
      if (i > 0 && /[\s\-_]/.test(text[i - 1])) score += 5;

      // Base score for match
      score += 1;

      lastMatchIndex = i;
      queryIndex++;
    }
  }

  // All query characters must be found
  if (queryIndex !== queryLower.length) return null;

  // Bonus for shorter strings (more relevant matches)
  score += Math.max(0, 20 - text.length);

  // Bonus for exact match
  if (textLower === queryLower) score += 50;

  // Bonus for starts with query
  if (textLower.startsWith(queryLower)) score += 25;

  return { score, matchIndices };
}

// Component to highlight matched characters in fuzzy search results
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

export function TagSelector({ selectedTags, onTagsChange }: TagSelectorProps) {
  const tags = useTagsStore(s => s.tags);
  const createTag = useTagsStore(s => s.createTag);
  const [inputValue, setInputValue] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [showAllSelectedTags, setShowAllSelectedTags] = useState(false);

  const MAX_VISIBLE_TAGS = 5;
  const visibleSelectedTags = showAllSelectedTags
    ? selectedTags
    : selectedTags.slice(0, MAX_VISIBLE_TAGS);
  const hiddenSelectedCount = selectedTags.length - MAX_VISIBLE_TAGS;

  // Flatten the tag tree for searching
  const flattenTags = (tags: TagWithCount[]): Tag[] => {
    return tags.reduce<Tag[]>((acc, tag) => {
      acc.push({
        id: tag.id,
        name: tag.name,
        parent_id: tag.parent_id,
        created_at: tag.created_at,
      });
      if (tag.children) {
        acc.push(...flattenTags(tag.children));
      }
      return acc;
    }, []);
  };

  const allTags = flattenTags(tags);
  const selectedTagIds = new Set(selectedTags.map((t) => t.id));

  // Fuzzy filter and sort tags based on input
  const filteredTagsWithMatches = useMemo(() => {
    if (!inputValue.trim()) return [];

    const matches: FuzzyMatch[] = [];

    for (const tag of allTags) {
      if (selectedTagIds.has(tag.id)) continue;

      const match = fuzzyMatch(tag.name, inputValue);
      if (match) {
        matches.push({ tag, score: match.score, matchIndices: match.matchIndices });
      }
    }

    // Sort by score descending
    matches.sort((a, b) => b.score - a.score);

    return matches;
  }, [allTags, inputValue, selectedTagIds]);

  const filteredTags = filteredTagsWithMatches.map(m => m.tag);
  const matchIndicesMap = new Map(filteredTagsWithMatches.map(m => [m.tag.id, m.matchIndices]));

  const handleAddTag = (tag: Tag) => {
    onTagsChange([...selectedTags, tag]);
    setInputValue('');
  };

  const handleRemoveTag = (tagId: string) => {
    onTagsChange(selectedTags.filter((t) => t.id !== tagId));
  };

  const handleCreateTag = async () => {
    if (!inputValue.trim() || isCreating) return;
    
    setIsCreating(true);
    try {
      const newTag = await createTag(inputValue.trim());
      onTagsChange([...selectedTags, newTag]);
      setInputValue('');
    } catch (error) {
      console.error('Failed to create tag:', error);
    } finally {
      setIsCreating(false);
    }
  };

  const showCreateOption =
    inputValue.trim() &&
    !allTags.some((t) => t.name.toLowerCase() === inputValue.toLowerCase());

  return (
    <div className="space-y-2">
      <label className="block text-sm font-medium text-[var(--color-text-primary)]">Tags</label>
      
      {/* Selected tags */}
      {selectedTags.length > 0 && (
        <div className="space-y-1 mb-2">
          <div className="flex flex-wrap gap-1.5">
            {visibleSelectedTags.map((tag) => (
              <TagChip
                key={tag.id}
                name={tag.name}
                size="md"
                onRemove={() => handleRemoveTag(tag.id)}
              />
            ))}
            {!showAllSelectedTags && hiddenSelectedCount > 0 && (
              <button
                onClick={() => setShowAllSelectedTags(true)}
                className="text-sm text-[var(--color-text-secondary)] hover:text-[var(--color-accent-light)] transition-colors px-2"
              >
                +{hiddenSelectedCount} more
              </button>
            )}
          </div>
          {showAllSelectedTags && selectedTags.length > MAX_VISIBLE_TAGS && (
            <button
              onClick={() => setShowAllSelectedTags(false)}
              className="text-sm text-[var(--color-text-secondary)] hover:text-[var(--color-accent-light)] transition-colors"
            >
              Show less
            </button>
          )}
        </div>
      )}

      {/* Input */}
      <div className="relative">
        <Input
          value={inputValue}
          onChange={(e) => setInputValue(e.target.value)}
          placeholder="Search or create tags..."
          onKeyDown={(e) => {
            if (e.key === 'Enter' && showCreateOption) {
              e.preventDefault();
              handleCreateTag();
            }
          }}
        />

        {/* Dropdown */}
        {inputValue && (filteredTags.length > 0 || showCreateOption) && (
          <div className="absolute z-10 w-full mt-1 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md shadow-lg max-h-48 overflow-y-auto">
            {filteredTags.map((tag) => {
              const matchIndices = matchIndicesMap.get(tag.id) || [];
              return (
                <button
                  key={tag.id}
                  onClick={() => handleAddTag(tag)}
                  className="w-full px-3 py-2 text-left text-sm text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] transition-colors"
                >
                  <HighlightedText text={tag.name} matchIndices={matchIndices} />
                </button>
              );
            })}
            {showCreateOption && (
              <button
                onClick={handleCreateTag}
                disabled={isCreating}
                className="w-full px-3 py-2 text-left text-sm text-[var(--color-accent)] hover:bg-[var(--color-bg-hover)] transition-colors flex items-center gap-2"
              >
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
                Create "{inputValue}"
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

