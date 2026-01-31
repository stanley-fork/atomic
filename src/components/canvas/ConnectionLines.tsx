import { memo, useMemo } from 'react';

export interface Connection {
  sourceId: string;
  targetId: string;
  sharedTagCount: number;
  type?: 'tag' | 'semantic' | 'both';
  strength?: number;
  similarityScore?: number | null;
}

interface ViewportBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

interface ConnectionLinesProps {
  connections: Connection[];
  nodePositions: Map<string, { x: number; y: number }>;
  fadedAtomIds: Set<string>;
  viewportBounds?: ViewportBounds;
}

function isInBounds(point: { x: number; y: number }, bounds: ViewportBounds): boolean {
  return point.x >= bounds.left && point.x <= bounds.right &&
         point.y >= bounds.top && point.y <= bounds.bottom;
}

export const ConnectionLines = memo(function ConnectionLines({
  connections,
  nodePositions,
  fadedAtomIds,
  viewportBounds,
}: ConnectionLinesProps) {
  // Filter connections to those with at least one endpoint in viewport
  const visibleConnections = useMemo(() => {
    if (!viewportBounds) return connections;
    return connections.filter(conn => {
      const source = nodePositions.get(conn.sourceId);
      const target = nodePositions.get(conn.targetId);
      if (!source || !target) return false;
      return isInBounds(source, viewportBounds) || isInBounds(target, viewportBounds);
    });
  }, [connections, nodePositions, viewportBounds]);

  return (
    <svg
      className="absolute top-0 left-0 w-full h-full pointer-events-none"
      style={{ zIndex: 0 }}
    >
      {visibleConnections.map((conn) => {
        const source = nodePositions.get(conn.sourceId);
        const target = nodePositions.get(conn.targetId);

        if (!source || !target) return null;

        // Check if either endpoint is faded
        const isFaded =
          fadedAtomIds.has(conn.sourceId) || fadedAtomIds.has(conn.targetId);

        // Determine stroke style based on connection type
        const connectionType = conn.type || 'tag';
        const strength = conn.strength || 0.3;

        let strokeColor: string;
        let strokeDasharray: string | undefined;
        let strokeWidth: number;
        let baseOpacity: number;

        switch (connectionType) {
          case 'semantic':
            // Purple dashed line for semantic connections
            strokeColor = 'var(--color-accent)';
            strokeDasharray = '6,3';
            strokeWidth = 1 + strength;
            baseOpacity = 0.2 + strength * 0.3;
            break;
          case 'both':
            // Thicker solid purple for combined connections
            strokeColor = 'var(--color-accent-light)';
            strokeDasharray = undefined;
            strokeWidth = 1.5 + strength;
            baseOpacity = 0.3 + strength * 0.3;
            break;
          case 'tag':
          default:
            // Gray solid line for tag connections
            strokeColor = 'var(--color-text-tertiary)';
            strokeDasharray = undefined;
            strokeWidth = 1;
            baseOpacity = 0.15;
            break;
        }

        return (
          <line
            key={`${conn.sourceId}-${conn.targetId}`}
            x1={source.x}
            y1={source.y}
            x2={target.x}
            y2={target.y}
            stroke={strokeColor}
            strokeWidth={strokeWidth}
            strokeOpacity={isFaded ? baseOpacity * 0.2 : baseOpacity}
            strokeDasharray={strokeDasharray}
          />
        );
      })}
    </svg>
  );
});
