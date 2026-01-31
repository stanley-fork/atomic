import { useEffect, useState, useMemo, useCallback, useRef } from 'react';
import { TransformWrapper, TransformComponent, useControls } from 'react-zoom-pan-pinch';
import { AtomWithTags } from '../../stores/atoms';
import { CanvasContent } from './CanvasContent';
import { CanvasControls } from './CanvasControls';
import { Connection } from './ConnectionLines';
import { useForceSimulation, buildConnections, SimulationNode } from './useForceSimulation';
import {
  getAtomPositions,
  saveAtomPositions,
  getAtomsWithEmbeddings,
  getSemanticEdges,
  getConnectionCounts,
  AtomPosition,
  SemanticEdge,
} from '../../lib/tauri';

const CANVAS_CENTER = 2500;

// Helper component to zoom to a highlighted atom
interface ZoomToHighlightProps {
  atomId: string | null;
  nodePositions: Map<string, { x: number; y: number }>;
  onComplete: () => void;
}

function ZoomToHighlight({ atomId, nodePositions, onComplete }: ZoomToHighlightProps) {
  const { setTransform } = useControls();
  const hasZoomedRef = useRef<string | null>(null);

  useEffect(() => {
    if (!atomId || hasZoomedRef.current === atomId) return;

    const pos = nodePositions.get(atomId);
    if (!pos) return;

    // Calculate transform to center on the atom
    // Assuming viewport is ~800x600
    const scale = 1;
    const x = -pos.x * scale + 400;
    const y = -pos.y * scale + 300;

    // Animate to position
    setTransform(x, y, scale, 500, 'easeOut');
    hasZoomedRef.current = atomId;

    // Clear highlight after animation
    const timer = setTimeout(() => {
      onComplete();
    }, 3000);

    return () => clearTimeout(timer);
  }, [atomId, nodePositions, setTransform, onComplete]);

  return null;
}

interface CanvasViewProps {
  atoms: AtomWithTags[];
  selectedTagId: string | null;
  searchResultIds: string[] | null; // atom IDs matching search, null = not searching
  highlightedAtomId: string | null;
  onAtomClick: (atomId: string) => void;
  onHighlightClear: () => void;
}

// Connection display options
export interface ConnectionOptions {
  showTagConnections: boolean;
  showSemanticConnections: boolean;
  minSimilarity: number;
}

export function CanvasView({
  atoms,
  selectedTagId,
  searchResultIds,
  highlightedAtomId,
  onAtomClick,
  onHighlightClear,
}: CanvasViewProps) {
  const [positions, setPositions] = useState<Map<string, { x: number; y: number }>>(
    new Map()
  );
  const [embeddings, setEmbeddings] = useState<Map<string, number[]>>(new Map());
  const [semanticEdges, setSemanticEdges] = useState<SemanticEdge[]>([]);
  const [connectionCounts, setConnectionCounts] = useState<Record<string, number>>({});
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const hasLoadedRef = useRef(false);

  // Transform state for viewport culling
  const [transformState, setTransformState] = useState({ scale: 1, positionX: 0, positionY: 0 });
  const containerRef = useRef<HTMLDivElement>(null);

  const handleTransformed = useCallback((_ref: any, state: { scale: number; positionX: number; positionY: number }) => {
    setTransformState({
      scale: state.scale,
      positionX: state.positionX,
      positionY: state.positionY,
    });
  }, []);

  // Connection display options
  const [connectionOptions, setConnectionOptions] = useState<ConnectionOptions>({
    showTagConnections: true,
    showSemanticConnections: true,
    minSimilarity: 0.5,
  });

  // Build tag-based connections
  const tagConnections = useMemo(() => buildConnections(atoms), [atoms]);

  // Build hybrid connections (tag + semantic)
  const connections = useMemo(() => {
    const { showTagConnections, showSemanticConnections, minSimilarity } = connectionOptions;

    // Start with tag connections if enabled
    const tagConns = showTagConnections
      ? tagConnections.map(c => ({
          ...c,
          type: 'tag' as const,
          strength: Math.min(c.sharedTagCount * 0.2, 0.6),
          similarityScore: null as number | null,
        }))
      : [];

    // Add semantic connections if enabled
    const semanticConns = showSemanticConnections
      ? semanticEdges
          .filter(e => e.similarity_score >= minSimilarity)
          .map(e => ({
            sourceId: e.source_atom_id,
            targetId: e.target_atom_id,
            sharedTagCount: 0,
            type: 'semantic' as const,
            strength: e.similarity_score,
            similarityScore: e.similarity_score,
          }))
      : [];

    // Merge: if both tag and semantic exist between same atoms, mark as 'both'
    const connectionMap = new Map<string, Connection>();

    for (const conn of tagConns) {
      const key = [conn.sourceId, conn.targetId].sort().join('-');
      connectionMap.set(key, conn);
    }

    for (const conn of semanticConns) {
      const key = [conn.sourceId, conn.targetId].sort().join('-');
      const existing = connectionMap.get(key);
      if (existing) {
        // Merge: both tag and semantic
        connectionMap.set(key, {
          ...existing,
          type: 'both',
          strength: Math.min((existing.strength || 0.3) + conn.strength, 1) / 1.5,
          similarityScore: conn.similarityScore,
        });
      } else {
        connectionMap.set(key, conn);
      }
    }

    return Array.from(connectionMap.values());
  }, [tagConnections, semanticEdges, connectionOptions]);

  // Load positions, embeddings, and semantic edges on mount
  useEffect(() => {
    if (hasLoadedRef.current) return;
    hasLoadedRef.current = true;

    async function loadData() {
      try {
        setIsLoading(true);
        setError(null);

        // Load positions, embeddings, semantic edges, and connection counts in parallel
        const [positionsData, embeddingsData, edgesData, countsData] = await Promise.all([
          getAtomPositions(),
          getAtomsWithEmbeddings(),
          getSemanticEdges(0.5),
          getConnectionCounts(0.5).catch(() => ({} as Record<string, number>)),
        ]);

        // Convert positions to map
        const posMap = new Map<string, { x: number; y: number }>();
        for (const pos of positionsData) {
          posMap.set(pos.atom_id, { x: pos.x, y: pos.y });
        }
        setPositions(posMap);

        // Convert embeddings to map
        const embMap = new Map<string, number[]>();
        for (const atom of embeddingsData) {
          if (atom.embedding) {
            embMap.set(atom.id, atom.embedding);
          }
        }
        setEmbeddings(embMap);

        // Store semantic edges and connection counts
        setSemanticEdges(edgesData);
        setConnectionCounts(countsData);

        setIsLoading(false);
      } catch (err) {
        console.error('Failed to load canvas data:', err);
        setError(String(err));
        setIsLoading(false);
      }
    }

    loadData();
  }, []);

  // Handle simulation end - save positions
  const handleSimulationEnd = useCallback(async (nodes: SimulationNode[]) => {
    try {
      const positionsToSave: AtomPosition[] = nodes.map((node) => ({
        atom_id: node.id,
        x: node.x,
        y: node.y,
      }));
      await saveAtomPositions(positionsToSave);

      // Update local positions map
      const newPositions = new Map<string, { x: number; y: number }>();
      for (const node of nodes) {
        newPositions.set(node.id, { x: node.x, y: node.y });
      }
      setPositions(newPositions);
    } catch (err) {
      console.error('Failed to save positions:', err);
    }
  }, []);

  // Run force simulation
  const { nodes, isSimulating } = useForceSimulation({
    atoms,
    embeddings,
    existingPositions: positions,
    connections,
    enabled: !isLoading && atoms.length > 0,
    onSimulationEnd: handleSimulationEnd,
  });

  // Calculate faded atom IDs based on tag filter and search
  const fadedAtomIds = useMemo(() => {
    const faded = new Set<string>();

    // If searching, fade non-matching atoms
    if (searchResultIds !== null) {
      const matchingIds = new Set(searchResultIds);
      for (const atom of atoms) {
        if (!matchingIds.has(atom.id)) {
          faded.add(atom.id);
        }
      }
      return faded;
    }

    // If tag is selected, fade non-matching atoms
    if (selectedTagId) {
      for (const atom of atoms) {
        const hasTag = atom.tags.some((tag) => tag.id === selectedTagId);
        if (!hasTag) {
          faded.add(atom.id);
        }
      }
    }

    return faded;
  }, [atoms, selectedTagId, searchResultIds]);

  // Build node positions map for zoom-to-highlight
  const nodePositions = useMemo(() => {
    const map = new Map<string, { x: number; y: number }>();
    for (const node of nodes) {
      map.set(node.id, { x: node.x, y: node.y });
    }
    return map;
  }, [nodes]);

  // Calculate initial transform to center on content
  const initialTransform = useMemo(() => {
    if (nodes.length === 0) {
      return { x: -CANVAS_CENTER + 400, y: -CANVAS_CENTER + 300, scale: 1 };
    }

    // Find bounding box of all nodes
    let minX = Infinity,
      maxX = -Infinity,
      minY = Infinity,
      maxY = -Infinity;
    for (const node of nodes) {
      minX = Math.min(minX, node.x);
      maxX = Math.max(maxX, node.x);
      minY = Math.min(minY, node.y);
      maxY = Math.max(maxY, node.y);
    }

    // Add padding
    const padding = 100;
    minX -= padding;
    maxX += padding;
    minY -= padding;
    maxY += padding;

    // Calculate center of content
    const centerX = (minX + maxX) / 2;
    const centerY = (minY + maxY) / 2;

    // Calculate scale to fit content (assuming viewport is ~800x600)
    const contentWidth = maxX - minX;
    const contentHeight = maxY - minY;
    const scaleX = 800 / contentWidth;
    const scaleY = 600 / contentHeight;
    const scale = Math.min(scaleX, scaleY, 1); // Don't zoom in past 1x

    return {
      x: -centerX * scale + 400,
      y: -centerY * scale + 300,
      scale,
    };
  }, [nodes]);

  if (isLoading) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[var(--color-bg-main)]">
        <div className="flex items-center gap-3 text-[var(--color-text-secondary)]">
          <svg className="w-5 h-5 animate-spin" fill="none" viewBox="0 0 24 24">
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
          Loading canvas...
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[var(--color-bg-main)]">
        <div className="text-red-500">Error: {error}</div>
      </div>
    );
  }

  if (atoms.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center bg-[var(--color-bg-main)]">
        <div className="text-[var(--color-text-secondary)]">No atoms to display</div>
      </div>
    );
  }

  return (
    <div ref={containerRef} className="flex-1 relative overflow-hidden bg-[var(--color-bg-main)]">
      {/* Simulation loading overlay */}
      {isSimulating && (
        <div className="absolute top-4 left-1/2 -translate-x-1/2 z-20 bg-[var(--color-bg-card)] border border-[var(--color-border)] rounded-md px-4 py-2 flex items-center gap-2">
          <svg className="w-4 h-4 animate-spin text-[var(--color-text-secondary)]" fill="none" viewBox="0 0 24 24">
            <circle
              className="opacity-25"
              cx="12"
              cy="12"
              r="10"
              stroke="currentColor"
              strokeWidth="4"
            />
            <path
              className="opacity-75"
              fill="currentColor"
              d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
            />
          </svg>
          <span className="text-sm text-[var(--color-text-secondary)]">Calculating positions...</span>
        </div>
      )}

      <TransformWrapper
        initialScale={initialTransform.scale}
        initialPositionX={initialTransform.x}
        initialPositionY={initialTransform.y}
        minScale={0.1}
        maxScale={2}
        limitToBounds={false}
        smooth
        panning={{ velocityDisabled: false }}
        wheel={{ smoothStep: 0.006, step: 0.6 }}
        pinch={{ step: 20 }}
        velocityAnimation={{
          sensitivity: 1,
          animationTime: 200,
          animationType: 'easeOut',
          equalToMove: true,
        }}
        onTransformed={handleTransformed}
      >
        <ZoomToHighlight
          atomId={highlightedAtomId}
          nodePositions={nodePositions}
          onComplete={onHighlightClear}
        />
        <CanvasControls
          connectionOptions={connectionOptions}
          onConnectionOptionsChange={setConnectionOptions}
        />
        <TransformComponent
          wrapperStyle={{
            width: '100%',
            height: '100%',
          }}
        >
          <CanvasContent
            nodes={nodes}
            connections={connections}
            fadedAtomIds={fadedAtomIds}
            connectionCounts={connectionCounts}
            highlightedAtomId={highlightedAtomId}
            onAtomClick={onAtomClick}
            transformState={transformState}
            containerWidth={containerRef.current?.clientWidth ?? 800}
            containerHeight={containerRef.current?.clientHeight ?? 600}
          />
        </TransformComponent>
      </TransformWrapper>
    </div>
  );
}

