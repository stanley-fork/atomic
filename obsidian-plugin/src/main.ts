import { Plugin, WorkspaceLeaf } from "obsidian";
import { AtomicClient } from "./atomic-client";
import { AtomicSettingTab, DEFAULT_SETTINGS, type AtomicSettings } from "./settings";
import { SyncEngine } from "./sync-engine";
import { SyncState, type SyncStateData } from "./sync-state";
import { SearchModal } from "./search-modal";
import { SimilarView, SIMILAR_VIEW_TYPE } from "./similar-view";
import { WikiView, WIKI_VIEW_TYPE } from "./wiki-view";

interface PluginData {
  settings: AtomicSettings;
  syncState?: SyncStateData;
}

export default class AtomicPlugin extends Plugin {
  settings: AtomicSettings = DEFAULT_SETTINGS;
  client: AtomicClient = new AtomicClient(this.settings);
  syncEngine!: SyncEngine;
  private syncState: SyncState = new SyncState();

  async onload(): Promise<void> {
    await this.loadSettings();
    this.client = new AtomicClient(this.settings);

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

    // Register sidebar views
    this.registerView(SIMILAR_VIEW_TYPE, (leaf) => {
      const syncState = SyncState.fromJSON(this.syncEngine.getSyncStateData());
      return new SimilarView(leaf, this.client, () => syncState);
    });

    this.registerView(WIKI_VIEW_TYPE, (leaf) => new WikiView(leaf, this.client));

    // Auto-sync if enabled
    if (this.settings.autoSync) {
      this.syncEngine.startWatching();
    }

    // Status bar
    const statusEl = this.addStatusBarItem();
    statusEl.addClass("atomic-status");
    statusEl.createSpan({ cls: "atomic-status-dot connected" });
    statusEl.createSpan({ text: "Atomic" });
  }

  async onunload(): Promise<void> {
    this.syncEngine.stopWatching();
  }

  async loadSettings(): Promise<void> {
    const data: PluginData | null = await this.loadData();
    this.settings = Object.assign({}, DEFAULT_SETTINGS, data?.settings);
  }

  async saveSettings(): Promise<void> {
    await this.savePluginData();
  }

  private async savePluginData(): Promise<void> {
    const data: PluginData = {
      settings: this.settings,
      syncState: this.syncEngine.getSyncStateData(),
    };
    await this.saveData(data);
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
