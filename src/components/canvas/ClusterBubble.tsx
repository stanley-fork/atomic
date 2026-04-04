import { memo, useMemo } from 'react';
import type { CanvasNode } from '../../lib/api';
import { computeMiniLayout } from './useMiniForceSimulation';

const MAX_DOTS = 15;
const DOT_RADIUS = 3;
const BLOB_PADDING = 18;

interface ClusterBubbleProps {
  node: CanvasNode;
  x: number;
  y: number;
  onClick: (node: CanvasNode) => void;
  onMouseEnter?: () => void;
  style?: React.CSSProperties;
}

function stringToHue(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  return Math.abs(hash % 360);
}

/** Compute convex hull via Graham scan */
function convexHull(points: { x: number; y: number }[]): { x: number; y: number }[] {
  if (points.length < 3) return [...points];
  const sorted = [...points].sort((a, b) => a.y - b.y || a.x - b.x);
  const pivot = sorted[0];
  sorted.slice(1).sort((a, b) => {
    const aa = Math.atan2(a.y - pivot.y, a.x - pivot.x);
    const ab = Math.atan2(b.y - pivot.y, b.x - pivot.x);
    return aa !== ab ? aa - ab : (a.x - pivot.x) ** 2 + (a.y - pivot.y) ** 2 - ((b.x - pivot.x) ** 2 + (b.y - pivot.y) ** 2);
  });
  const hull: { x: number; y: number }[] = [pivot];
  for (let i = 1; i < sorted.length; i++) {
    while (hull.length > 1) {
      const o = hull[hull.length - 2], a = hull[hull.length - 1], b = sorted[i];
      if ((a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x) > 0) break;
      hull.pop();
    }
    hull.push(sorted[i]);
  }
  return hull;
}

/** Generate smooth blob path from hull points */
function blobPath(hull: { x: number; y: number }[], padding: number): string {
  if (hull.length === 0) return '';
  if (hull.length === 1) {
    const p = hull[0];
    const r = padding;
    return `M ${p.x - r} ${p.y} A ${r} ${r} 0 1 1 ${p.x + r} ${p.y} A ${r} ${r} 0 1 1 ${p.x - r} ${p.y}`;
  }
  if (hull.length === 2) {
    const [a, b] = hull;
    const dx = b.x - a.x, dy = b.y - a.y;
    const len = Math.sqrt(dx * dx + dy * dy) || 1;
    const nx = (-dy / len) * padding, ny = (dx / len) * padding;
    return `M ${a.x + nx} ${a.y + ny} A ${padding} ${padding} 0 0 0 ${a.x - nx} ${a.y - ny} L ${b.x - nx} ${b.y - ny} A ${padding} ${padding} 0 0 0 ${b.x + nx} ${b.y + ny} Z`;
  }

  // Expand outward
  const exp: { x: number; y: number }[] = [];
  for (let i = 0; i < hull.length; i++) {
    const prev = hull[(i - 1 + hull.length) % hull.length];
    const curr = hull[i];
    const next = hull[(i + 1) % hull.length];
    const n1x = -(curr.y - prev.y), n1y = curr.x - prev.x;
    const n1l = Math.sqrt(n1x * n1x + n1y * n1y) || 1;
    const n2x = -(next.y - curr.y), n2y = next.x - curr.x;
    const n2l = Math.sqrt(n2x * n2x + n2y * n2y) || 1;
    const nx = (n1x / n1l + n2x / n2l) / 2;
    const ny = (n1y / n1l + n2y / n2l) / 2;
    const nl = Math.sqrt(nx * nx + ny * ny) || 1;
    exp.push({ x: curr.x + (nx / nl) * padding, y: curr.y + (ny / nl) * padding });
  }

  // Smooth bezier through midpoints
  const mids = exp.map((c, i) => {
    const n = exp[(i + 1) % exp.length];
    return { x: (c.x + n.x) / 2, y: (c.y + n.y) / 2 };
  });
  let d = `M ${mids[0].x} ${mids[0].y}`;
  for (let i = 0; i < exp.length; i++) {
    const v = exp[(i + 1) % exp.length];
    const m = mids[(i + 1) % mids.length];
    d += ` Q ${v.x} ${v.y} ${m.x} ${m.y}`;
  }
  return d + ' Z';
}

export const ClusterBubble = memo(function ClusterBubble({
  node,
  x,
  y,
  onClick,
  onMouseEnter,
  style: externalStyle,
}: ClusterBubbleProps) {
  const hue = useMemo(() => stringToHue(node.label), [node.label]);

  // How many dots to show
  const dotCount = Math.min(node.atom_count, MAX_DOTS);
  const extraCount = node.atom_count - dotCount;

  // Scale blob radius by atom count
  const blobRadius = useMemo(() => {
    if (dotCount <= 3) return 18;
    if (dotCount <= 8) return 24;
    return 30 + Math.min(dotCount - 8, 7) * 2;
  }, [dotCount]);

  // Compute dot positions centered at (0, 0) — we'll translate the whole SVG
  const dotPositions = useMemo(
    () => computeMiniLayout(dotCount, { x: 0, y: 0 }, blobRadius, DOT_RADIUS),
    [dotCount, blobRadius],
  );

  // Blob path
  const path = useMemo(() => {
    if (dotPositions.length === 0) return '';
    return blobPath(
      dotPositions.length >= 3 ? convexHull(dotPositions) : dotPositions,
      BLOB_PADDING,
    );
  }, [dotPositions]);

  // SVG viewBox size
  const svgHalf = blobRadius + BLOB_PADDING + 10;
  const svgSize = svgHalf * 2;

  // Dot colors — vary slightly by index for visual interest
  const dotColor = (i: number) => {
    const h = (hue + i * 25) % 360;
    return `hsla(${h}, 45%, 55%, 0.7)`;
  };

  return (
    <div
      className="absolute cursor-pointer select-none transition-opacity duration-150"
      style={{
        left: x,
        top: y,
        transform: 'translate(-50%, -50%)',
        ...externalStyle,
      }}
      onClick={() => onClick(node)}
      onMouseEnter={onMouseEnter}
    >
      <div className="flex flex-col items-center">
        {/* SVG blob + dots */}
        <svg
          width={svgSize}
          height={svgSize}
          viewBox={`${-svgHalf} ${-svgHalf} ${svgSize} ${svgSize}`}
          className="overflow-visible"
        >
          {/* Blob background */}
          <path
            d={path}
            fill={`hsla(${hue}, 25%, 28%, 0.12)`}
            stroke={`hsla(${hue}, 35%, 50%, 0.2)`}
            strokeWidth={0.75}
          />
          {/* Dots */}
          {dotPositions.map((pos, i) => (
            <circle
              key={i}
              cx={pos.x}
              cy={pos.y}
              r={DOT_RADIUS}
              fill={dotColor(i)}
            />
          ))}
          {/* +N indicator */}
          {extraCount > 0 && (
            <text
              x={0}
              y={blobRadius + BLOB_PADDING - 2}
              textAnchor="middle"
              fill={`hsla(${hue}, 30%, 55%, 0.5)`}
              fontSize={7}
              style={{ fontFamily: 'var(--font-sans)' }}
            >
              +{extraCount}
            </text>
          )}
        </svg>

        {/* Label below */}
        <span
          className="text-[9px] text-center leading-tight mt-0.5 max-w-[100px] truncate"
          style={{ color: `hsla(${hue}, 30%, 65%, 0.8)` }}
        >
          {node.label}
        </span>

        {/* Atom count */}
        <span className="text-[8px] text-[var(--color-text-tertiary)]">
          {node.atom_count}
        </span>
      </div>
    </div>
  );
});
