import { Plugin, WorkspaceLeaf } from "obsidian";
import { AtomicClient } from "./atomic-client";
import { AtomicSettingTab, DEFAULT_SETTINGS, type AtomicSettings } from "./settings";
import { SyncEngine } from "./sync-engine";
import { SyncState, type SyncStateData } from "./sync-state";
import { SearchModal } from "./search-modal";
import { SimilarView, SIMILAR_VIEW_TYPE } from "./similar-view";
import { WikiView, WIKI_VIEW_TYPE } from "./wiki-view";
import { ChatView, CHAT_VIEW_TYPE } from "./chat-view";
import { AtomicWebSocket } from "./ws-client";
import { OnboardingModal } from "./onboarding-modal";
import { CanvasView, CANVAS_VIEW_TYPE } from "./canvas-view";

interface PluginData {
  settings: AtomicSettings;
  syncState?: SyncStateData;
}

export default class AtomicPlugin extends Plugin {
  settings: AtomicSettings = DEFAULT_SETTINGS;
  client: AtomicClient = new AtomicClient(this.settings);
  ws: AtomicWebSocket = new AtomicWebSocket(this.settings);
  syncEngine!: SyncEngine;
  private syncState: SyncState = new SyncState();

  async onload(): Promise<void> {
    await this.loadSettings();
    this.client = new AtomicClient(this.settings);
    this.ws = new AtomicWebSocket(this.settings);

    this.syncEngine = new SyncEngine(
      this.app,
      this.client,
      this.settings,
      (await this.loadData())?.syncState,
      () => this.savePluginData()
    );

    this.addSettingTab(new AtomicSettingTab(this.app, this));

    // Commands
    this.addCommand({
      id: "semantic-search",
      name: "Semantic Search",
      callback: () => new SearchModal(this.app, this.client).open(),
    });

    this.addCommand({
      id: "sync-current-note",
      name: "Sync Current Note",
      callback: () => this.syncEngine.syncCurrentFile(),
    });

    this.addCommand({
      id: "sync-vault",
      name: "Sync Entire Vault",
      callback: () => this.syncEngine.syncAll(),
    });

    this.addCommand({
      id: "toggle-auto-sync",
      name: "Toggle Auto Sync",
      callback: () => this.syncEngine.toggleAutoSync(),
    });

    this.addCommand({
      id: "open-similar-notes",
      name: "Open Similar Notes",
      callback: () => this.activateView(SIMILAR_VIEW_TYPE),
    });

    this.addCommand({
      id: "open-wiki",
      name: "Open Wiki",
      callback: () => this.activateView(WIKI_VIEW_TYPE),
    });

    this.addCommand({
      id: "open-chat",
      name: "Open Chat",
      callback: () => this.activateView(CHAT_VIEW_TYPE),
    });

    this.addCommand({
      id: "open-canvas",
      name: "Open Knowledge Graph Canvas",
      callback: () => this.activateCanvasView(),
    });

    this.addCommand({
      id: "setup-wizard",
      name: "Setup Wizard",
      callback: () => new OnboardingModal(this.app, this).open(),
    });

    // Register sidebar views
    this.registerView(SIMILAR_VIEW_TYPE, (leaf) => {
      const syncState = SyncState.fromJSON(this.syncEngine.getSyncStateData());
      return new SimilarView(leaf, this.client, () => syncState);
    });

    this.registerView(WIKI_VIEW_TYPE, (leaf) => new WikiView(
      leaf,
      this.client,
      () => this.settings.vaultName || this.app.vault.getName(),
    ));

    this.registerView(CHAT_VIEW_TYPE, (leaf) => new ChatView(
      leaf,
      this.client,
      this.ws,
      () => this.settings.vaultName || this.app.vault.getName(),
    ));

    this.registerView(CANVAS_VIEW_TYPE, (leaf) => new CanvasView(leaf, this));

    // Auto-sync if enabled (skip if not yet configured)
    if (this.settings.authToken && this.settings.autoSync) {
      this.syncEngine.startWatching();
    }

    // First-run: open onboarding wizard if no auth token configured
    if (!this.settings.authToken) {
      setTimeout(() => {
        new OnboardingModal(this.app, this).open();
      }, 500);
    }

    // Status bar
    const statusEl = this.addStatusBarItem();
    statusEl.addClass("atomic-status");
    statusEl.createSpan({ cls: "atomic-status-dot connected" });
    statusEl.createSpan({ text: "Atomic" });
  }

  async onunload(): Promise<void> {
    this.syncEngine.stopWatching();
    this.ws.close();
  }

  async loadSettings(): Promise<void> {
    const data: PluginData | null = await this.loadData();
    this.settings = Object.assign({}, DEFAULT_SETTINGS, data?.settings);
  }

  async saveSettings(): Promise<void> {
    this.ws.updateSettings(this.settings);
    await this.savePluginData();
  }

  private async savePluginData(): Promise<void> {
    const data: PluginData = {
      settings: this.settings,
      syncState: this.syncEngine.getSyncStateData(),
    };
    await this.saveData(data);
  }

  private async activateCanvasView(): Promise<void> {
    const { workspace } = this.app;
    const existing = workspace.getLeavesOfType(CANVAS_VIEW_TYPE);
    if (existing.length > 0) {
      workspace.revealLeaf(existing[0]);
      return;
    }
    const leaf = workspace.getLeaf("tab");
    await leaf.setViewState({ type: CANVAS_VIEW_TYPE, active: true });
    workspace.revealLeaf(leaf);
  }

  private async activateView(viewType: string): Promise<void> {
    const { workspace } = this.app;

    let leaf: WorkspaceLeaf | null = null;
    const leaves = workspace.getLeavesOfType(viewType);

    if (leaves.length > 0) {
      leaf = leaves[0];
    } else {
      leaf = workspace.getRightLeaf(false);
      if (leaf) {
        await leaf.setViewState({ type: viewType, active: true });
      }
    }

    if (leaf) {
      workspace.revealLeaf(leaf);
    }
  }
}
