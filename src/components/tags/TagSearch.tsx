import { useState, useMemo } from 'react';
import { useTagsStore, TagWithCount } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';
import { useAtomsStore } from '../../stores/atoms';

export function TagSearch() {
  const { tags } = useTagsStore();
  const { setSelectedTag } = useUIStore();
  const { fetchAtomsByTag } = useAtomsStore();
  const [searchQuery, setSearchQuery] = useState('');

  // Flatten the tag hierarchy for searching
  const flattenTags = (tagList: TagWithCount[]): TagWithCount[] => {
    const result: TagWithCount[] = [];
    const flatten = (tags: TagWithCount[]) => {
      for (const tag of tags) {
        result.push(tag);
        if (tag.children && tag.children.length > 0) {
          flatten(tag.children);
        }
      }
    };
    flatten(tagList);
    return result;
  };

  // Fuzzy search - case insensitive, matches anywhere in the tag name
  const filteredTags = useMemo(() => {
    if (!searchQuery.trim()) {
      return [];
    }

    const query = searchQuery.toLowerCase();
    const allTags = flattenTags(tags);

    return allTags
      .filter(tag => tag.name.toLowerCase().includes(query))
      .sort((a, b) => {
        // Sort by atom count descending
        return b.atom_count - a.atom_count;
      })
      .slice(0, 10); // Limit to 10 results
  }, [searchQuery, tags]);

  const handleSelectTag = async (tagId: string) => {
    setSelectedTag(tagId);
    await fetchAtomsByTag(tagId);
    setSearchQuery(''); // Clear search after selection
  };

  return (
    <div className="px-4 py-3 border-b border-[#3d3d3d]">
      <div className="relative">
        <input
          type="text"
          value={searchQuery}
          onChange={(e) => setSearchQuery(e.target.value)}
          placeholder="Search tags..."
          className="w-full px-3 py-1.5 pl-8 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md text-sm text-[#dcddde] placeholder-[#888888] focus:outline-none focus:ring-1 focus:ring-[#7c3aed] focus:border-transparent"
        />
        <svg
          className="absolute left-2.5 top-1/2 -translate-y-1/2 w-4 h-4 text-[#888888]"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
        </svg>
      </div>

      {/* Search Results Dropdown */}
      {searchQuery.trim() && (
        <div className="absolute left-4 right-4 mt-1 bg-[#2d2d2d] border border-[#3d3d3d] rounded-md shadow-lg z-50 max-h-[300px] overflow-y-auto">
          {filteredTags.length > 0 ? (
            <div className="py-1">
              {filteredTags.map((tag) => (
                <button
                  key={tag.id}
                  onClick={() => handleSelectTag(tag.id)}
                  className="w-full px-3 py-2 text-left text-sm hover:bg-[#3d3d3d] transition-colors flex items-center justify-between"
                >
                  <span className="text-[#dcddde]">{tag.name}</span>
                  <span className="text-xs text-[#888888]">
                    {tag.atom_count} {tag.atom_count === 1 ? 'atom' : 'atoms'}
                  </span>
                </button>
              ))}
            </div>
          ) : (
            <div className="px-3 py-2 text-sm text-[#888888]">
              No tags found
            </div>
          )}
        </div>
      )}
    </div>
  );
}
