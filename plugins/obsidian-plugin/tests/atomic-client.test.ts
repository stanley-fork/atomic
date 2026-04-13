import { describe, it, expect, beforeEach, vi } from "vitest";
import { requestUrl } from "obsidian";
import { AtomicClient } from "../src/atomic-client";
import { DEFAULT_SETTINGS, type AtomicSettings } from "../src/settings";

const mockRequestUrl = requestUrl as unknown as ReturnType<typeof vi.fn>;

function makeSettings(overrides: Partial<AtomicSettings> = {}): AtomicSettings {
  return { ...DEFAULT_SETTINGS, authToken: "test-token", ...overrides };
}

function okResponse(json: unknown, status = 200) {
  return { status, json, text: JSON.stringify(json), headers: {}, arrayBuffer: new ArrayBuffer(0) };
}

beforeEach(() => {
  mockRequestUrl.mockReset();
});

describe("AtomicClient baseUrl / headers", () => {
  it("strips trailing slashes from serverUrl", async () => {
    mockRequestUrl.mockResolvedValue(okResponse({}));
    const c = new AtomicClient(makeSettings({ serverUrl: "http://host:8080///" }));
    await c.testConnection();
    const call = mockRequestUrl.mock.calls[0][0];
    expect(call.url).toBe("http://host:8080/api/settings");
  });

  it("sends Authorization bearer on every request", async () => {
    mockRequestUrl.mockResolvedValue(okResponse({}));
    const c = new AtomicClient(makeSettings({ authToken: "secret-123" }));
    await c.testConnection();
    const call = mockRequestUrl.mock.calls[0][0];
    expect(call.headers.Authorization).toBe("Bearer secret-123");
    expect(call.headers["Content-Type"]).toBe("application/json");
  });

  it("sends X-Atomic-Database only when databaseName is set", async () => {
    mockRequestUrl.mockResolvedValue(okResponse({}));
    const c1 = new AtomicClient(makeSettings({ databaseName: "" }));
    await c1.testConnection();
    expect(mockRequestUrl.mock.calls[0][0].headers["X-Atomic-Database"]).toBeUndefined();

    mockRequestUrl.mockClear();
    const c2 = new AtomicClient(makeSettings({ databaseName: "notes" }));
    await c2.testConnection();
    expect(mockRequestUrl.mock.calls[0][0].headers["X-Atomic-Database"]).toBe("notes");
  });
});

describe("AtomicClient error handling", () => {
  it("throws with server-provided error message on 4xx", async () => {
    mockRequestUrl.mockResolvedValue({
      status: 401,
      json: { error: "Unauthorized" },
      text: "",
      headers: {},
      arrayBuffer: new ArrayBuffer(0),
    });
    const c = new AtomicClient(makeSettings());
    await expect(c.testConnection()).rejects.toThrow("Unauthorized");
  });

  it("throws with HTTP status on 5xx without error field", async () => {
    mockRequestUrl.mockResolvedValue({
      status: 503,
      json: {},
      text: "",
      headers: {},
      arrayBuffer: new ArrayBuffer(0),
    });
    const c = new AtomicClient(makeSettings());
    await expect(c.testConnection()).rejects.toThrow(/503/);
  });

  it("propagates network errors from requestUrl", async () => {
    mockRequestUrl.mockRejectedValue(new Error("ECONNREFUSED"));
    const c = new AtomicClient(makeSettings());
    await expect(c.testConnection()).rejects.toThrow("ECONNREFUSED");
  });
});

describe("AtomicClient CRUD", () => {
  it("createAtom POSTs to /api/atoms with body", async () => {
    mockRequestUrl.mockResolvedValue(okResponse({ id: "a1", tags: [] }));
    const c = new AtomicClient(makeSettings());
    const result = await c.createAtom({ content: "hi", source_url: "obsidian://v/f.md" });
    const call = mockRequestUrl.mock.calls[0][0];
    expect(call.method).toBe("POST");
    expect(call.url).toMatch(/\/api\/atoms$/);
    expect(JSON.parse(call.body)).toEqual({ content: "hi", source_url: "obsidian://v/f.md" });
    expect(result.id).toBe("a1");
  });

  it("updateAtom PUTs to /api/atoms/:id", async () => {
    mockRequestUrl.mockResolvedValue(okResponse({ id: "a1", tags: [] }));
    const c = new AtomicClient(makeSettings());
    await c.updateAtom("a1", { content: "new" });
    const call = mockRequestUrl.mock.calls[0][0];
    expect(call.method).toBe("PUT");
    expect(call.url).toMatch(/\/api\/atoms\/a1$/);
  });

  it("deleteAtom DELETEs /api/atoms/:id", async () => {
    mockRequestUrl.mockResolvedValue(okResponse({}));
    const c = new AtomicClient(makeSettings());
    await c.deleteAtom("a1");
    expect(mockRequestUrl.mock.calls[0][0].method).toBe("DELETE");
  });

  it("getAtom GETs /api/atoms/:id", async () => {
    mockRequestUrl.mockResolvedValue(okResponse({ id: "a1", tags: [] }));
    const c = new AtomicClient(makeSettings());
    const result = await c.getAtom("a1");
    expect(result.id).toBe("a1");
    expect(mockRequestUrl.mock.calls[0][0].method).toBe("GET");
  });

  it("getAtomBySourceUrl returns null on 404", async () => {
    mockRequestUrl.mockResolvedValue({
      status: 404,
      json: { error: "No atom found" },
      text: "",
      headers: {},
      arrayBuffer: new ArrayBuffer(0),
    });
    const c = new AtomicClient(makeSettings());
    const result = await c.getAtomBySourceUrl("obsidian://v/f.md");
    expect(result).toBeNull();
  });

  it("getAtomBySourceUrl rethrows non-404 errors", async () => {
    mockRequestUrl.mockResolvedValue({
      status: 500,
      json: { error: "boom" },
      text: "",
      headers: {},
      arrayBuffer: new ArrayBuffer(0),
    });
    const c = new AtomicClient(makeSettings());
    await expect(c.getAtomBySourceUrl("x")).rejects.toThrow("boom");
  });

  it("getAtomBySourceUrl url-encodes the url param", async () => {
    mockRequestUrl.mockResolvedValue(okResponse({ id: "a" }));
    const c = new AtomicClient(makeSettings());
    await c.getAtomBySourceUrl("obsidian://v/a b.md");
    const call = mockRequestUrl.mock.calls[0][0];
    expect(call.url).toContain("url=obsidian%3A%2F%2Fv%2Fa%20b.md");
  });

  it("bulkCreateAtoms sends array body and returns result", async () => {
    const payload = { atoms: [{ id: "a1", source_url: "u1", tags: [] }], count: 1, skipped: 0 };
    mockRequestUrl.mockResolvedValue(okResponse(payload));
    const c = new AtomicClient(makeSettings());
    const res = await c.bulkCreateAtoms([
      { content: "x", source_url: "u1", skip_if_source_exists: true },
    ]);
    const call = mockRequestUrl.mock.calls[0][0];
    expect(call.url).toMatch(/\/api\/atoms\/bulk$/);
    const body = JSON.parse(call.body);
    expect(Array.isArray(body)).toBe(true);
    expect(body[0].skip_if_source_exists).toBe(true);
    expect(res.count).toBe(1);
  });
});

describe("AtomicClient search / wiki", () => {
  it("search POSTs with query/mode/limit", async () => {
    mockRequestUrl.mockResolvedValue(okResponse([]));
    const c = new AtomicClient(makeSettings());
    await c.search("hello", "semantic", 5);
    const body = JSON.parse(mockRequestUrl.mock.calls[0][0].body);
    expect(body).toEqual({ query: "hello", mode: "semantic", limit: 5 });
  });

  it("getWikiArticle returns null on any error", async () => {
    mockRequestUrl.mockRejectedValue(new Error("nope"));
    const c = new AtomicClient(makeSettings());
    const result = await c.getWikiArticle("tag-1");
    expect(result).toBeNull();
  });
});
