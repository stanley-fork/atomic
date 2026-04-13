import { vi } from "vitest";

// ---- Augment HTMLElement with Obsidian's DOM helpers ----
// Obsidian extends HTMLElement with createEl / createDiv / createSpan / empty / addClass / removeClass.
// happy-dom doesn't provide these; stub them for tests that render DOM.
function augmentEl(proto: any) {
  if (proto.__atomicAugmented) return;
  proto.__atomicAugmented = true;

  proto.createEl = function (tag: string, opts?: any) {
    const el = this.ownerDocument.createElement(tag);
    if (opts) {
      if (opts.text) el.textContent = opts.text;
      if (opts.cls) el.className = Array.isArray(opts.cls) ? opts.cls.join(" ") : opts.cls;
      if (opts.attr) for (const [k, v] of Object.entries(opts.attr)) el.setAttribute(k, String(v));
    }
    this.appendChild(el);
    return el;
  };
  proto.createDiv = function (opts?: any) {
    return proto.createEl.call(this, "div", opts);
  };
  proto.createSpan = function (opts?: any) {
    return proto.createEl.call(this, "span", opts);
  };
  proto.empty = function () {
    while (this.firstChild) this.removeChild(this.firstChild);
  };
  proto.addClass = function (...classes: string[]) {
    for (const c of classes) if (c) this.classList.add(c);
  };
  proto.removeClass = function (...classes: string[]) {
    for (const c of classes) if (c) this.classList.remove(c);
  };
  proto.toggleClass = function (c: string, on: boolean) {
    this.classList.toggle(c, on);
  };
  proto.setText = function (t: string) {
    this.textContent = t;
  };
}

if (typeof HTMLElement !== "undefined") {
  augmentEl(HTMLElement.prototype);
}

// ---- Core classes ----

export class App {
  vault: Vault;
  workspace: Workspace;
  metadataCache: { getFileCache: ReturnType<typeof vi.fn> };

  constructor() {
    this.vault = new Vault();
    this.workspace = new Workspace();
    this.metadataCache = { getFileCache: vi.fn(() => null) };
  }
}

export class Vault {
  private files: TFile[] = [];
  private _name = "TestVault";
  getName = vi.fn(() => this._name);
  getMarkdownFiles = vi.fn(() => this.files);
  read = vi.fn(async (_f: TFile) => "");
  on = vi.fn((_evt: string, _cb: any) => ({ id: Math.random() }));
  offref = vi.fn();

  _setFiles(files: TFile[]) {
    this.files = files;
  }
}

export class Workspace {
  getActiveFile = vi.fn(() => null as TFile | null);
  getLeavesOfType = vi.fn(() => []);
  getRightLeaf = vi.fn();
  revealLeaf = vi.fn();
  detachLeavesOfType = vi.fn();
  on = vi.fn();
}

export class TAbstractFile {
  path: string;
  name: string;
  constructor(path: string) {
    this.path = path;
    this.name = path.split("/").pop() ?? path;
  }
}

export class TFile extends TAbstractFile {
  extension: string;
  basename: string;
  constructor(path: string) {
    super(path);
    const dot = this.name.lastIndexOf(".");
    this.extension = dot >= 0 ? this.name.slice(dot + 1) : "";
    this.basename = dot >= 0 ? this.name.slice(0, dot) : this.name;
  }
}

export class TFolder extends TAbstractFile {
  children: TAbstractFile[] = [];
}

export class Plugin {
  app: App;
  manifest: any;
  constructor(app: App, manifest: any) {
    this.app = app;
    this.manifest = manifest;
  }
  addCommand = vi.fn();
  addRibbonIcon = vi.fn();
  addSettingTab = vi.fn();
  registerView = vi.fn();
  registerEvent = vi.fn();
  loadData = vi.fn(async () => ({}));
  saveData = vi.fn(async () => {});
  onload() {}
  onunload() {}
}

export class PluginSettingTab {
  app: App;
  plugin: any;
  containerEl: HTMLElement;
  constructor(app: App, plugin: any) {
    this.app = app;
    this.plugin = plugin;
    this.containerEl = document.createElement("div");
  }
  display() {}
  hide() {}
}

export class Modal {
  app: App;
  contentEl: HTMLElement;
  modalEl: HTMLElement;
  titleEl: HTMLElement;
  constructor(app: App) {
    this.app = app;
    this.modalEl = document.createElement("div");
    this.contentEl = document.createElement("div");
    this.titleEl = document.createElement("div");
    this.modalEl.appendChild(this.contentEl);
  }
  open() {
    (this as any).onOpen?.();
  }
  close() {
    (this as any).onClose?.();
  }
  onOpen() {}
  onClose() {}
}

export class Notice {
  static instances: Notice[] = [];
  message: string;
  constructor(message: string, _timeout?: number) {
    this.message = message;
    Notice.instances.push(this);
  }
  hide() {}
}

// Chainable Setting mock. Captures callbacks so tests can invoke them.
export class Setting {
  containerEl: HTMLElement;
  settingEl: HTMLElement;
  components: any[] = [];
  constructor(containerEl: HTMLElement) {
    this.containerEl = containerEl;
    this.settingEl = document.createElement("div");
    containerEl.appendChild(this.settingEl);
  }
  setName(_: string) { return this; }
  setDesc(_: string) { return this; }
  addText(cb: (t: any) => void) {
    const input = document.createElement("input");
    const t = {
      inputEl: input,
      _value: "",
      _onChange: null as any,
      setPlaceholder(_: string) { return this; },
      setValue(v: string) { this._value = v; input.value = v; return this; },
      getValue() { return this._value; },
      onChange(fn: any) { this._onChange = fn; return this; },
    };
    cb(t);
    this.components.push(t);
    return this;
  }
  addTextArea(cb: (t: any) => void) { return this.addText(cb); }
  addToggle(cb: (t: any) => void) {
    const t = {
      _value: false,
      _onChange: null as any,
      setValue(v: boolean) { this._value = v; return this; },
      getValue() { return this._value; },
      onChange(fn: any) { this._onChange = fn; return this; },
    };
    cb(t);
    this.components.push(t);
    return this;
  }
  addButton(cb: (b: any) => void) {
    const btn = document.createElement("button");
    const b = {
      buttonEl: btn,
      _onClick: null as any,
      setButtonText(text: string) { btn.textContent = text; return this; },
      setCta() { return this; },
      setIcon(_: string) { return this; },
      onClick(fn: any) { this._onClick = fn; btn.addEventListener("click", fn); return this; },
    };
    cb(b);
    this.components.push(b);
    return this;
  }
  addDropdown(cb: (d: any) => void) {
    const d = {
      _value: "",
      _onChange: null as any,
      _options: {} as Record<string, string>,
      addOption(k: string, v: string) { this._options[k] = v; return this; },
      addOptions(o: any) { Object.assign(this._options, o); return this; },
      setValue(v: string) { this._value = v; return this; },
      onChange(fn: any) { this._onChange = fn; return this; },
    };
    cb(d);
    this.components.push(d);
    return this;
  }
}

export class ItemView {
  leaf: WorkspaceLeaf;
  containerEl: HTMLElement;
  contentEl: HTMLElement;
  constructor(leaf: WorkspaceLeaf) {
    this.leaf = leaf;
    this.containerEl = document.createElement("div");
    const child1 = document.createElement("div");
    this.contentEl = document.createElement("div");
    this.containerEl.appendChild(child1);
    this.containerEl.appendChild(this.contentEl);
  }
  getViewType() { return ""; }
  getDisplayText() { return ""; }
  getIcon() { return ""; }
  onOpen() { return Promise.resolve(); }
  onClose() { return Promise.resolve(); }
  registerEvent = vi.fn();
}

export class WorkspaceLeaf {
  view: any;
  setViewState = vi.fn(async () => {});
  getViewState = vi.fn(() => ({}));
  detach = vi.fn();
}

export class SuggestModal<T> extends Modal {
  inputEl: HTMLInputElement;
  constructor(app: App) {
    super(app);
    this.inputEl = document.createElement("input");
  }
  setPlaceholder(_: string) {}
  getSuggestions(_: string): T[] | Promise<T[]> { return []; }
  renderSuggestion(_v: T, _el: HTMLElement) {}
  onChooseSuggestion(_v: T, _evt: MouseEvent | KeyboardEvent) {}
}

export const MarkdownRenderer = {
  render: vi.fn(async () => {}),
  renderMarkdown: vi.fn(async () => {}),
};

export const setIcon = vi.fn();

// requestUrl — mutable mock so tests can override.
export const requestUrl = vi.fn(async (_params: RequestUrlParam) => ({
  status: 200,
  json: {},
  text: "",
  headers: {},
  arrayBuffer: new ArrayBuffer(0),
}));

export interface RequestUrlParam {
  url: string;
  method?: string;
  body?: string;
  headers?: Record<string, string>;
}

export type EventRef = { id: number };
