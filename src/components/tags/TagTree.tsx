import { useState, MouseEvent } from 'react';
import { TagNode } from './TagNode';
import { ContextMenu } from '../ui/ContextMenu';
import { Modal } from '../ui/Modal';
import { Input } from '../ui/Input';
import { Button } from '../ui/Button';
import { useTagsStore, TagWithCount } from '../../stores/tags';
import { useUIStore } from '../../stores/ui';
import { useAtomsStore } from '../../stores/atoms';

export function TagTree() {
  const tags = useTagsStore(s => s.tags);
  const createTag = useTagsStore(s => s.createTag);
  const updateTag = useTagsStore(s => s.updateTag);
  const deleteTag = useTagsStore(s => s.deleteTag);
  const selectedTagId = useUIStore(s => s.selectedTagId);
  const setSelectedTag = useUIStore(s => s.setSelectedTag);
  const openCommandPalette = useUIStore(s => s.openCommandPalette);
  const fetchAtoms = useAtomsStore(s => s.fetchAtoms);
  const fetchAtomsByTag = useAtomsStore(s => s.fetchAtomsByTag);

  const [contextMenu, setContextMenu] = useState<{
    position: { x: number; y: number } | null;
    tag: TagWithCount | null;
  }>({ position: null, tag: null });

  const [renameModal, setRenameModal] = useState<{
    isOpen: boolean;
    tag: TagWithCount | null;
    name: string;
  }>({ isOpen: false, tag: null, name: '' });

  const [deleteModal, setDeleteModal] = useState<{
    isOpen: boolean;
    tag: TagWithCount | null;
  }>({ isOpen: false, tag: null });

  const [newTagModal, setNewTagModal] = useState<{
    isOpen: boolean;
    parentId: string | null;
    name: string;
  }>({ isOpen: false, parentId: null, name: '' });

  const handleSelectTag = async (tagId: string | null) => {
    setSelectedTag(tagId);
    if (tagId) {
      await fetchAtomsByTag(tagId);
    } else {
      await fetchAtoms();
    }
  };

  const handleContextMenu = (e: MouseEvent, tag: TagWithCount) => {
    setContextMenu({
      position: { x: e.clientX, y: e.clientY },
      tag,
    });
  };

  const handleRename = async () => {
    if (renameModal.tag && renameModal.name.trim()) {
      await updateTag(renameModal.tag.id, renameModal.name.trim(), renameModal.tag.parent_id || undefined);
      setRenameModal({ isOpen: false, tag: null, name: '' });
    }
  };

  const handleDelete = async () => {
    if (deleteModal.tag) {
      await deleteTag(deleteModal.tag.id);
      if (selectedTagId === deleteModal.tag.id) {
        handleSelectTag(null);
      }
      setDeleteModal({ isOpen: false, tag: null });
    }
  };

  const handleCreateTag = async () => {
    if (newTagModal.name.trim()) {
      await createTag(newTagModal.name.trim(), newTagModal.parentId || undefined);
      setNewTagModal({ isOpen: false, parentId: null, name: '' });
    }
  };

  const contextMenuItems = contextMenu.tag
    ? [
        {
          label: 'Rename',
          onClick: () => {
            setRenameModal({
              isOpen: true,
              tag: contextMenu.tag,
              name: contextMenu.tag?.name || '',
            });
          },
          icon: (
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
          ),
        },
        {
          label: 'Add Child Tag',
          onClick: () => {
            setNewTagModal({
              isOpen: true,
              parentId: contextMenu.tag?.id || null,
              name: '',
            });
          },
          icon: (
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
          ),
        },
        {
          label: 'Delete',
          onClick: () => {
            setDeleteModal({
              isOpen: true,
              tag: contextMenu.tag,
            });
          },
          danger: true,
          icon: (
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          ),
        },
      ]
    : [];

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-y-auto scrollbar-hidden">
        {/* All Atoms option */}
        <div
          className={`flex items-center gap-2 px-3 py-2 cursor-pointer transition-colors ${
            selectedTagId === null
              ? 'bg-[var(--color-accent)]/20 text-[var(--color-text-primary)]'
              : 'text-[var(--color-text-secondary)] hover:bg-[var(--color-bg-card)] hover:text-[var(--color-text-primary)]'
          }`}
          onClick={() => handleSelectTag(null)}
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
          </svg>
          <span className="flex-1 text-sm font-medium">All Atoms</span>
        </div>

        {/* Tags header with search button */}
        <div className="flex items-center justify-between px-3 py-2">
          <span className="text-xs font-semibold text-[var(--color-text-tertiary)] uppercase tracking-wider">
            Tags
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation();
              openCommandPalette('#');
            }}
            className="p-1 rounded hover:bg-[var(--color-bg-hover)] text-[var(--color-text-tertiary)] hover:text-[var(--color-text-primary)] transition-colors"
            title="Search tags"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          </button>
        </div>

        {/* Tag tree */}
        {tags.length === 0 ? (
          <div className="px-3 py-4 text-sm text-[var(--color-text-tertiary)] text-center">
            No tags yet
          </div>
        ) : (
          tags.map((tag) => (
            <TagNode
              key={tag.id}
              tag={tag}
              level={0}
              selectedTagId={selectedTagId}
              onSelect={handleSelectTag}
              onContextMenu={handleContextMenu}
            />
          ))
        )}
      </div>

      {/* New Tag button */}
      <div className="p-3 border-t border-[var(--color-border)]">
        <Button
          variant="ghost"
          size="sm"
          className="w-full justify-start"
          onClick={() => setNewTagModal({ isOpen: true, parentId: null, name: '' })}
        >
          <svg className="w-4 h-4 mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
          </svg>
          New Tag
        </Button>
      </div>

      {/* Context Menu */}
      <ContextMenu
        items={contextMenuItems}
        position={contextMenu.position}
        onClose={() => setContextMenu({ position: null, tag: null })}
      />

      {/* Rename Modal */}
      <Modal
        isOpen={renameModal.isOpen}
        onClose={() => setRenameModal({ isOpen: false, tag: null, name: '' })}
        title="Rename Tag"
        confirmLabel="Rename"
        onConfirm={handleRename}
      >
        <Input
          label="Tag Name"
          value={renameModal.name}
          onChange={(e) => setRenameModal((prev) => ({ ...prev, name: e.target.value }))}
          placeholder="Enter tag name"
          autoFocus
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              handleRename();
            }
          }}
        />
      </Modal>

      {/* Delete Confirmation Modal */}
      <Modal
        isOpen={deleteModal.isOpen}
        onClose={() => setDeleteModal({ isOpen: false, tag: null })}
        title="Delete Tag"
        confirmLabel="Delete"
        confirmVariant="danger"
        onConfirm={handleDelete}
      >
        <p>
          Are you sure you want to delete the tag "{deleteModal.tag?.name}"?
          {deleteModal.tag && deleteModal.tag.children.length > 0 && (
            <span className="block mt-2 text-[var(--color-text-secondary)]">
              This will also affect {deleteModal.tag.children.length} child tag(s).
            </span>
          )}
        </p>
      </Modal>

      {/* New Tag Modal */}
      <Modal
        isOpen={newTagModal.isOpen}
        onClose={() => setNewTagModal({ isOpen: false, parentId: null, name: '' })}
        title={newTagModal.parentId ? 'New Child Tag' : 'New Tag'}
        confirmLabel="Create"
        onConfirm={handleCreateTag}
      >
        <Input
          label="Tag Name"
          value={newTagModal.name}
          onChange={(e) => setNewTagModal((prev) => ({ ...prev, name: e.target.value }))}
          placeholder="Enter tag name"
          autoFocus
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              handleCreateTag();
            }
          }}
        />
      </Modal>
    </div>
  );
}

