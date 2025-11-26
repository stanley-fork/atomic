import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { SimilarAtomResult } from '../../stores/atoms';

interface RelatedAtomsProps {
  atomId: string;
  onAtomClick: (atomId: string) => void;
}

export function RelatedAtoms({ atomId, onAtomClick }: RelatedAtomsProps) {
  const [relatedAtoms, setRelatedAtoms] = useState<SimilarAtomResult[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isCollapsed, setIsCollapsed] = useState(true);
  const [hasLoaded, setHasLoaded] = useState(false);

  useEffect(() => {
    // Only fetch when expanded and not yet loaded
    if (!isCollapsed && !hasLoaded) {
      const fetchRelated = async () => {
        setIsLoading(true);
        try {
          const results = await invoke<SimilarAtomResult[]>('find_similar_atoms', {
            atomId,
            limit: 5,
            threshold: 0.7,
          });
          setRelatedAtoms(results);
          setHasLoaded(true);
        } catch (error) {
          console.error('Failed to fetch related atoms:', error);
          setRelatedAtoms([]);
        } finally {
          setIsLoading(false);
        }
      };

      fetchRelated();
    }
  }, [atomId, isCollapsed, hasLoaded]);

  // Don't render if loaded and no related atoms found
  if (hasLoaded && !isLoading && relatedAtoms.length === 0) {
    return null;
  }

  return (
    <div className="border-t border-[#3d3d3d] px-6 py-4">
      <button
        onClick={() => setIsCollapsed(!isCollapsed)}
        className="flex items-center justify-between w-full text-sm font-medium text-[#888888] hover:text-[#dcddde] transition-colors"
      >
        <div className="flex items-center gap-2">
          <span>Related Atoms</span>
          {hasLoaded && relatedAtoms.length > 0 && (
            <span className="text-xs text-[#666666] bg-[#2d2d2d] px-2 py-0.5 rounded">
              {relatedAtoms.length}
            </span>
          )}
        </div>
        <svg
          className={`w-4 h-4 transition-transform ${isCollapsed ? '' : 'rotate-180'}`}
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
        </svg>
      </button>

      {!isCollapsed && (
        isLoading ? (
          <div className="text-sm text-[#666666] mt-3">Loading...</div>
        ) : (
          <div className="space-y-2 mt-3">
            {relatedAtoms.map((result) => (
              <button
                key={result.id}
                onClick={() => onAtomClick(result.id)}
                className="w-full text-left p-3 bg-[#252525] rounded-md hover:bg-[#2d2d2d] transition-colors"
              >
                <p className="text-sm text-[#dcddde] line-clamp-2">
                  {result.content.length > 100
                    ? result.content.slice(0, 100) + '...'
                    : result.content}
                </p>
                <div className="flex items-center gap-2 mt-2">
                  <span className="text-xs text-[#7c3aed]">
                    {Math.round(result.similarity_score * 100)}% similar
                  </span>
                </div>
              </button>
            ))}
          </div>
        )
      )}
    </div>
  );
}

