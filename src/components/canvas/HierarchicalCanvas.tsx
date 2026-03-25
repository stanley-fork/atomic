import { useEffect, useCallback, useRef, useState, useMemo } from 'react';
import { useShallow } from 'zustand/react/shallow';
import { TransformWrapper, TransformComponent } from 'react-zoom-pan-pinch';
import { useUIStore } from '../../stores/ui';
import { CanvasBreadcrumb } from './CanvasBreadcrumb';
import { ClusterBubble } from './ClusterBubble';
import { AtomNode } from './AtomNode';
import { useHierarchicalForceSimulation } from './useHierarchicalForceSimulation';
import type { CanvasNode, CanvasEdge } from '../../lib/api';

const EMPTY_NODES: CanvasNode[] = [];
const EMPTY_EDGES: CanvasEdge[] = [];

export function HierarchicalCanvas() {
  const { currentLevel, isLoading } = useUIStore(
    useShallow(s => ({
      currentLevel: s.canvasNav.currentLevel,
      isLoading: s.canvasNav.isLoading,
    }))
  );
  const navigateCanvas = useUIStore(s => s.navigateCanvas);
  const openDrawer = useUIStore(s => s.openDrawer);
  const selectedTagId = useUIStore(s => s.selectedTagId);

  const containerRef = useRef<HTMLDivElement>(null);
  const [dimensions, setDimensions] = useState({ width: 0, height: 0 });
  const [fadeIn, setFadeIn] = useState(false);

  // Measure container
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    const observer = new ResizeObserver((entries) => {
      const { width, height } = entries[0].contentRect;
      setDimensions({ width, height });
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  // Load root level on mount (only once)
  const hasInitializedRef = useRef(false);
  useEffect(() => {
    if (!currentLevel && !isLoading && !hasInitializedRef.current) {
      hasInitializedRef.current = true;
      navigateCanvas(null);
    }
  }, [currentLevel, isLoading, navigateCanvas]);

  // Sync with sidebar tag selection
  const lastSyncedTagRef = useRef<string | null>(null);
  useEffect(() => {
    if (selectedTagId && selectedTagId !== lastSyncedTagRef.current) {
      lastSyncedTagRef.current = selectedTagId;
      navigateCanvas(selectedTagId);
    } else if (!selectedTagId && lastSyncedTagRef.current) {
      lastSyncedTagRef.current = null;
      navigateCanvas(null);
    }
  }, [selectedTagId, navigateCanvas]);

  // Fade-in transition on level change
  useEffect(() => {
    if (currentLevel) {
      setFadeIn(false);
      const timer = requestAnimationFrame(() => setFadeIn(true));
      return () => cancelAnimationFrame(timer);
    }
  }, [currentLevel]);

  const nodes = currentLevel?.nodes ?? EMPTY_NODES;
  const edges = currentLevel?.edges ?? EMPTY_EDGES;

  const { simNodes } = useHierarchicalForceSimulation({
    nodes,
    edges,
    width: dimensions.width,
    height: dimensions.height,
  });

  const handleBreadcrumbNavigate = useCallback((parentId: string | null) => {
    navigateCanvas(parentId);
  }, [navigateCanvas]);

  const handleNodeClick = useCallback((node: CanvasNode) => {
    if (node.node_type === 'atom') {
      openDrawer('viewer', node.id);
    } else if (node.node_type === 'semantic_cluster') {
      navigateCanvas(node.id, node.children_ids);
    } else {
      navigateCanvas(node.id);
    }
  }, [navigateCanvas, openDrawer]);

  const handleAtomNodeClick = useCallback((atomId: string) => {
    openDrawer('viewer', atomId);
  }, [openDrawer]);

  // Build node position map for edge rendering
  const nodePositionMap = useMemo(() => {
    const map = new Map<string, { x: number; y: number }>();
    for (const sn of simNodes) {
      map.set(sn.id, { x: sn.x, y: sn.y });
    }
    return map;
  }, [simNodes]);

  // Build atom summary objects for AtomNode (minimal shape)
  const atomSummaryMap = useMemo(() => {
    const map = new Map<string, import('../../stores/atoms').AtomSummary>();
    for (const node of nodes) {
      if (node.node_type === 'atom') {
        map.set(node.id, {
          id: node.id,
          title: node.label,
          snippet: '',
          tags: (node.dominant_tags || []).map(name => ({ id: name, name })),
          source_url: null,
          source: null,
          published_at: null,
          created_at: '',
          updated_at: '',
          embedding_status: 'complete' as const,
          tagging_status: 'complete' as const,
        });
      }
    }
    return map;
  }, [nodes]);

  return (
    <div className="flex flex-col h-full w-full">
      {/* Breadcrumb */}
      <CanvasBreadcrumb
        breadcrumb={currentLevel?.breadcrumb ?? []}
        onNavigate={handleBreadcrumbNavigate}
      />

      {/* Canvas area */}
      <div ref={containerRef} className="flex-1 relative overflow-hidden">
        {isLoading && (
          <div className="absolute inset-0 flex items-center justify-center z-10 bg-[var(--color-bg-main)]/50">
            <div className="flex items-center gap-2 text-[var(--color-text-secondary)]">
              <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
              </svg>
              <span className="text-sm">Loading...</span>
            </div>
          </div>
        )}

        {!isLoading && nodes.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center">
            <div className="text-center text-[var(--color-text-secondary)]">
              <p className="text-lg mb-2">No content to display</p>
              <p className="text-sm">Create some atoms to see them on the canvas</p>
            </div>
          </div>
        )}

        {dimensions.width > 0 && simNodes.length > 0 && (
          <div className={`w-full h-full transition-opacity duration-200 ${fadeIn ? 'opacity-100' : 'opacity-0'}`}>
          <TransformWrapper
            initialScale={1}
            minScale={0.3}
            maxScale={3}
            limitToBounds={false}
            doubleClick={{ disabled: true }}
            panning={{ velocityDisabled: true }}
          >
            {({ zoomIn, zoomOut, resetTransform }) => (
              <>
                <TransformComponent
                  wrapperStyle={{ width: '100%', height: '100%' }}
                  contentStyle={{ width: dimensions.width, height: dimensions.height }}
                >
                  {/* Edge lines */}
                  <svg
                    className="absolute top-0 left-0 pointer-events-none"
                    width={dimensions.width}
                    height={dimensions.height}
                    style={{ zIndex: 0, overflow: 'visible' }}
                  >
                    {edges.map((edge) => {
                      const src = nodePositionMap.get(edge.source_id);
                      const tgt = nodePositionMap.get(edge.target_id);
                      if (!src || !tgt) return null;
                      return (
                        <line
                          key={`${edge.source_id}-${edge.target_id}`}
                          x1={src.x}
                          y1={src.y}
                          x2={tgt.x}
                          y2={tgt.y}
                          stroke="var(--color-accent)"
                          strokeWidth={1 + edge.weight}
                          strokeOpacity={0.15 + edge.weight * 0.2}
                          strokeDasharray={edge.weight < 0.5 ? '4,3' : undefined}
                        />
                      );
                    })}
                  </svg>

                  {/* Nodes */}
                  {simNodes.map((sn) => {
                    const node = sn.canvasNode;
                    if (node.node_type === 'atom') {
                      const atomSummary = atomSummaryMap.get(node.id);
                      if (!atomSummary) return null;
                      return (
                        <AtomNode
                          key={node.id}
                          atom={atomSummary}
                          atomId={node.id}
                          x={sn.x}
                          y={sn.y}
                          isFaded={false}
                          onClick={handleAtomNodeClick}
                        />
                      );
                    }
                    return (
                      <ClusterBubble
                        key={node.id}
                        node={node}
                        x={sn.x}
                        y={sn.y}
                        onClick={handleNodeClick}
                      />
                    );
                  })}
                </TransformComponent>

                {/* Zoom controls */}
                <div className="absolute bottom-4 left-4 flex flex-col gap-1 z-10">
                  <button
                    onClick={() => zoomIn()}
                    className="w-8 h-8 rounded-md bg-[var(--color-bg-card)] border border-[var(--color-border)] text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] flex items-center justify-center transition-colors"
                    title="Zoom in"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M12 4v16m8-8H4" />
                    </svg>
                  </button>
                  <button
                    onClick={() => zoomOut()}
                    className="w-8 h-8 rounded-md bg-[var(--color-bg-card)] border border-[var(--color-border)] text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] flex items-center justify-center transition-colors"
                    title="Zoom out"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M20 12H4" />
                    </svg>
                  </button>
                  <button
                    onClick={() => resetTransform()}
                    className="w-8 h-8 rounded-md bg-[var(--color-bg-card)] border border-[var(--color-border)] text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)] hover:bg-[var(--color-bg-hover)] flex items-center justify-center transition-colors"
                    title="Reset view"
                  >
                    <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
                      <path strokeLinecap="round" strokeLinejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                    </svg>
                  </button>
                </div>
              </>
            )}
          </TransformWrapper>
          </div>
        )}
      </div>
    </div>
  );
}
