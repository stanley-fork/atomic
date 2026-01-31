import { useEffect, useState, useMemo, useCallback, useRef } from 'react';
import { TransformWrapper, TransformComponent } from 'react-zoom-pan-pinch';
import * as d3 from 'd3-force';
import { getAtomNeighborhood, type NeighborhoodGraph, type NeighborhoodAtom } from '../../lib/tauri';
import { useUIStore } from '../../stores/ui';

// Generate a consistent HSL color from a string (tag name)
function stringToHSL(str: string): string {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = str.charCodeAt(i) + ((hash << 5) - hash);
  }
  const h = Math.abs(hash % 360);
  const s = 50 + (hash % 20);
  const l = 45 + (hash % 10);
  return `hsl(${h}, ${s}%, ${l}%)`;
}

interface SimulationNode extends d3.SimulationNodeDatum {
  id: string;
  depth: number;
  atom: NeighborhoodAtom;
}

interface LocalGraphViewProps {
  onAtomClick: (atomId: string) => void;
}

export function LocalGraphView({ onAtomClick }: LocalGraphViewProps) {
  const localGraph = useUIStore(s => s.localGraph);
  const navigateLocalGraph = useUIStore(s => s.navigateLocalGraph);
  const goBackLocalGraph = useUIStore(s => s.goBackLocalGraph);
  const closeLocalGraph = useUIStore(s => s.closeLocalGraph);
  const setLocalGraphDepth = useUIStore(s => s.setLocalGraphDepth);
  const openDrawer = useUIStore(s => s.openDrawer);
  const [graph, setGraph] = useState<NeighborhoodGraph | null>(null);
  const [nodes, setNodes] = useState<SimulationNode[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const simulationRef = useRef<d3.Simulation<SimulationNode, undefined> | null>(null);

  // Fetch neighborhood data
  useEffect(() => {
    if (!localGraph.centerAtomId) return;

    const fetchNeighborhood = async () => {
      setIsLoading(true);
      setError(null);
      try {
        const data = await getAtomNeighborhood(localGraph.centerAtomId!, localGraph.depth, 0.5);
        setGraph(data);
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load neighborhood');
      } finally {
        setIsLoading(false);
      }
    };

    fetchNeighborhood();
  }, [localGraph.centerAtomId, localGraph.depth]);

  // Run force simulation
  useEffect(() => {
    if (!graph) return;

    // Stop any existing simulation
    if (simulationRef.current) {
      simulationRef.current.stop();
    }

    const centerX = 600;
    const centerY = 500;

    // Initialize nodes
    const initialNodes: SimulationNode[] = graph.atoms.map((atom) => ({
      id: atom.id,
      depth: atom.depth,
      atom,
      x: atom.depth === 0 ? centerX : centerX + (Math.random() - 0.5) * 200,
      y: atom.depth === 0 ? centerY : centerY + (Math.random() - 0.5) * 200,
      fx: atom.depth === 0 ? centerX : undefined, // Fix center atom
      fy: atom.depth === 0 ? centerY : undefined,
    }));

    // Build links
    const links = graph.edges.map((edge) => ({
      source: edge.source_id,
      target: edge.target_id,
      strength: edge.strength,
    }));

    // Create simulation with radial layout - increased spacing for readability
    const simulation = d3.forceSimulation(initialNodes)
      .force('charge', d3.forceManyBody().strength(-400))
      .force('collide', d3.forceCollide().radius(100))
      .force('link', d3.forceLink(links)
        .id((d: any) => d.id)
        .distance(200)
        .strength((link: any) => link.strength * 0.3))
      .force('radial', d3.forceRadial(
        (d: SimulationNode) => d.depth === 0 ? 0 : d.depth === 1 ? 250 : 450,
        centerX,
        centerY
      ).strength(0.6))
      .alphaDecay(0.05)
      .velocityDecay(0.4);

    simulationRef.current = simulation;

    // Update nodes on each tick
    simulation.on('tick', () => {
      setNodes([...initialNodes]);
    });

    return () => {
      simulation.stop();
    };
  }, [graph]);

  // Get center atom title
  const centerAtomTitle = useMemo(() => {
    if (!graph) return '';
    const centerAtom = graph.atoms.find(a => a.id === graph.center_atom_id);
    if (!centerAtom) return '';
    const content = centerAtom.content;
    // Get first line or first 50 chars
    const firstLine = content.split('\n')[0];
    return firstLine.length > 50 ? firstLine.substring(0, 50) + '...' : firstLine;
  }, [graph]);

  const handleNodeClick = useCallback((atomId: string) => {
    if (atomId === localGraph.centerAtomId) {
      // Clicking center atom opens it in drawer
      onAtomClick(atomId);
    } else {
      // Clicking other atoms navigates to them
      navigateLocalGraph(atomId);
    }
  }, [localGraph.centerAtomId, navigateLocalGraph, onAtomClick]);

  const handleNodeDoubleClick = useCallback((atomId: string) => {
    // Double-click always opens in drawer
    openDrawer('viewer', atomId);
  }, [openDrawer]);

  if (!localGraph.isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 bg-[var(--color-bg-main)] flex flex-col">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-[var(--color-border)]">
        <div className="flex items-center gap-3">
          {/* Back button */}
          {localGraph.navigationHistory.length > 1 && (
            <button
              onClick={goBackLocalGraph}
              className="p-1.5 rounded hover:bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]"
              title="Go back"
            >
              <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
              </svg>
            </button>
          )}
          <h2 className="text-[var(--color-text-primary)] font-medium">
            Neighborhood: {centerAtomTitle || 'Loading...'}
          </h2>
        </div>

        <div className="flex items-center gap-4">
          {/* Depth toggle */}
          <div className="flex items-center gap-2 text-sm">
            <span className="text-[var(--color-text-secondary)]">Depth:</span>
            <button
              onClick={() => setLocalGraphDepth(1)}
              className={`px-2 py-1 rounded ${localGraph.depth === 1 ? 'bg-[var(--color-accent)] text-white' : 'bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)]'}`}
            >
              1
            </button>
            <button
              onClick={() => setLocalGraphDepth(2)}
              className={`px-2 py-1 rounded ${localGraph.depth === 2 ? 'bg-[var(--color-accent)] text-white' : 'bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)]'}`}
            >
              2
            </button>
          </div>

          {/* Close button */}
          <button
            onClick={closeLocalGraph}
            className="p-1.5 rounded hover:bg-[var(--color-bg-hover)] text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]"
            title="Close"
          >
            <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>
      </div>

      {/* Breadcrumb */}
      {localGraph.navigationHistory.length > 1 && graph && (
        <div className="px-4 py-2 border-b border-[var(--color-border)] text-sm flex items-center gap-1 overflow-x-auto">
          {localGraph.navigationHistory.map((atomId, idx) => {
            const atom = graph.atoms.find(a => a.id === atomId);
            const title = atom?.content.split('\n')[0].substring(0, 30) || 'Unknown';
            const isLast = idx === localGraph.navigationHistory.length - 1;

            return (
              <span key={atomId} className="flex items-center gap-1 whitespace-nowrap">
                {idx > 0 && <span className="text-[var(--color-text-tertiary)]">›</span>}
                <span
                  className={`${isLast ? 'text-[var(--color-text-primary)]' : 'text-[var(--color-text-secondary)]'}`}
                >
                  {title}{title.length >= 30 ? '...' : ''}
                </span>
              </span>
            );
          })}
        </div>
      )}

      {/* Graph content */}
      <div className="flex-1 relative overflow-hidden">
        {isLoading && (
          <div className="absolute inset-0 flex items-center justify-center bg-[var(--color-bg-main)]/80 z-10">
            <div className="text-[var(--color-text-secondary)]">Loading neighborhood...</div>
          </div>
        )}

        {error && (
          <div className="absolute inset-0 flex items-center justify-center bg-[var(--color-bg-main)]/80 z-10">
            <div className="text-red-500">{error}</div>
          </div>
        )}

        {!isLoading && !error && graph && (
          <TransformWrapper
            initialScale={1}
            minScale={0.3}
            maxScale={2}
            centerOnInit={true}
            limitToBounds={false}
            panning={{ velocityDisabled: false }}
            wheel={{ smoothStep: 0.006, step: 0.6 }}
            pinch={{ step: 20 }}
          >
            <TransformComponent wrapperStyle={{ width: '100%', height: '100%' }}>
              <div className="relative" style={{ width: 1200, height: 1000 }}>
                {/* Connection lines (SVG layer) */}
                <svg className="absolute top-0 left-0 w-full h-full pointer-events-none">
                  {graph.edges.map((edge) => {
                    const sourceNode = nodes.find(n => n.id === edge.source_id);
                    const targetNode = nodes.find(n => n.id === edge.target_id);
                    if (!sourceNode || !targetNode) return null;

                    const strokeColor = edge.edge_type === 'semantic'
                      ? 'var(--color-accent)'
                      : edge.edge_type === 'both'
                      ? 'var(--color-accent-light)'
                      : 'var(--color-text-tertiary)';

                    const strokeDash = edge.edge_type === 'semantic' ? '6,3' : undefined;

                    return (
                      <line
                        key={`${edge.source_id}-${edge.target_id}`}
                        x1={sourceNode.x}
                        y1={sourceNode.y}
                        x2={targetNode.x}
                        y2={targetNode.y}
                        stroke={strokeColor}
                        strokeWidth={1 + edge.strength}
                        strokeOpacity={0.3 + edge.strength * 0.3}
                        strokeDasharray={strokeDash}
                      />
                    );
                  })}
                </svg>

                {/* Atom nodes (HTML layer) */}
                {nodes.map((node) => {
                  const isCenter = node.depth === 0;
                  const content = node.atom.content;
                  const firstLine = content.split('\n')[0]
                    .replace(/^#+\s*/, '')
                    .replace(/\*\*/g, '')
                    .replace(/\*/g, '')
                    .trim();
                  const displayText = firstLine.length > 45 ? firstLine.substring(0, 42) + '...' : firstLine;
                  const primaryTag = node.atom.tags[0];
                  const tagColor = primaryTag ? stringToHSL(primaryTag.name) : null;

                  return (
                    <div
                      key={node.id}
                      className="absolute cursor-pointer select-none"
                      style={{
                        left: node.x,
                        top: node.y,
                        transform: 'translate(-50%, -50%)',
                        width: 170,
                      }}
                      onClick={() => handleNodeClick(node.id)}
                      onDoubleClick={() => handleNodeDoubleClick(node.id)}
                    >
                      <div
                        className={`
                          bg-[var(--color-bg-card)] border rounded-md px-3 py-2
                          hover:scale-[1.02] transition-all duration-150
                          relative overflow-hidden
                          ${isCenter
                            ? 'border-[var(--color-accent)] shadow-[0_0_12px_rgb(var(--color-accent-rgb) / 0.4)] ring-2 ring-[var(--color-accent)] ring-opacity-50'
                            : node.depth === 2
                            ? 'border-[var(--color-border-hover)] border-dashed'
                            : 'border-[var(--color-border)] hover:border-[var(--color-border-hover)]'}
                        `}
                      >
                        {/* Tag color indicator */}
                        {tagColor && (
                          <div
                            className="absolute left-0 top-0 bottom-0 w-1 rounded-l"
                            style={{ backgroundColor: tagColor }}
                          />
                        )}

                        {/* Center indicator */}
                        {isCenter && (
                          <div className="absolute top-1 right-1">
                            <div className="w-2 h-2 rounded-full bg-[var(--color-accent)]" />
                          </div>
                        )}

                        <p className={`text-sm text-[var(--color-text-primary)] line-clamp-2 break-words ${isCenter ? 'font-medium' : ''}`}>
                          {displayText || 'Empty atom'}
                        </p>

                        {/* Tag chip */}
                        {primaryTag && (
                          <div className="flex items-center gap-1 mt-1.5">
                            <span
                              className="text-[10px] px-1.5 py-0.5 rounded"
                              style={{
                                backgroundColor: tagColor ? `${tagColor.replace(')', ', 0.35)')}` : 'var(--color-bg-hover)',
                                color: 'var(--color-text-primary)'
                              }}
                            >
                              {primaryTag.name.length > 12
                                ? primaryTag.name.substring(0, 10) + '...'
                                : primaryTag.name}
                            </span>
                            {node.atom.tags.length > 1 && (
                              <span className="text-[10px] text-[var(--color-text-tertiary)]">
                                +{node.atom.tags.length - 1}
                              </span>
                            )}
                          </div>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </TransformComponent>
          </TransformWrapper>
        )}
      </div>

      {/* Legend */}
      <div className="px-4 py-2 border-t border-[var(--color-border)] flex items-center gap-6 text-xs text-[var(--color-text-secondary)]">
        <div className="flex items-center gap-2">
          <div className="w-3 h-3 rounded border-2 border-[var(--color-accent)] bg-[var(--color-bg-card)]" />
          <span>Center atom</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-6 h-0.5 bg-[var(--color-text-tertiary)]" />
          <span>Tag connection</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-6 h-0.5 bg-[var(--color-accent)]" style={{ backgroundImage: 'repeating-linear-gradient(90deg, var(--color-accent) 0, var(--color-accent) 6px, transparent 6px, transparent 9px)' }} />
          <span>Semantic connection</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-6 h-0.5 bg-[var(--color-accent-light)]" />
          <span>Both</span>
        </div>
        <div className="ml-auto text-[var(--color-text-tertiary)]">
          Click to navigate • Double-click to view
        </div>
      </div>
    </div>
  );
}
