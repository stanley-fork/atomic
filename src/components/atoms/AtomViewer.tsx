import { useState } from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Button } from '../ui/Button';
import { Modal } from '../ui/Modal';
import { TagChip } from '../tags/TagChip';
import { RelatedAtoms } from './RelatedAtoms';
import { AtomWithTags } from '../../stores/atoms';
import { useAtomsStore } from '../../stores/atoms';
import { useTagsStore } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';
import { formatDate } from '../../lib/date';

interface AtomViewerProps {
  atom: AtomWithTags;
  onClose: () => void;
  onEdit: () => void;
}

export function AtomViewer({ atom, onClose, onEdit }: AtomViewerProps) {
  const { deleteAtom } = useAtomsStore();
  const { fetchTags } = useTagsStore();
  const { setSelectedTag, closeDrawer, openDrawer } = useUIStore();
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [isDeleting, setIsDeleting] = useState(false);
  const [showAllTags, setShowAllTags] = useState(false);
  const [metadataExpanded, setMetadataExpanded] = useState(false);

  const MAX_VISIBLE_TAGS = 5;
  const visibleTags = showAllTags ? atom.tags : atom.tags.slice(0, MAX_VISIBLE_TAGS);
  const hiddenCount = atom.tags.length - MAX_VISIBLE_TAGS;

  const handleDelete = async () => {
    setIsDeleting(true);
    try {
      await deleteAtom(atom.id);
      await fetchTags();
      closeDrawer();
    } catch (error) {
      console.error('Failed to delete atom:', error);
    } finally {
      setIsDeleting(false);
      setShowDeleteModal(false);
    }
  };

  const handleTagClick = (tagId: string) => {
    setSelectedTag(tagId);
    closeDrawer();
  };

  const handleRelatedAtomClick = (atomId: string) => {
    // Open the related atom in the viewer
    openDrawer('viewer', atomId);
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-[#3d3d3d]">
        <button
          onClick={onClose}
          className="text-[#888888] hover:text-[#dcddde] transition-colors"
        >
          <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" onClick={onEdit}>
            <svg className="w-4 h-4 mr-1.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
            Edit
          </Button>
          <Button variant="ghost" size="sm" onClick={() => setShowDeleteModal(true)}>
            <svg className="w-4 h-4 mr-1.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
            Delete
          </Button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-6 py-4">
        <article className="prose prose-invert prose-sm max-w-none prose-headings:text-[#dcddde] prose-p:text-[#dcddde] prose-a:text-[#7c3aed] prose-strong:text-[#dcddde] prose-code:text-[#a78bfa] prose-code:bg-[#2d2d2d] prose-code:px-1 prose-code:py-0.5 prose-code:rounded prose-pre:bg-[#2d2d2d] prose-pre:border prose-pre:border-[#3d3d3d] prose-blockquote:border-l-[#7c3aed] prose-blockquote:text-[#888888] prose-li:text-[#dcddde] prose-hr:border-[#3d3d3d]">
          <ReactMarkdown
            remarkPlugins={[remarkGfm]}
            components={{
              a: ({ href, children }) => (
                <a
                  href={href}
                  onClick={(e) => {
                    e.preventDefault();
                    if (href) {
                      openUrl(href).catch(err => console.error('Failed to open URL:', err));
                    }
                  }}
                  className="cursor-pointer"
                >
                  {children}
                </a>
              ),
            }}
          >
            {atom.content}
          </ReactMarkdown>
        </article>
      </div>

      {/* Metadata */}
      <div className="border-t border-[#3d3d3d] px-6 py-4">
        {/* Collapsible header with tags */}
        <button
          onClick={() => setMetadataExpanded(!metadataExpanded)}
          className="flex items-center justify-between w-full"
        >
          <div className="flex flex-wrap items-center gap-1.5 flex-1">
            {/* Show first 5 tags inline */}
            {atom.tags.length > 0 && (
              <>
                {visibleTags.map((tag) => (
                  <TagChip
                    key={tag.id}
                    name={tag.name}
                    size="md"
                    onClick={(e) => {
                      e.stopPropagation();
                      handleTagClick(tag.id);
                    }}
                  />
                ))}
                {!showAllTags && hiddenCount > 0 && !metadataExpanded && (
                  <span className="text-sm text-[#888888] px-2">
                    +{hiddenCount} more
                  </span>
                )}
              </>
            )}
          </div>
          <svg
            className={`w-4 h-4 text-[#888888] transition-transform ml-2 flex-shrink-0 ${metadataExpanded ? 'rotate-180' : ''}`}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>

        {/* Expanded metadata */}
        {metadataExpanded && (
          <div className="mt-3 space-y-3">
            {/* Additional tags when expanded */}
            {atom.tags.length > MAX_VISIBLE_TAGS && (
              <div className="space-y-1">
                <div className="flex flex-wrap gap-1.5">
                  {atom.tags.slice(MAX_VISIBLE_TAGS).map((tag) => (
                    <TagChip
                      key={tag.id}
                      name={tag.name}
                      size="md"
                      onClick={() => handleTagClick(tag.id)}
                    />
                  ))}
                </div>
              </div>
            )}

            {/* Source URL */}
            {atom.source_url && (
              <div className="flex items-center gap-2 text-sm">
                <span className="text-[#888888]">Source:</span>
                <a
                  href={atom.source_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[#7c3aed] hover:underline truncate"
                >
                  {atom.source_url}
                </a>
              </div>
            )}

            {/* Dates */}
            <div className="text-xs text-[#666666] space-y-1">
              <p>Created: {formatDate(atom.created_at)}</p>
              <p>Updated: {formatDate(atom.updated_at)}</p>
            </div>
          </div>
        )}
      </div>

      {/* Related Atoms - only show if embedding is complete */}
      {atom.embedding_status === 'complete' && (
        <RelatedAtoms atomId={atom.id} onAtomClick={handleRelatedAtomClick} />
      )}

      {/* Delete Confirmation Modal */}
      <Modal
        isOpen={showDeleteModal}
        onClose={() => setShowDeleteModal(false)}
        title="Delete Atom"
        confirmLabel={isDeleting ? 'Deleting...' : 'Delete'}
        confirmVariant="danger"
        onConfirm={handleDelete}
      >
        <p>Are you sure you want to delete this atom? This action cannot be undone.</p>
      </Modal>
    </div>
  );
}

