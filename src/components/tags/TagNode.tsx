import { useEffect, useRef, memo, MouseEvent } from 'react';
import { TagWithCount } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';

interface TagNodeProps {
  tag: TagWithCount;
  level: number;
  selectedTagId: string | null;
  onSelect: (tagId: string) => void;
  onContextMenu: (e: MouseEvent, tag: TagWithCount) => void;
}

export const TagNode = memo(function TagNode({ tag, level, selectedTagId, onSelect, onContextMenu }: TagNodeProps) {
  const nodeRef = useRef<HTMLDivElement>(null);
  const openWikiDrawer = useUIStore(s => s.openWikiDrawer);
  const openChatDrawer = useUIStore(s => s.openChatDrawer);
  const isExpanded = useUIStore(s => !!s.expandedTagIds[tag.id]);
  const toggleTagExpanded = useUIStore(s => s.toggleTagExpanded);
  const hasChildren = tag.children && tag.children.length > 0;
  const isSelected = selectedTagId === tag.id;

  // Scroll into view when selected
  useEffect(() => {
    if (isSelected && nodeRef.current) {
      // Use a small delay to ensure the DOM has updated after expansion
      setTimeout(() => {
        nodeRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
      }, 50);
    }
  }, [isSelected]);

  const handleToggle = (e: MouseEvent) => {
    e.stopPropagation();
    toggleTagExpanded(tag.id);
  };

  const handleContextMenu = (e: MouseEvent) => {
    e.preventDefault();
    onContextMenu(e, tag);
  };

  const handleWikiClick = (e: MouseEvent) => {
    e.stopPropagation();
    openWikiDrawer(tag.id, tag.name);
  };

  const handleChatClick = (e: MouseEvent) => {
    e.stopPropagation();
    openChatDrawer(tag.id);
  };

  return (
    <div>
      <div
        ref={nodeRef}
        className={`group flex items-center gap-1 px-2 py-1.5 rounded-md cursor-pointer transition-colors ${
          isSelected
            ? 'bg-[var(--color-accent)]/20 text-[var(--color-text-primary)]'
            : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-card)] hover:text-[var(--color-text-primary)]'
        }`}
        style={{ paddingLeft: `${8 + level * 16}px` }}
        onClick={() => onSelect(tag.id)}
        onContextMenu={handleContextMenu}
      >
        {hasChildren ? (
          <button
            onClick={handleToggle}
            className="w-4 h-4 flex items-center justify-center text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]"
          >
            <svg
              className={`w-3 h-3 transition-transform ${isExpanded ? 'rotate-90' : ''}`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
            </svg>
          </button>
        ) : (
          <span className="w-4" />
        )}
        <span className="flex-1 truncate text-sm">{tag.name}</span>
        {/* Chat icon - visible on hover */}
        <button
          onClick={handleChatClick}
          className="w-5 h-5 flex items-center justify-center opacity-0 group-hover:opacity-100 text-[var(--color-text-secondary)] hover:text-[var(--color-accent-light)] transition-all"
          title="Chat with this tag"
        >
          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 12h.01M12 12h.01M16 12h.01M21 12c0 4.418-4.03 8-9 8a9.863 9.863 0 01-4.255-.949L3 20l1.395-3.72C3.512 15.042 3 13.574 3 12c0-4.418 4.03-8 9-8s9 3.582 9 8z" />
          </svg>
        </button>
        {/* Article icon - visible on hover */}
        <button
          onClick={handleWikiClick}
          className="w-5 h-5 flex items-center justify-center opacity-0 group-hover:opacity-100 text-[var(--color-text-secondary)] hover:text-[var(--color-accent-light)] transition-all"
          title="View wiki article"
        >
          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
        </button>
        <span className="text-xs text-[var(--color-text-tertiary)] tabular-nums">{tag.atom_count}</span>
      </div>
      {hasChildren && isExpanded && (
        <div>
          {tag.children.map((child) => (
            <TagNode
              key={child.id}
              tag={child}
              level={level + 1}
              selectedTagId={selectedTagId}
              onSelect={onSelect}
              onContextMenu={onContextMenu}
            />
          ))}
        </div>
      )}
    </div>
  );
});

