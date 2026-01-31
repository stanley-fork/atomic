import { useMemo, useCallback } from 'react';
import { AtomNode } from './AtomNode';
import { ConnectionLines, Connection } from './ConnectionLines';
import { SimulationNode } from './useForceSimulation';

const CANVAS_SIZE = 5000;
const HUB_THRESHOLD = 8;
const VIEWPORT_BUFFER = 300; // px buffer outside viewport

interface CanvasContentProps {
  nodes: SimulationNode[];
  connections: Connection[];
  fadedAtomIds: Set<string>;
  connectionCounts: Record<string, number>;
  highlightedAtomId: string | null;
  onAtomClick: (atomId: string) => void;
  transformState: { scale: number; positionX: number; positionY: number };
  containerWidth: number;
  containerHeight: number;
}

export function CanvasContent({
  nodes,
  connections,
  fadedAtomIds,
  connectionCounts,
  highlightedAtomId,
  onAtomClick,
  transformState,
  containerWidth,
  containerHeight,
}: CanvasContentProps) {
  // Stable onClick handler to prevent AtomNode re-renders
  const handleAtomClick = useCallback((atomId: string) => {
    onAtomClick(atomId);
  }, [onAtomClick]);

  // Calculate viewport bounds in canvas coordinates
  const viewportBounds = useMemo(() => {
    const { scale, positionX, positionY } = transformState;
    return {
      left: -positionX / scale - VIEWPORT_BUFFER,
      top: -positionY / scale - VIEWPORT_BUFFER,
      right: (containerWidth - positionX) / scale + VIEWPORT_BUFFER,
      bottom: (containerHeight - positionY) / scale + VIEWPORT_BUFFER,
    };
  }, [transformState, containerWidth, containerHeight]);

  // Filter nodes to only those in viewport
  const visibleNodes = useMemo(() => {
    const { left, top, right, bottom } = viewportBounds;
    return nodes.filter(node =>
      node.x >= left && node.x <= right &&
      node.y >= top && node.y <= bottom
    );
  }, [nodes, viewportBounds]);

  // Build position map for connection lines (need all nodes for line endpoints)
  const nodePositions = useMemo(() => {
    const map = new Map<string, { x: number; y: number }>();
    for (const node of nodes) {
      map.set(node.id, { x: node.x, y: node.y });
    }
    return map;
  }, [nodes]);

  // Identify hub atoms
  const hubAtomIds = useMemo(() => {
    const hubs = new Set<string>();
    for (const [atomId, count] of Object.entries(connectionCounts)) {
      if (count >= HUB_THRESHOLD) {
        hubs.add(atomId);
      }
    }
    return hubs;
  }, [connectionCounts]);

  return (
    <div
      className="relative bg-[var(--color-bg-main)]"
      style={{
        width: CANVAS_SIZE,
        height: CANVAS_SIZE,
      }}
    >
      {/* Connection lines (behind atoms) */}
      <ConnectionLines
        connections={connections}
        nodePositions={nodePositions}
        fadedAtomIds={fadedAtomIds}
        viewportBounds={viewportBounds}
      />

      {/* Atom nodes - only render visible ones */}
      {visibleNodes.map((node) => (
        <AtomNode
          key={node.id}
          atom={node.atom}
          x={node.x}
          y={node.y}
          isFaded={fadedAtomIds.has(node.id)}
          isHub={hubAtomIds.has(node.id)}
          isHighlighted={node.id === highlightedAtomId}
          connectionCount={connectionCounts[node.id] || 0}
          onClick={handleAtomClick}
          atomId={node.id}
        />
      ))}
    </div>
  );
}
