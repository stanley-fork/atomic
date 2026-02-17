import { memo, MouseEvent, useCallback, useState } from 'react';
import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { TagWithCount, useTagsStore } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';
import { getTransport } from '../../lib/transport';

interface TagNodeProps {
  tag: TagWithCount;
  level: number;
  selectedTagId: string | null;
  onSelect: (tagId: string) => void;
  onContextMenu: (e: MouseEvent, tag: TagWithCount) => void;
}

export const TagNode = memo(function TagNode({ tag, level, selectedTagId, onSelect, onContextMenu }: TagNodeProps) {
  const openWikiDrawer = useUIStore(s => s.openWikiDrawer);
  const openChatDrawer = useUIStore(s => s.openChatDrawer);
  const isExpanded = useUIStore(s => !!s.expandedTagIds[tag.id]);
  const toggleTagExpanded = useUIStore(s => s.toggleTagExpanded);
  const fetchTagChildren = useTagsStore(s => s.fetchTagChildren);
  const hasChildren = (tag.children && tag.children.length > 0) || tag.children_total > 0;
  const isSelected = selectedTagId === tag.id;

  const handleToggle = useCallback(async (e: MouseEvent) => {
    e.stopPropagation();
    // If expanding and there are more children than currently loaded, fetch all
    if (!isExpanded && tag.children_total > tag.children.length) {
      await fetchTagChildren(tag.id);
    }
    toggleTagExpanded(tag.id);
  }, [isExpanded, tag.children_total, tag.children.length, tag.id, fetchTagChildren, toggleTagExpanded]);

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

  const [copyState, setCopyState] = useState<'idle' | 'copying' | 'done'>('idle');
  const handleCopyAll = async (e: MouseEvent) => {
    e.stopPropagation();
    if (copyState !== 'idle') return;
    setCopyState('copying');
    try {
      const atoms = await getTransport().invoke<{ content: string }[]>('get_atoms_by_tag', { tagId: tag.id });
      const text = atoms.map(a => a.content).join('\n\n---\n\n');
      await writeText(text);
      setCopyState('done');
      setTimeout(() => setCopyState('idle'), 1500);
    } catch {
      setCopyState('idle');
    }
  };

  return (
    <div
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
      {/* Copy all atoms - visible on hover */}
      <button
        onClick={handleCopyAll}
        className={`w-5 h-5 flex items-center justify-center transition-all ${
          copyState === 'done'
            ? 'opacity-100 text-green-400'
            : 'opacity-0 group-hover:opacity-100 text-[var(--color-text-secondary)] hover:text-[var(--color-accent-light)]'
        }`}
        title={`Copy all ${tag.atom_count} atoms to clipboard`}
      >
        {copyState === 'done' ? (
          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
          </svg>
        ) : (
          <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
          </svg>
        )}
      </button>
      <span className="text-xs text-[var(--color-text-tertiary)] tabular-nums">{tag.atom_count}</span>
    </div>
  );
});
