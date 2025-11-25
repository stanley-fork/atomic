import { useState } from 'react';
import { TagTree } from '../tags/TagTree';
import { SettingsButton, SettingsModal } from '../settings';

export function LeftPanel() {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  
  return (
    <aside className="w-[250px] h-full bg-[#252525] border-r border-[#3d3d3d] flex flex-col">
      {/* Header */}
      <div className="px-4 py-3 border-b border-[#3d3d3d] flex items-center justify-between">
        <h1 className="text-lg font-bold text-[#dcddde] flex items-center gap-2">
          <svg className="w-5 h-5 text-[#7c3aed]" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
          </svg>
          Atomic
        </h1>
        <SettingsButton onClick={() => setIsSettingsOpen(true)} />
      </div>
      
      {/* Tag Tree */}
      <div className="flex-1 overflow-hidden">
        <TagTree />
      </div>
      
      <SettingsModal isOpen={isSettingsOpen} onClose={() => setIsSettingsOpen(false)} />
    </aside>
  );
}

