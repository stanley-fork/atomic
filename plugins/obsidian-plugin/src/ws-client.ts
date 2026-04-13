import type { AtomicSettings } from "./settings";

/**
 * ServerEvent types from atomic-server (wire tag is PascalCase on the `type` field).
 * We only declare the subset the Obsidian plugin currently listens for; other
 * events are passed through to subscribers that care.
 */
export type ServerEvent =
  | { type: "ChatStreamDelta"; conversation_id: string; content: string }
  | {
      type: "ChatToolStart";
      conversation_id: string;
      tool_call_id: string;
      tool_name: string;
      tool_input: unknown;
    }
  | {
      type: "ChatToolComplete";
      conversation_id: string;
      tool_call_id: string;
      results_count: number;
    }
  | { type: "ChatComplete"; conversation_id: string; message: unknown }
  | { type: "ChatError"; conversation_id: string; error: string }
  | { type: "EmbeddingComplete"; atom_id: string }
  | { type: "EmbeddingFailed"; atom_id: string; error: string }
  | {
      type: "TaggingComplete";
      atom_id: string;
      tags_extracted: number;
      new_tags_created: number;
    }
  | { type: "TaggingFailed"; atom_id: string; error: string }
  | { type: "TaggingSkipped"; atom_id: string }
  | { type: string; [key: string]: unknown };

export type EventHandler = (event: ServerEvent) => void;

/**
 * Thin WebSocket wrapper: connects to `{wsBase}/ws?token=…`, reconnects with
 * exponential backoff while `open()` has been called, and fans events out to
 * registered handlers. Handlers should filter by `event.type` themselves.
 */
export class AtomicWebSocket {
  private settings: AtomicSettings;
  private socket: WebSocket | null = null;
  private handlers = new Set<EventHandler>();
  private wantOpen = false;
  private reconnectAttempt = 0;
  private reconnectTimer: number | null = null;

  constructor(settings: AtomicSettings) {
    this.settings = settings;
  }

  updateSettings(settings: AtomicSettings): void {
    const reconnectNeeded =
      this.wantOpen &&
      (settings.serverUrl !== this.settings.serverUrl ||
        settings.authToken !== this.settings.authToken);
    this.settings = settings;
    if (reconnectNeeded) {
      this.close();
      this.open();
    }
  }

  on(handler: EventHandler): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  open(): void {
    this.wantOpen = true;
    if (this.socket && this.socket.readyState <= WebSocket.OPEN) return;
    this.connect();
  }

  close(): void {
    this.wantOpen = false;
    if (this.reconnectTimer !== null) {
      window.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    if (this.socket) {
      this.socket.close();
      this.socket = null;
    }
  }

  private connect(): void {
    const url = this.buildUrl();
    if (!url) return;
    try {
      this.socket = new WebSocket(url);
    } catch (e) {
      console.error("[Atomic] WS construct failed:", e);
      this.scheduleReconnect();
      return;
    }

    this.socket.addEventListener("open", () => {
      this.reconnectAttempt = 0;
    });
    this.socket.addEventListener("message", (evt) => {
      let data: ServerEvent;
      try {
        data = JSON.parse(evt.data);
      } catch {
        return;
      }
      for (const handler of this.handlers) {
        try {
          handler(data);
        } catch (e) {
          console.error("[Atomic] WS handler threw:", e);
        }
      }
    });
    this.socket.addEventListener("close", () => {
      this.socket = null;
      if (this.wantOpen) this.scheduleReconnect();
    });
    this.socket.addEventListener("error", () => {
      // "close" fires after error; let that path handle reconnect.
    });
  }

  private scheduleReconnect(): void {
    if (!this.wantOpen || this.reconnectTimer !== null) return;
    // 500ms, 1s, 2s, 4s, 8s, 15s (cap) — matches what users tolerate when the
    // server is briefly restarting during dev.
    const delays = [500, 1000, 2000, 4000, 8000, 15000];
    const delay = delays[Math.min(this.reconnectAttempt, delays.length - 1)];
    this.reconnectAttempt++;
    this.reconnectTimer = window.setTimeout(() => {
      this.reconnectTimer = null;
      this.connect();
    }, delay);
  }

  private buildUrl(): string | null {
    const base = this.settings.serverUrl.replace(/\/+$/, "");
    if (!base || !this.settings.authToken) return null;
    const wsBase = base.replace(/^http:/i, "ws:").replace(/^https:/i, "wss:");
    return `${wsBase}/ws?token=${encodeURIComponent(this.settings.authToken)}`;
  }
}
