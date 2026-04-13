type RGB = [number, number, number];

export interface CanvasTheme {
  id: string;
  name: string;
  background: string;
  /** Node color at connectivity=0 (peripheral) */
  nodeMin: RGB;
  /** Node color at connectivity=1 (hub) */
  nodeMax: RGB;
  /** Cluster palette — each cluster gets a distinct base color */
  palette: RGB[];
  /** Edge color at weight=0 (weak) */
  edgeMin: RGB;
  /** Edge color at weight=1 (strong) */
  edgeMax: RGB;
  /** Cluster label text color */
  labelColor: string;
  /** Cluster label pill background */
  labelBg: string;
  /** Cluster label pill border */
  labelBorder: string;
  /** Sigma node label color */
  nodeLabelColor: string;
}

function lerp(a: RGB, b: RGB, t: number): string {
  const r = Math.round(a[0] + (b[0] - a[0]) * t);
  const g = Math.round(a[1] + (b[1] - a[1]) * t);
  const bl = Math.round(a[2] + (b[2] - a[2]) * t);
  return `rgb(${r},${g},${bl})`;
}

/** Brighten/dim a color by connectivity: 0.6× at low, 1.0× at high */
function modulate(base: RGB, connectivity: number): string {
  const factor = 0.6 + connectivity * 0.4;
  return `rgb(${Math.round(base[0] * factor)},${Math.round(base[1] * factor)},${Math.round(base[2] * factor)})`;
}

export function nodeColor(theme: CanvasTheme, connectivity: number, clusterIndex?: number): string {
  if (clusterIndex !== undefined && theme.palette.length > 0) {
    return modulate(theme.palette[clusterIndex % theme.palette.length], connectivity);
  }
  return lerp(theme.nodeMin, theme.nodeMax, connectivity);
}

export function edgeColor(theme: CanvasTheme, weight: number): string {
  return lerp(theme.edgeMin, theme.edgeMax, weight);
}

export const CANVAS_THEMES: CanvasTheme[] = [
  {
    id: 'ember',
    name: 'Ember',
    background: '#1a1816',
    nodeMin: [170, 110, 70],
    nodeMax: [230, 50, 40],
    palette: [
      [230, 80, 50],   // vermilion
      [220, 160, 60],  // amber
      [180, 60, 90],   // crimson
      [240, 120, 40],  // tangerine
      [160, 90, 140],  // dusty mauve
      [200, 140, 80],  // caramel
      [220, 60, 120],  // hot pink
      [170, 120, 50],  // bronze
    ],
    edgeMin: [45, 30, 25],
    edgeMax: [160, 60, 40],
    labelColor: 'rgb(200, 175, 155)',
    labelBg: 'rgb(24, 20, 18)',
    labelBorder: 'rgba(140, 100, 70, 0.3)',
    nodeLabelColor: '#b0a090',
  },
  {
    id: 'steel-violet',
    name: 'Steel Violet',
    background: '#1a1a1a',
    nodeMin: [100, 115, 175],
    nodeMax: [130, 50, 230],
    palette: [
      [140, 80, 220],  // violet
      [80, 140, 210],  // steel blue
      [180, 70, 160],  // magenta
      [90, 170, 180],  // teal
      [200, 100, 120], // rose
      [110, 120, 200], // periwinkle
      [160, 140, 80],  // muted gold
      [100, 180, 130], // sage
    ],
    edgeMin: [30, 30, 45],
    edgeMax: [80, 65, 160],
    labelColor: 'rgb(160, 175, 200)',
    labelBg: 'rgb(22, 22, 22)',
    labelBorder: 'rgba(80, 100, 140, 0.3)',
    nodeLabelColor: '#8899b0',
  },
  {
    id: 'aurora',
    name: 'Aurora',
    background: '#141a1a',
    nodeMin: [60, 160, 140],
    nodeMax: [140, 60, 220],
    palette: [
      [60, 190, 160],  // seafoam
      [140, 80, 210],  // purple
      [80, 160, 220],  // sky
      [200, 90, 140],  // pink
      [100, 200, 100], // green
      [180, 140, 60],  // gold
      [70, 130, 200],  // cobalt
      [190, 120, 180], // orchid
    ],
    edgeMin: [25, 40, 38],
    edgeMax: [70, 100, 150],
    labelColor: 'rgb(155, 200, 190)',
    labelBg: 'rgb(18, 22, 22)',
    labelBorder: 'rgba(70, 140, 130, 0.3)',
    nodeLabelColor: '#88b0a8',
  },
  {
    id: 'midnight',
    name: 'Midnight',
    background: '#12141c',
    nodeMin: [70, 90, 150],
    nodeMax: [100, 140, 255],
    palette: [
      [90, 130, 240],  // bright blue
      [60, 180, 190],  // cyan
      [150, 90, 220],  // lavender
      [80, 190, 140],  // mint
      [180, 80, 160],  // plum
      [100, 160, 210], // cerulean
      [200, 130, 80],  // warm amber
      [120, 100, 220], // indigo
    ],
    edgeMin: [25, 28, 50],
    edgeMax: [55, 80, 170],
    labelColor: 'rgb(150, 170, 210)',
    labelBg: 'rgb(16, 18, 26)',
    labelBorder: 'rgba(70, 90, 150, 0.3)',
    nodeLabelColor: '#7088b0',
  },
  {
    id: 'monochrome',
    name: 'Mono',
    background: '#181818',
    nodeMin: [100, 100, 100],
    nodeMax: [220, 220, 220],
    palette: [
      [180, 180, 180], // light grey
      [140, 140, 140], // mid grey
      [200, 200, 200], // near white
      [120, 120, 120], // dark grey
      [160, 160, 160], // medium
      [190, 190, 190], // soft white
      [130, 130, 130], // charcoal
      [170, 170, 170], // silver
    ],
    edgeMin: [35, 35, 35],
    edgeMax: [100, 100, 100],
    labelColor: 'rgb(180, 180, 180)',
    labelBg: 'rgb(20, 20, 20)',
    labelBorder: 'rgba(100, 100, 100, 0.3)',
    nodeLabelColor: '#909090',
  },
];

export const DEFAULT_THEME = CANVAS_THEMES.find(t => t.id === 'steel-violet')!;
