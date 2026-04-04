import { App, PluginSettingTab, Setting, Notice } from "obsidian";
import type AtomicPlugin from "./main";

export interface AtomicSettings {
  serverUrl: string;
  authToken: string;
  vaultName: string;
  autoSync: boolean;
  syncDebounceMs: number;
  excludePatterns: string[];
  syncFolderTags: boolean;
  deleteOnRemove: boolean;
}

export const DEFAULT_SETTINGS: AtomicSettings = {
  serverUrl: "http://localhost:8080",
  authToken: "",
  vaultName: "",
  autoSync: false,
  syncDebounceMs: 2000,
  excludePatterns: [".obsidian/**", ".trash/**", ".git/**", "node_modules/**"],
  syncFolderTags: true,
  deleteOnRemove: false,
};

export class AtomicSettingTab extends PluginSettingTab {
  plugin: AtomicPlugin;

  constructor(app: App, plugin: AtomicPlugin) {
    super(app, plugin);
    this.plugin = plugin;
  }

  display(): void {
    const { containerEl } = this;
    containerEl.empty();

    containerEl.createEl("h2", { text: "Atomic Settings" });

    // Connection
    containerEl.createEl("h3", { text: "Connection" });

    new Setting(containerEl)
      .setName("Server URL")
      .setDesc("URL of your atomic-server instance")
      .addText((text) =>
        text
          .setPlaceholder("http://localhost:8080")
          .setValue(this.plugin.settings.serverUrl)
          .onChange(async (value) => {
            this.plugin.settings.serverUrl = value;
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName("Auth Token")
      .setDesc("Bearer token for API authentication")
      .addText((text) => {
        text
          .setPlaceholder("Enter your API token")
          .setValue(this.plugin.settings.authToken)
          .onChange(async (value) => {
            this.plugin.settings.authToken = value;
            await this.plugin.saveSettings();
          });
        text.inputEl.type = "password";
      });

    new Setting(containerEl)
      .setName("Test Connection")
      .setDesc("Verify that the server is reachable and the token is valid")
      .addButton((button) =>
        button.setButtonText("Test").onClick(async () => {
          try {
            await this.plugin.client.testConnection();
            new Notice("Connected to Atomic server successfully!");
          } catch (e) {
            new Notice(`Connection failed: ${e instanceof Error ? e.message : String(e)}`);
          }
        })
      );

    // Sync
    containerEl.createEl("h3", { text: "Sync" });

    new Setting(containerEl)
      .setName("Vault Name")
      .setDesc("Identifier used in source URLs (defaults to vault name)")
      .addText((text) =>
        text
          .setPlaceholder(this.app.vault.getName())
          .setValue(this.plugin.settings.vaultName)
          .onChange(async (value) => {
            this.plugin.settings.vaultName = value;
            await this.plugin.saveSettings();
          })
      );

    new Setting(containerEl)
      .setName("Auto Sync")
      .setDesc("Automatically sync notes when they change")
      .addToggle((toggle) =>
        toggle.setValue(this.plugin.settings.autoSync).onChange(async (value) => {
          this.plugin.settings.autoSync = value;
          await this.plugin.saveSettings();
          if (value) {
            this.plugin.syncEngine.startWatching();
          } else {
            this.plugin.syncEngine.stopWatching();
          }
        })
      );

    new Setting(containerEl)
      .setName("Sync Debounce (ms)")
      .setDesc("Wait this long after the last edit before syncing (default: 2000)")
      .addText((text) =>
        text
          .setPlaceholder("2000")
          .setValue(String(this.plugin.settings.syncDebounceMs))
          .onChange(async (value) => {
            const num = parseInt(value, 10);
            if (!isNaN(num) && num >= 500) {
              this.plugin.settings.syncDebounceMs = num;
              await this.plugin.saveSettings();
            }
          })
      );

    new Setting(containerEl)
      .setName("Sync Folder Tags")
      .setDesc("Create hierarchical tags from folder structure")
      .addToggle((toggle) =>
        toggle.setValue(this.plugin.settings.syncFolderTags).onChange(async (value) => {
          this.plugin.settings.syncFolderTags = value;
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("Delete on Remove")
      .setDesc("Delete atoms from Atomic when the note is deleted in Obsidian")
      .addToggle((toggle) =>
        toggle.setValue(this.plugin.settings.deleteOnRemove).onChange(async (value) => {
          this.plugin.settings.deleteOnRemove = value;
          await this.plugin.saveSettings();
        })
      );

    new Setting(containerEl)
      .setName("Exclude Patterns")
      .setDesc("Glob patterns to exclude from sync, one per line")
      .addTextArea((text) =>
        text
          .setPlaceholder(".obsidian/**\n.trash/**")
          .setValue(this.plugin.settings.excludePatterns.join("\n"))
          .onChange(async (value) => {
            this.plugin.settings.excludePatterns = value
              .split("\n")
              .map((s) => s.trim())
              .filter((s) => s.length > 0);
            await this.plugin.saveSettings();
          })
      );
  }
}
