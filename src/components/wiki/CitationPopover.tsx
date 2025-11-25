import { useEffect, useRef } from 'react';
import { WikiCitation } from '../../stores/wiki';
import { useKeyboard } from '../../hooks/useKeyboard';

interface CitationPopoverProps {
  citation: WikiCitation;
  anchorEl: HTMLElement | null;
  onClose: () => void;
  onViewAtom: (atomId: string) => void;
}

export function CitationPopover({ citation, anchorEl, onClose, onViewAtom }: CitationPopoverProps) {
  const popoverRef = useRef<HTMLDivElement>(null);

  // Close on Escape
  useKeyboard('Escape', onClose, true);

  // Close on click outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (popoverRef.current && !popoverRef.current.contains(event.target as Node)) {
        onClose();
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, [onClose]);

  // Position the popover
  useEffect(() => {
    if (!anchorEl || !popoverRef.current) return;

    const anchorRect = anchorEl.getBoundingClientRect();
    const popover = popoverRef.current;
    const popoverRect = popover.getBoundingClientRect();

    // Calculate position - prefer below, but go above if not enough space
    const spaceBelow = window.innerHeight - anchorRect.bottom;
    const spaceAbove = anchorRect.top;

    let top: number;
    if (spaceBelow >= popoverRect.height + 8 || spaceBelow >= spaceAbove) {
      // Position below
      top = anchorRect.bottom + 8;
    } else {
      // Position above
      top = anchorRect.top - popoverRect.height - 8;
    }

    // Horizontal positioning - center on anchor, but keep within viewport
    let left = anchorRect.left + anchorRect.width / 2 - popoverRect.width / 2;
    left = Math.max(8, Math.min(left, window.innerWidth - popoverRect.width - 8));

    popover.style.top = `${top}px`;
    popover.style.left = `${left}px`;
  }, [anchorEl]);

  const handleViewAtom = () => {
    onViewAtom(citation.atom_id);
    onClose();
  };

  // Truncate excerpt if needed
  const displayExcerpt = citation.excerpt.length > 300 
    ? citation.excerpt.slice(0, 297) + '...'
    : citation.excerpt;

  return (
    <div
      ref={popoverRef}
      className="fixed z-[100] w-[400px] max-w-[calc(100vw-16px)] bg-[#2d2d2d] border border-[#3d3d3d] rounded-lg shadow-xl"
      style={{ top: 0, left: 0 }}
    >
      {/* Citation number badge */}
      <div className="px-4 py-2 border-b border-[#3d3d3d] flex items-center gap-2">
        <span className="inline-flex items-center justify-center w-6 h-6 rounded bg-[#7c3aed]/20 text-[#a78bfa] text-xs font-medium">
          {citation.citation_index}
        </span>
        <span className="text-xs text-[#888888]">Source excerpt</span>
      </div>

      {/* Excerpt content */}
      <div className="px-4 py-3">
        <p className="text-sm text-[#dcddde] leading-relaxed whitespace-pre-wrap">
          {displayExcerpt}
        </p>
      </div>

      {/* Footer with link */}
      <div className="px-4 py-2 border-t border-[#3d3d3d]">
        <button
          onClick={handleViewAtom}
          className="flex items-center gap-1 text-sm text-[#7c3aed] hover:text-[#a78bfa] transition-colors"
        >
          View full atom
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M14 5l7 7m0 0l-7 7m7-7H3" />
          </svg>
        </button>
      </div>
    </div>
  );
}

