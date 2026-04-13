import { ItemView, Notice, TFile, WorkspaceLeaf } from "obsidian";
import Graph from "graphology";
import Sigma from "sigma";
import EdgeCurveProgram from "@sigma/edge-curve";
import type AtomicPlugin from "./main";
import { CANVAS_THEMES, DEFAULT_THEME, edgeColor, nodeColor, type CanvasTheme } from "./canvas-themes";

export const CANVAS_VIEW_TYPE = "atomic-canvas";

/**
 * Full-tab Sigma canvas showing the knowledge graph filtered to the current
 * vault's atoms. Clicking a node opens the corresponding Obsidian note.
 *
 * Visuals mirror the Atomic desktop canvas: curved weight-colored edges,
 * cluster-palette node colors, centroid cluster-label pills, collision-checked
 * atom labels, dark-pill hover.
 */
export class CanvasView extends ItemView {
  private plugin: AtomicPlugin;
  private sigmaInstance: Sigma | null = null;
  private labelCanvas: HTMLCanvasElement | null = null;
  private resizeObserver: ResizeObserver | null = null;
  private theme: CanvasTheme = DEFAULT_THEME;
  private labeledNodeIds: Set<string> = new Set();

  constructor(leaf: WorkspaceLeaf, plugin: AtomicPlugin) {
    super(leaf);
    this.plugin = plugin;
  }

  getViewType(): string { return CANVAS_VIEW_TYPE; }
  getDisplayText(): string { return "Atomic Canvas"; }
  getIcon(): string { return "network"; }

  async onOpen(): Promise<void> {
    const container = this.containerEl.children[1] as HTMLElement;
    container.empty();
    container.addClass("atomic-canvas-root");
    container.style.backgroundColor = this.theme.background;

    const statusEl = container.createDiv({ cls: "atomic-canvas-status", text: "Loading canvas…" });

    try {
      if (!this.plugin.settings.authToken) {
        statusEl.textContent = "Connect Atomic first via the Setup Wizard.";
        return;
      }

      const vaultName = this.plugin.settings.vaultName || this.plugin.app.vault.getName();
      const sourcePrefix = `obsidian://${vaultName}/`;
      const data = await this.plugin.client.getCanvas(sourcePrefix);

      statusEl.remove();

      if (data.atoms.length === 0) {
        const emptyEl = container.createDiv({ cls: "atomic-canvas-status" });
        emptyEl.createDiv({ text: "No embedded notes found." });
        emptyEl.createDiv({
          cls: "atomic-canvas-status-sub",
          text: "Sync and wait for embedding to finish, then re-open the canvas.",
        });
        return;
      }

      const sigmaEl = container.createDiv({ cls: "atomic-canvas-sigma" });

      // --- Precompute edge counts + cluster index ---
      const edgeCounts = new Map<string, number>();
      for (const edge of data.edges) {
        edgeCounts.set(edge.source, (edgeCounts.get(edge.source) || 0) + 1);
        edgeCounts.set(edge.target, (edgeCounts.get(edge.target) || 0) + 1);
      }
      const maxEdges = Math.max(1, ...edgeCounts.values());

      const atomCluster = new Map<string, number>();
      for (let i = 0; i < data.clusters.length; i++) {
        for (const atomId of data.clusters[i].atom_ids) {
          atomCluster.set(atomId, i);
        }
      }

      // --- Build graph ---
      const graph = new Graph({ type: "undirected", multi: false });
      const scale = 500;

      for (const atom of data.atoms) {
        const connectivity = (edgeCounts.get(atom.atom_id) || 0) / maxEdges;
        const clusterIdx = atomCluster.get(atom.atom_id);
        graph.addNode(atom.atom_id, {
          x: atom.x * scale,
          y: atom.y * scale,
          size: 2.5 + connectivity * 5,
          color: nodeColor(this.theme, connectivity, clusterIdx),
          label: truncLabel(atom.title || atom.atom_id.substring(0, 8), 30),
          fullLabel: atom.title || atom.atom_id.substring(0, 8),
          connectivity,
          source_url: atom.source_url ?? null,
        });
      }

      // Normalize edge weights to [0,1]
      let minW = 1, maxW = 0;
      for (const edge of data.edges) {
        if (edge.weight < minW) minW = edge.weight;
        if (edge.weight > maxW) maxW = edge.weight;
      }
      const wRange = Math.max(maxW - minW, 0.001);

      const seen = new Set<string>();
      for (const edge of data.edges) {
        if (!graph.hasNode(edge.source) || !graph.hasNode(edge.target)) continue;
        const key = [edge.source, edge.target].sort().join("\0");
        if (seen.has(key)) continue;
        seen.add(key);
        const w = (edge.weight - minW) / wRange;
        graph.addEdge(edge.source, edge.target, { weight: w, type: "curved" });
      }

      // --- Sigma ---
      this.sigmaInstance = new Sigma(graph, sigmaEl, {
        renderLabels: false, // we draw labels manually on an overlay canvas
        defaultEdgeType: "curved",
        edgeProgramClasses: { curved: EdgeCurveProgram },
        minCameraRatio: 0.05,
        maxCameraRatio: 10,
        stagePadding: 40,
        defaultDrawNodeHover: (context, nodeData, settings) => {
          const size = nodeData.size || 4;
          const alreadyLabeled = (nodeData as { labeled?: boolean }).labeled === true;

          // Halo ring is always drawn
          context.beginPath();
          context.arc(nodeData.x, nodeData.y, size + 2, 0, Math.PI * 2);
          context.strokeStyle = "rgba(255, 255, 255, 0.3)";
          context.lineWidth = 1.5;
          context.stroke();

          // Skip the pill when the overlay already shows this node's label —
          // otherwise the dark pill lands on top and obscures it.
          if (alreadyLabeled) return;

          const label = (nodeData as { fullLabel?: string }).fullLabel || nodeData.label || "";
          if (!label) return;
          const font = settings.labelFont || "system-ui, sans-serif";
          const fontSize = 13;
          context.font = `${fontSize}px ${font}`;
          const textWidth = context.measureText(label).width;
          const padding = 6;
          const boxW = textWidth + padding * 2;
          const boxH = fontSize + padding * 2;
          const x = nodeData.x + size + 4;
          const y = nodeData.y - boxH / 2;

          context.fillStyle = "rgba(20, 20, 20, 0.92)";
          context.beginPath();
          context.roundRect(x, y, boxW, boxH, 4);
          context.fill();
          context.strokeStyle = "rgba(255, 255, 255, 0.1)";
          context.lineWidth = 0.5;
          context.stroke();

          context.fillStyle = "#d0d0d0";
          context.textAlign = "left";
          context.textBaseline = "middle";
          context.fillText(label, x + padding, nodeData.y);
        },
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        nodeReducer: (n, attrs) => ({ ...attrs, labeled: this.labeledNodeIds.has(n) } as any),
        edgeReducer: (_edge, attrs) => {
          const w = (attrs as { weight?: number }).weight ?? 0.5;
          return {
            ...attrs,
            color: edgeColor(this.theme, w),
            size: 0.2 + w * 0.7,
          };
        },
      });

      // Click node → open corresponding Obsidian note
      this.sigmaInstance.on("clickNode", ({ node }) => {
        const attrs = graph.getNodeAttributes(node);
        const sourceUrl = attrs.source_url as string | null;
        if (!sourceUrl) {
          new Notice("This atom has no source URL — it may have been created outside Obsidian.");
          return;
        }
        const match = sourceUrl.match(/^obsidian:\/\/[^/]+\/(.*)$/);
        if (!match) {
          new Notice(`Unsupported source URL: ${sourceUrl}`);
          return;
        }
        const filePath = match[1].split("/").map((s) => {
          try { return decodeURIComponent(s); } catch { return s; }
        }).join("/");
        const file = this.plugin.app.vault.getAbstractFileByPath(filePath);
        if (file instanceof TFile) {
          this.plugin.app.workspace.getLeaf(false).openFile(file);
        } else {
          new Notice(`Note not found in this vault: ${filePath}`);
        }
      });

      // --- Overlay canvas for cluster + atom labels ---
      this.labelCanvas = sigmaEl.createEl("canvas", { cls: "atomic-canvas-labels" });

      const drawLabels = () => this.drawLabels(graph, sigmaEl, data.clusters);
      this.sigmaInstance.on("afterRender", drawLabels);
      requestAnimationFrame(drawLabels);

      this.resizeObserver = new ResizeObserver(() => {
        this.sigmaInstance?.refresh();
        drawLabels();
      });
      this.resizeObserver.observe(sigmaEl);

      // --- Theme picker + info strip ---
      this.renderControls(container, () => {
        // Re-color all nodes + edges, re-bg, redraw
        container.style.backgroundColor = this.theme.background;
        graph.forEachNode((id, attrs) => {
          const connectivity = attrs.connectivity as number;
          const clusterIdx = atomCluster.get(id);
          graph.setNodeAttribute(id, "color", nodeColor(this.theme, connectivity, clusterIdx));
        });
        this.sigmaInstance?.refresh();
        drawLabels();
      });

      container.createDiv({
        cls: "atomic-canvas-info",
        text: `${data.atoms.length} notes · ${data.edges.length} connections · ${data.clusters.length} clusters`,
      });
    } catch (e) {
      statusEl.textContent = `Failed to load canvas: ${e instanceof Error ? e.message : String(e)}`;
      statusEl.addClass("atomic-canvas-status-error");
    }
  }

  private renderControls(container: HTMLElement, onThemeChange: () => void): void {
    const controls = container.createDiv({ cls: "atomic-canvas-controls" });
    for (const theme of CANVAS_THEMES) {
      const swatch = controls.createEl("button", {
        cls: "atomic-canvas-swatch",
        attr: { "aria-label": theme.name, title: theme.name },
      });
      swatch.style.background = `rgb(${theme.palette[0].join(",")})`;
      if (theme.id === this.theme.id) swatch.addClass("active");
      swatch.addEventListener("click", () => {
        this.theme = theme;
        controls.querySelectorAll(".atomic-canvas-swatch").forEach((el) => el.removeClass("active"));
        swatch.addClass("active");
        onThemeChange();
      });
    }
  }

  private drawLabels(
    graph: Graph,
    sigmaEl: HTMLElement,
    clusters: { label: string; atom_count: number; atom_ids: string[] }[]
  ): void {
    const canvas = this.labelCanvas;
    const sigma = this.sigmaInstance;
    if (!canvas || !sigma) return;

    const width = sigmaEl.clientWidth;
    const height = sigmaEl.clientHeight;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = width * ratio;
    canvas.height = height * ratio;
    canvas.style.width = `${width}px`;
    canvas.style.height = `${height}px`;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
    ctx.clearRect(0, 0, width, height);

    const t = this.theme;
    const placed: { x: number; y: number; w: number; h: number }[] = [];
    const collides = (r: { x: number; y: number; w: number; h: number }, pad: number) => {
      for (const p of placed) {
        if (
          r.x - pad < p.x + p.w &&
          r.x + r.w + pad > p.x &&
          r.y - pad < p.y + p.h &&
          r.y + r.h + pad > p.y
        ) return true;
      }
      return false;
    };

    // Cluster pills (priority)
    const clusterFontSize = 13;
    ctx.font = `600 ${clusterFontSize}px system-ui, -apple-system, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";

    const sortedClusters = [...clusters].sort((a, b) => b.atom_count - a.atom_count);
    const maxClusterLabels = Math.max(4, Math.floor((width * height) / 40000));
    const clusterPad = 24;
    let clusterCount = 0;

    for (const cluster of sortedClusters) {
      if (clusterCount >= maxClusterLabels) break;
      let cx = 0, cy = 0, count = 0;
      for (const atomId of cluster.atom_ids) {
        if (!graph.hasNode(atomId)) continue;
        cx += graph.getNodeAttribute(atomId, "x") as number;
        cy += graph.getNodeAttribute(atomId, "y") as number;
        count++;
      }
      if (count === 0) continue;
      cx /= count; cy /= count;
      const pos = sigma.graphToViewport({ x: cx, y: cy });
      const labelY = pos.y - 20;
      const metrics = ctx.measureText(cluster.label);
      const pillW = metrics.width + 16;
      const pillH = clusterFontSize + 8;
      const rect = { x: pos.x - pillW / 2, y: labelY - pillH / 2, w: pillW, h: pillH };

      if (collides(rect, clusterPad)) continue;
      placed.push(rect);
      clusterCount++;

      ctx.fillStyle = t.labelBg;
      ctx.beginPath();
      ctx.roundRect(rect.x, rect.y, pillW, pillH, pillH / 2);
      ctx.fill();
      ctx.strokeStyle = t.labelBorder;
      ctx.lineWidth = 1;
      ctx.stroke();

      ctx.fillStyle = t.labelColor;
      ctx.fillText(cluster.label, pos.x, labelY);
    }

    // Atom labels (collision-checked)
    const atomFontSize = 12;
    ctx.font = `${atomFontSize}px system-ui, -apple-system, sans-serif`;
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";

    const minRenderedSize = 4;
    const atomLabelPad = 20;

    type Cand = { id: string; vx: number; vy: number; rsize: number; label: string };
    const candidates: Cand[] = [];
    graph.forEachNode((id, attrs) => {
      const rsize = sigma.scaleSize(attrs.size as number);
      if (rsize < minRenderedSize) return;
      const pos = sigma.graphToViewport({ x: attrs.x as number, y: attrs.y as number });
      if (pos.x < -200 || pos.x > width + 50 || pos.y < -30 || pos.y > height + 30) return;
      const label = (attrs.label as string) || "";
      if (!label) return;
      candidates.push({ id, vx: pos.x, vy: pos.y, rsize, label });
    });
    candidates.sort((a, b) => b.rsize - a.rsize);

    const nextLabeled = new Set<string>();
    ctx.fillStyle = t.nodeLabelColor;
    for (const c of candidates) {
      const tw = ctx.measureText(c.label).width;
      const lx = c.vx + c.rsize + 4;
      const ly = c.vy;
      const rect = { x: lx, y: ly - atomFontSize / 2, w: tw, h: atomFontSize };
      if (collides(rect, atomLabelPad)) continue;
      placed.push(rect);
      nextLabeled.add(c.id);
      ctx.fillText(c.label, lx, ly);
    }
    this.labeledNodeIds = nextLabeled;
  }

  async onClose(): Promise<void> {
    this.resizeObserver?.disconnect();
    this.resizeObserver = null;
    this.sigmaInstance?.kill();
    this.sigmaInstance = null;
    this.labelCanvas = null;
  }
}

function truncLabel(s: string, max: number): string {
  return s.length <= max ? s : s.slice(0, max - 1) + "…";
}
