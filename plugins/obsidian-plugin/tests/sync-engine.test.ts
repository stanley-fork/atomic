import { describe, it, expect, beforeEach, vi } from "vitest";
import { App, TFile, Notice } from "obsidian";
import { SyncEngine } from "../src/sync-engine";
import { DEFAULT_SETTINGS, type AtomicSettings } from "../src/settings";

function makeClientMock() {
  return {
    createAtom: vi.fn(async (req: any) => ({ id: "new-atom", source_url: req.source_url, tags: [] })),
    updateAtom: vi.fn(async (id: string, req: any) => ({ id, source_url: req.source_url, tags: [] })),
    deleteAtom: vi.fn(async () => {}),
    getAtomBySourceUrl: vi.fn(async () => null),
    bulkCreateAtoms: vi.fn(async (atoms: any[]) => ({
      atoms: atoms.map((a, i) => ({ id: `bulk-${i}`, source_url: a.source_url, tags: [] })),
      count: atoms.length,
      skipped: 0,
    })),
  };
}

function makeSettings(overrides: Partial<AtomicSettings> = {}): AtomicSettings {
  return { ...DEFAULT_SETTINGS, authToken: "t", ...overrides };
}

function makeFile(path: string): TFile {
  return new TFile(path);
}

describe("SyncEngine.shouldExclude", () => {
  function engine(patterns: string[]) {
    const app = new App();
    const client = makeClientMock() as any;
    const e = new SyncEngine(app, client, makeSettings({ excludePatterns: patterns }), undefined, async () => {});
    return e as any;
  }

  it("excludes via ** globstar", () => {
    const e = engine([".obsidian/**"]);
    expect(e.shouldExclude(".obsidian/workspace")).toBe(true);
    expect(e.shouldExclude(".obsidian/plugins/x/main.js")).toBe(true);
  });

  it("does not exclude unmatched paths", () => {
    const e = engine([".obsidian/**"]);
    expect(e.shouldExclude("notes/hello.md")).toBe(false);
  });

  it("* matches single segment", () => {
    const e = engine(["drafts/*.md"]);
    expect(e.shouldExclude("drafts/one.md")).toBe(true);
    expect(e.shouldExclude("drafts/nested/one.md")).toBe(false);
  });

  it("multiple patterns all apply", () => {
    const e = engine([".obsidian/**", "**/private/**"]);
    expect(e.shouldExclude(".obsidian/x")).toBe(true);
    expect(e.shouldExclude("a/private/b.md")).toBe(true);
    expect(e.shouldExclude("public/b.md")).toBe(false);
  });
});

describe("SyncEngine.syncFile", () => {
  let app: App;
  let client: ReturnType<typeof makeClientMock>;
  let engine: SyncEngine;
  let saveState: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    app = new App();
    client = makeClientMock();
    saveState = vi.fn(async () => {});
    engine = new SyncEngine(app, client as any, makeSettings(), undefined, saveState);
  });

  it("creates a new atom when unseen", async () => {
    const file = makeFile("notes/a.md");
    app.vault.read = vi.fn(async () => "hello world");
    await engine.syncFile(file);
    expect(client.createAtom).toHaveBeenCalledOnce();
    expect(client.updateAtom).not.toHaveBeenCalled();
    expect(engine.getSyncStateData().files["notes/a.md"]?.atomId).toBe("new-atom");
    expect(saveState).toHaveBeenCalled();
  });

  it("skips unchanged file (hash match)", async () => {
    const file = makeFile("a.md");
    app.vault.read = vi.fn(async () => "same");
    await engine.syncFile(file);
    client.createAtom.mockClear();
    client.updateAtom.mockClear();

    // Re-sync — same content, same hash
    await engine.syncFile(file);
    expect(client.createAtom).not.toHaveBeenCalled();
    expect(client.updateAtom).not.toHaveBeenCalled();
  });

  it("updates existing atom when content changes", async () => {
    const file = makeFile("a.md");
    app.vault.read = vi.fn(async () => "v1");
    await engine.syncFile(file);

    app.vault.read = vi.fn(async () => "v2");
    client.updateAtom.mockClear();
    await engine.syncFile(file);
    expect(client.updateAtom).toHaveBeenCalledOnce();
    expect(client.updateAtom.mock.calls[0][0]).toBe("new-atom");
  });

  it("adopts server-side atom if source_url already exists", async () => {
    const file = makeFile("a.md");
    app.vault.read = vi.fn(async () => "content");
    client.getAtomBySourceUrl.mockResolvedValueOnce({
      id: "server-atom",
      source_url: "x",
      tags: [],
    } as any);
    await engine.syncFile(file);
    expect(client.updateAtom).toHaveBeenCalledWith(
      "server-atom",
      expect.objectContaining({ content: "content" })
    );
    expect(client.createAtom).not.toHaveBeenCalled();
    expect(engine.getSyncStateData().files["a.md"]?.atomId).toBe("server-atom");
  });

  it("uses vaultName from settings in source_url when provided", async () => {
    engine = new SyncEngine(
      app,
      client as any,
      makeSettings({ vaultName: "CustomVault" }),
      undefined,
      saveState
    );
    const file = makeFile("folder/a.md");
    app.vault.read = vi.fn(async () => "x");
    await engine.syncFile(file);
    expect(client.createAtom.mock.calls[0][0].source_url).toBe("obsidian://CustomVault/folder/a.md");
  });

  it("propagates errors from client", async () => {
    const file = makeFile("a.md");
    app.vault.read = vi.fn(async () => "x");
    client.createAtom.mockRejectedValueOnce(new Error("boom"));
    await expect(engine.syncFile(file)).rejects.toThrow("boom");
  });
});

describe("SyncEngine debounce (file events)", () => {
  it("coalesces multiple modify events within debounce window", async () => {
    vi.useFakeTimers();
    const app = new App();
    const client = makeClientMock();
    const engine = new SyncEngine(
      app,
      client as any,
      makeSettings({ syncDebounceMs: 1000 }),
      undefined,
      async () => {}
    );
    const file = makeFile("a.md");
    app.vault.read = vi.fn(async () => "hello");

    engine.startWatching();
    // Manually invoke the handler that would fire from vault.on('modify', ...)
    const handler = (app.vault.on as any).mock.calls.find((c: any) => c[0] === "modify")?.[1];
    expect(handler).toBeTypeOf("function");

    handler(file);
    handler(file);
    handler(file);

    await vi.advanceTimersByTimeAsync(1100);
    // Allow microtasks for async syncFile
    await vi.runAllTimersAsync();

    expect(client.createAtom).toHaveBeenCalledTimes(1);
    vi.useRealTimers();
  });
});

describe("SyncEngine.syncAll", () => {
  it("fires progress callbacks and returns final summary", async () => {
    const app = new App();
    const client = makeClientMock();
    const engine = new SyncEngine(app, client as any, makeSettings(), undefined, async () => {});
    const files = [makeFile("a.md"), makeFile("b.md")];
    (app.vault as any)._setFiles(files);
    app.vault.read = vi.fn(async (f: TFile) => `content of ${f.path}`);

    const progressEvents: any[] = [];
    const result = await engine.syncAll((p) => progressEvents.push({ ...p }));

    expect(progressEvents.length).toBeGreaterThanOrEqual(2);
    expect(progressEvents[0].phase).toBe("reading");
    expect(result.phase).toBe("complete");
    expect(result.totalFiles).toBe(2);
    expect(result.created).toBe(2);
    expect(result.atomIds.length).toBe(2);
    expect(client.bulkCreateAtoms).toHaveBeenCalledOnce();
  });

  it("counts skipped unchanged files", async () => {
    const app = new App();
    const client = makeClientMock();
    const engine = new SyncEngine(app, client as any, makeSettings(), undefined, async () => {});
    const files = [makeFile("a.md")];
    (app.vault as any)._setFiles(files);
    app.vault.read = vi.fn(async () => "same");

    await engine.syncAll();
    client.bulkCreateAtoms.mockClear();
    const second = await engine.syncAll();
    expect(second.skipped).toBe(1);
    expect(second.created).toBe(0);
    expect(client.bulkCreateAtoms).not.toHaveBeenCalled();
  });

  it("chunks large batches under byte cap", async () => {
    const app = new App();
    const client = makeClientMock();
    const engine = new SyncEngine(app, client as any, makeSettings(), undefined, async () => {});
    // Two files, each ~1MB — should split into two batches (cap 1.5MB).
    const big = "x".repeat(1_000_000);
    const files = [makeFile("a.md"), makeFile("b.md")];
    (app.vault as any)._setFiles(files);
    app.vault.read = vi.fn(async () => big);

    await engine.syncAll();
    expect(client.bulkCreateAtoms).toHaveBeenCalledTimes(2);
  });

  it("handles bulk failure by incrementing error count", async () => {
    const app = new App();
    const client = makeClientMock();
    client.bulkCreateAtoms.mockRejectedValue(new Error("server down"));
    const engine = new SyncEngine(app, client as any, makeSettings(), undefined, async () => {});
    const files = [makeFile("a.md"), makeFile("b.md")];
    (app.vault as any)._setFiles(files);
    app.vault.read = vi.fn(async () => "x");

    const result = await engine.syncAll();
    expect(result.errors).toBe(2);
    expect(result.created).toBe(0);
  });
});

describe("SyncEngine delete / rename", () => {
  it("syncCurrentFile emits Notice when no active file", async () => {
    const before = Notice.instances.length;
    const app = new App();
    const engine = new SyncEngine(app, makeClientMock() as any, makeSettings(), undefined, async () => {});
    await engine.syncCurrentFile();
    expect(Notice.instances.length).toBeGreaterThan(before);
    expect(Notice.instances[Notice.instances.length - 1].message).toMatch(/No active file/);
  });
});
