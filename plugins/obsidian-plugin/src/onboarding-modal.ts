import { App, Modal, Setting, setIcon } from "obsidian";
import type AtomicPlugin from "./main";
import type { SyncProgress } from "./sync-engine";

const STEPS = ["Welcome", "Connect", "Index", "Done"] as const;

export class OnboardingModal extends Modal {
  private plugin: AtomicPlugin;
  private currentStep = 0;
  private connectionVerified = false;
  private syncResult: SyncProgress | null = null;
  private _wsUnsubscribe: (() => void) | null = null;
  private _modalClosed = false;

  constructor(app: App, plugin: AtomicPlugin) {
    super(app);
    this.plugin = plugin;
  }

  onOpen(): void {
    this.modalEl.addClass("atomic-onboarding-modal");
    this.render();
  }

  onClose(): void {
    this._modalClosed = true;
    this._wsUnsubscribe?.();
    this._wsUnsubscribe = null;
    this.contentEl.empty();
  }

  private render(): void {
    this.contentEl.empty();

    const wrapper = this.contentEl.createDiv({ cls: "atomic-onboarding" });

    // Step indicator
    this.renderStepIndicator(wrapper);

    // Step content
    const body = wrapper.createDiv({ cls: "atomic-onboarding-body" });
    switch (this.currentStep) {
      case 0: this.renderWelcome(body); break;
      case 1: this.renderConnect(body); break;
      case 2: this.renderSync(body); break;
      case 3: this.renderDone(body); break;
    }
  }

  private renderStepIndicator(container: HTMLElement): void {
    const indicator = container.createDiv({ cls: "atomic-step-indicator" });

    for (let i = 0; i < STEPS.length; i++) {
      if (i > 0) {
        const connector = indicator.createDiv({ cls: "atomic-step-connector" });
        if (i <= this.currentStep) connector.addClass("completed");
      }

      const step = indicator.createDiv({ cls: "atomic-step" });
      const dot = step.createDiv({ cls: "atomic-step-dot" });

      if (i < this.currentStep) {
        dot.addClass("completed");
        setIcon(dot, "check");
      } else if (i === this.currentStep) {
        dot.addClass("active");
      }

      step.createDiv({ cls: "atomic-step-label", text: STEPS[i] });
    }
  }

  // --- Step 1: Welcome ---

  private renderWelcome(container: HTMLElement): void {
    container.createEl("h2", { text: "Welcome to Atomic" });
    container.createEl("p", {
      cls: "atomic-onboarding-desc",
      text: "Atomic turns your notes into a semantically-connected knowledge graph. This plugin syncs your vault to Atomic so you can:",
    });

    const features = container.createEl("ul", { cls: "atomic-onboarding-features" });
    const items = [
      ["Search semantically", "Find notes by meaning, not just keywords"],
      ["Discover connections", "See which notes are related based on content"],
      ["Generate wiki articles", "AI-synthesized summaries of topics across your notes"],
    ];
    for (const [title, desc] of items) {
      const li = features.createEl("li");
      li.createEl("strong", { text: title });
      li.createSpan({ text: ` \u2014 ${desc}` });
    }

    const footer = container.createDiv({ cls: "atomic-onboarding-footer" });
    const btn = footer.createEl("button", { text: "Get Started", cls: "mod-cta" });
    btn.addEventListener("click", () => { this.currentStep = 1; this.render(); });
  }

  // --- Step 2: Connect ---

  private renderConnect(container: HTMLElement): void {
    container.createEl("h2", { text: "Connect to your server" });
    container.createEl("p", {
      cls: "atomic-onboarding-desc",
      text: "Enter the URL and API token for your Atomic server.",
    });

    const form = container.createDiv({ cls: "atomic-onboarding-form" });

    new Setting(form)
      .setName("Server URL")
      .addText((text) =>
        text
          .setPlaceholder("http://localhost:8080")
          .setValue(this.plugin.settings.serverUrl)
          .onChange((value) => {
            this.plugin.settings.serverUrl = value;
            this.connectionVerified = false;
            this.updateConnectButtons();
          })
      );

    new Setting(form)
      .setName("API Token")
      .addText((text) => {
        text
          .setPlaceholder("Enter your API token")
          .setValue(this.plugin.settings.authToken)
          .onChange((value) => {
            this.plugin.settings.authToken = value;
            this.connectionVerified = false;
            this.updateConnectButtons();
          });
        text.inputEl.type = "password";
      });

    new Setting(form)
      .setName("Database")
      .setDesc("Leave empty for default")
      .addText((text) =>
        text
          .setPlaceholder("default")
          .setValue(this.plugin.settings.databaseName)
          .onChange((value) => {
            this.plugin.settings.databaseName = value;
            this.connectionVerified = false;
            this.updateConnectButtons();
          })
      );

    // Test connection area
    const testArea = container.createDiv({ cls: "atomic-onboarding-test" });
    const testBtn = testArea.createEl("button", { text: "Test Connection" });
    const testStatus = testArea.createDiv({ cls: "atomic-onboarding-test-status" });

    testBtn.addEventListener("click", async () => {
      testBtn.disabled = true;
      testBtn.textContent = "Testing...";
      testStatus.empty();
      testStatus.removeClass("success", "error");

      // Update the client with current settings
      this.plugin.client = new (await import("./atomic-client")).AtomicClient(this.plugin.settings);

      try {
        await this.plugin.client.testConnection();
        this.connectionVerified = true;
        testStatus.addClass("success");
        testStatus.textContent = "Connected successfully";
        await this.plugin.saveSettings();
      } catch (e) {
        this.connectionVerified = false;
        testStatus.addClass("error");
        testStatus.textContent = e instanceof Error ? e.message : "Connection failed";
      }

      testBtn.disabled = false;
      testBtn.textContent = "Test Connection";
      this.updateConnectButtons();
    });

    // Footer
    const footer = container.createDiv({ cls: "atomic-onboarding-footer" });

    const backBtn = footer.createEl("button", { text: "Back" });
    backBtn.addEventListener("click", () => { this.currentStep = 0; this.render(); });

    const nextBtn = footer.createEl("button", { text: "Next", cls: "mod-cta" });
    nextBtn.disabled = !this.connectionVerified;
    nextBtn.addEventListener("click", () => { this.currentStep = 2; this.render(); });

    // Store refs for updating button state
    this._connectNextBtn = nextBtn;
  }

  private _connectNextBtn: HTMLButtonElement | null = null;

  private updateConnectButtons(): void {
    if (this._connectNextBtn) {
      this._connectNextBtn.disabled = !this.connectionVerified;
    }
  }

  // --- Step 3: Index Vault ---

  private renderSync(container: HTMLElement): void {
    container.createEl("h2", { text: "Index your vault" });

    const files = this.app.vault.getMarkdownFiles().filter(
      (f) => !this.plugin.syncEngine["shouldExclude"](f.path)
    );

    container.createEl("p", {
      cls: "atomic-onboarding-desc",
      text: `Found ${files.length} markdown files. Indexing sends them to Atomic for embedding and semantic analysis.`,
    });

    // Phase 1: file upload progress (hidden initially)
    const progressArea = container.createDiv({ cls: "atomic-sync-progress hidden" });
    const barOuter = progressArea.createDiv({ cls: "atomic-progress-bar" });
    const barFill = barOuter.createDiv({ cls: "atomic-progress-fill" });
    const progressLabel = progressArea.createDiv({ cls: "atomic-progress-label" });

    const stats = progressArea.createDiv({ cls: "atomic-sync-stats" });
    const statCreated = stats.createDiv({ cls: "atomic-sync-stat" });
    statCreated.createSpan({ cls: "atomic-sync-stat-value", text: "0" });
    statCreated.createSpan({ cls: "atomic-sync-stat-label", text: "created" });

    const statSkipped = stats.createDiv({ cls: "atomic-sync-stat" });
    statSkipped.createSpan({ cls: "atomic-sync-stat-value", text: "0" });
    statSkipped.createSpan({ cls: "atomic-sync-stat-label", text: "unchanged" });

    const statErrors = stats.createDiv({ cls: "atomic-sync-stat" });
    statErrors.createSpan({ cls: "atomic-sync-stat-value", text: "0" });
    statErrors.createSpan({ cls: "atomic-sync-stat-label", text: "errors" });

    // Phase 2: AI processing (hidden until upload completes)
    const aiArea = container.createDiv({ cls: "atomic-ai-progress hidden" });
    aiArea.createDiv({ cls: "atomic-ai-title", text: "AI Processing" });

    const embedRow = aiArea.createDiv({ cls: "atomic-ai-row" });
    embedRow.createSpan({ cls: "atomic-ai-row-label", text: "Embedding" });
    const embedBarOuter = embedRow.createDiv({ cls: "atomic-progress-bar atomic-ai-bar" });
    const embedBarFill = embedBarOuter.createDiv({ cls: "atomic-progress-fill" });
    const embedCount = embedRow.createSpan({ cls: "atomic-ai-row-count", text: "0 / 0" });

    const tagRow = aiArea.createDiv({ cls: "atomic-ai-row" });
    tagRow.createSpan({ cls: "atomic-ai-row-label", text: "Auto-tagging" });
    const tagBarOuter = tagRow.createDiv({ cls: "atomic-progress-bar atomic-ai-bar" });
    const tagBarFill = tagBarOuter.createDiv({ cls: "atomic-progress-fill" });
    const tagCount = tagRow.createSpan({ cls: "atomic-ai-row-count", text: "0 / 0" });

    aiArea.createDiv({
      cls: "atomic-ai-hint",
      text: "Features become available as notes are processed.",
    });

    // Buttons
    const footer = container.createDiv({ cls: "atomic-onboarding-footer" });

    const skipLink = footer.createEl("button", { cls: "atomic-skip-link", text: "Skip for now" });
    skipLink.addEventListener("click", () => { this.currentStep = 3; this.render(); });

    const startBtn = footer.createEl("button", { text: "Start Indexing", cls: "mod-cta" });

    startBtn.addEventListener("click", async () => {
      startBtn.disabled = true;
      startBtn.textContent = "Indexing...";
      skipLink.addClass("hidden");
      progressArea.removeClass("hidden");

      const onProgress = (p: SyncProgress) => {
        const pct = p.totalFiles > 0 ? Math.round((p.processed / p.totalFiles) * 100) : 0;
        barFill.style.width = `${pct}%`;

        if (p.phase === "reading") {
          progressLabel.textContent = "Reading files...";
        } else if (p.phase === "syncing") {
          progressLabel.textContent = `${p.processed} of ${p.totalFiles} files`;
        } else {
          progressLabel.textContent = "Upload complete";
        }

        statCreated.querySelector(".atomic-sync-stat-value")!.textContent = String(p.created);
        statSkipped.querySelector(".atomic-sync-stat-value")!.textContent = String(p.skipped);
        statErrors.querySelector(".atomic-sync-stat-value")!.textContent = String(p.errors);
      };

      try {
        this.syncResult = await this.plugin.syncEngine.syncAll(onProgress);
      } catch (e) {
        console.error("Atomic: Sync failed during onboarding:", e);
      }

      const atomIds = this.syncResult?.atomIds ?? [];

      if (atomIds.length === 0) {
        // Nothing to process — advance after a brief pause
        setTimeout(() => {
          if (!this._modalClosed) { this.currentStep = 3; this.render(); }
        }, 1000);
        return;
      }

      // --- Phase 2: track embedding + tagging via WS + reconciliation ---

      barFill.style.width = "100%";
      aiArea.removeClass("hidden");

      const total = atomIds.length;
      const pendingEmbed = new Set(atomIds);
      const pendingTag = new Set(atomIds);
      let doneEmbed = 0;
      let doneTag = 0;

      embedCount.textContent = `0 / ${total}`;
      tagCount.textContent = `0 / ${total}`;

      let advanced = false;
      const advance = () => {
        if (advanced || this._modalClosed) return;
        advanced = true;
        this._wsUnsubscribe?.();
        this._wsUnsubscribe = null;
        this.currentStep = 3;
        this.render();
      };

      const updateBars = () => {
        const embedPct = (doneEmbed / total) * 100;
        const tagPct = (doneTag / total) * 100;
        embedBarFill.style.width = `${embedPct}%`;
        tagBarFill.style.width = `${tagPct}%`;
        embedCount.textContent = `${doneEmbed} / ${total}`;
        tagCount.textContent = `${doneTag} / ${total}`;
        if (pendingEmbed.size === 0 && pendingTag.size === 0) {
          setTimeout(advance, 800);
        }
      };

      // Subscribe to WS pipeline events, filtered to atoms from this sync
      this.plugin.ws.open();
      this._wsUnsubscribe = this.plugin.ws.on((evt) => {
        if (evt.type === "EmbeddingComplete" || evt.type === "EmbeddingFailed") {
          const id = (evt as { atom_id: string }).atom_id;
          if (pendingEmbed.delete(id)) { doneEmbed++; updateBars(); }
        } else if (
          evt.type === "TaggingComplete" ||
          evt.type === "TaggingFailed" ||
          evt.type === "TaggingSkipped"
        ) {
          const id = (evt as { atom_id: string }).atom_id;
          if (pendingTag.delete(id)) { doneTag++; updateBars(); }
        }
      });

      // Swap footer: remove "Start Indexing", add "Continue in background"
      startBtn.remove();
      const continueBtn = footer.createEl("button", {
        text: "Continue in background",
        cls: "mod-cta",
      });
      continueBtn.addEventListener("click", advance);

      // Reconcile: fetch current status for all atoms to catch anything that
      // completed in the window between bulk-create and WS subscription
      try {
        const CONCURRENCY = 8;
        for (let i = 0; i < atomIds.length; i += CONCURRENCY) {
          if (advanced || this._modalClosed) break;
          const batch = atomIds.slice(i, i + CONCURRENCY);
          await Promise.all(
            batch.map(async (id) => {
              try {
                const atom = await this.plugin.client.getAtom(id);
                const embDone =
                  atom.embedding_status === "complete" || atom.embedding_status === "failed";
                const tagDone =
                  atom.tagging_status === "complete" ||
                  atom.tagging_status === "failed" ||
                  atom.tagging_status === "skipped";
                if (embDone && pendingEmbed.delete(id)) doneEmbed++;
                if (tagDone && pendingTag.delete(id)) doneTag++;
              } catch {
                // leave as pending — WS will fire when it completes
              }
            })
          );
          updateBars();
        }
      } catch (e) {
        console.error("Atomic: Reconciliation failed during onboarding:", e);
      }
    });
  }

  // --- Step 4: Done ---

  private renderDone(container: HTMLElement): void {
    container.createEl("h2", { text: "You're all set!" });

    if (this.syncResult) {
      const { created, updated, skipped, errors } = this.syncResult;
      const parts: string[] = [];
      if (created > 0) parts.push(`${created} notes indexed`);
      if (updated > 0) parts.push(`${updated} updated`);
      if (skipped > 0) parts.push(`${skipped} unchanged`);
      if (errors > 0) parts.push(`${errors} errors`);
      container.createEl("p", {
        cls: "atomic-onboarding-desc",
        text: parts.join(", ") + ".",
      });
    } else {
      container.createEl("p", {
        cls: "atomic-onboarding-desc",
        text: "Your vault is connected. Notes will sync automatically as you edit them.",
      });
    }

    const cards = container.createDiv({ cls: "atomic-feature-cards" });

    this.renderFeatureCard(cards, {
      icon: "search",
      title: "Semantic Search",
      desc: "Search by meaning across your entire vault.",
      shortcut: "Cmd+P \u2192 Atomic: Semantic Search",
    });

    this.renderFeatureCard(cards, {
      icon: "arrow-left-right",
      title: "Similar Notes",
      desc: "See related notes in the sidebar as you work.",
      shortcut: "Cmd+P \u2192 Atomic: Open Similar Notes",
    });

    this.renderFeatureCard(cards, {
      icon: "book-open",
      title: "Wiki Articles",
      desc: "AI-generated summaries of topics in your notes.",
      shortcut: "Cmd+P \u2192 Atomic: Open Wiki",
    });

    const footer = container.createDiv({ cls: "atomic-onboarding-footer" });
    const finishBtn = footer.createEl("button", { text: "Finish", cls: "mod-cta" });
    finishBtn.addEventListener("click", async () => {
      await this.plugin.saveSettings();
      if (this.plugin.settings.autoSync) {
        this.plugin.syncEngine.startWatching();
      }
      this.close();
    });
  }

  private renderFeatureCard(
    container: HTMLElement,
    opts: { icon: string; title: string; desc: string; shortcut: string }
  ): void {
    const card = container.createDiv({ cls: "atomic-feature-card" });
    const iconEl = card.createDiv({ cls: "atomic-feature-icon" });
    setIcon(iconEl, opts.icon);
    const text = card.createDiv({ cls: "atomic-feature-text" });
    text.createDiv({ cls: "atomic-feature-title", text: opts.title });
    text.createDiv({ cls: "atomic-feature-desc", text: opts.desc });
    text.createEl("code", { cls: "atomic-feature-shortcut", text: opts.shortcut });
  }
}
