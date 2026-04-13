import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { AtomicWebSocket } from "../src/ws-client";
import { DEFAULT_SETTINGS } from "../src/settings";

// Minimal WebSocket stub
class FakeWebSocket {
  static instances: FakeWebSocket[] = [];
  static CONNECTING = 0;
  static OPEN = 1;
  static CLOSING = 2;
  static CLOSED = 3;
  readyState = FakeWebSocket.CONNECTING;
  url: string;
  listeners: Record<string, Array<(e: any) => void>> = {};
  constructor(url: string) {
    this.url = url;
    FakeWebSocket.instances.push(this);
  }
  addEventListener(evt: string, cb: (e: any) => void) {
    (this.listeners[evt] ||= []).push(cb);
  }
  close = vi.fn(() => {
    this.readyState = FakeWebSocket.CLOSED;
    for (const cb of this.listeners.close ?? []) cb({});
  });
  fireOpen() {
    this.readyState = FakeWebSocket.OPEN;
    for (const cb of this.listeners.open ?? []) cb({});
  }
  fireMessage(data: string) {
    for (const cb of this.listeners.message ?? []) cb({ data });
  }
  fireClose() {
    this.readyState = FakeWebSocket.CLOSED;
    for (const cb of this.listeners.close ?? []) cb({});
  }
  fireError() {
    for (const cb of this.listeners.error ?? []) cb({});
  }
}

beforeEach(() => {
  FakeWebSocket.instances = [];
  (globalThis as any).WebSocket = FakeWebSocket;
});

afterEach(() => {
  vi.useRealTimers();
});

describe("AtomicWebSocket connection", () => {
  it("connects to ws URL with encoded token", () => {
    const ws = new AtomicWebSocket({
      ...DEFAULT_SETTINGS,
      serverUrl: "http://host:8080/",
      authToken: "tok+special/1",
    });
    ws.open();
    expect(FakeWebSocket.instances.length).toBe(1);
    const inst = FakeWebSocket.instances[0];
    expect(inst.url).toMatch(/^ws:\/\/host:8080\/ws\?token=/);
    expect(inst.url).toContain(encodeURIComponent("tok+special/1"));
  });

  it("uses wss when serverUrl is https", () => {
    const ws = new AtomicWebSocket({
      ...DEFAULT_SETTINGS,
      serverUrl: "https://host:8443",
      authToken: "t",
    });
    ws.open();
    expect(FakeWebSocket.instances[0].url).toMatch(/^wss:\/\//);
  });

  it("does not connect when token or url is missing", () => {
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, serverUrl: "", authToken: "" });
    ws.open();
    expect(FakeWebSocket.instances.length).toBe(0);
  });
});

describe("AtomicWebSocket messaging", () => {
  it("delivers parsed messages to subscribers", () => {
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    const inst = FakeWebSocket.instances[0];
    const handler = vi.fn();
    ws.on(handler);
    inst.fireMessage(JSON.stringify({ type: "EmbeddingComplete", atom_id: "a1" }));
    expect(handler).toHaveBeenCalledWith({ type: "EmbeddingComplete", atom_id: "a1" });
  });

  it("tolerates malformed JSON without throwing", () => {
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    const handler = vi.fn();
    ws.on(handler);
    expect(() => FakeWebSocket.instances[0].fireMessage("not json{")).not.toThrow();
    expect(handler).not.toHaveBeenCalled();
  });

  it("unsubscribe fn stops delivery", () => {
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    const handler = vi.fn();
    const unsub = ws.on(handler);
    unsub();
    FakeWebSocket.instances[0].fireMessage(JSON.stringify({ type: "X" }));
    expect(handler).not.toHaveBeenCalled();
  });

  it("isolates handler exceptions", () => {
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    const bad = vi.fn(() => { throw new Error("nope"); });
    const good = vi.fn();
    ws.on(bad);
    ws.on(good);
    FakeWebSocket.instances[0].fireMessage(JSON.stringify({ type: "Y" }));
    expect(good).toHaveBeenCalled();
  });
});

describe("AtomicWebSocket reconnect", () => {
  it("schedules reconnect with backoff after close", () => {
    vi.useFakeTimers();
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    const first = FakeWebSocket.instances[0];
    first.fireClose();

    // Should schedule a reconnect at 500ms
    vi.advanceTimersByTime(499);
    expect(FakeWebSocket.instances.length).toBe(1);
    vi.advanceTimersByTime(2);
    expect(FakeWebSocket.instances.length).toBe(2);

    // Second close → 1000ms delay
    FakeWebSocket.instances[1].fireClose();
    vi.advanceTimersByTime(999);
    expect(FakeWebSocket.instances.length).toBe(2);
    vi.advanceTimersByTime(2);
    expect(FakeWebSocket.instances.length).toBe(3);
  });

  it("close() stops reconnect attempts", () => {
    vi.useFakeTimers();
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    FakeWebSocket.instances[0].fireClose();
    ws.close();
    vi.advanceTimersByTime(10_000);
    expect(FakeWebSocket.instances.length).toBe(1);
  });

  it("reconnectAttempt resets on successful open", () => {
    vi.useFakeTimers();
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    FakeWebSocket.instances[0].fireClose();
    vi.advanceTimersByTime(600);
    // Second socket — fire open then close and verify first-tier (500ms) delay is used again
    FakeWebSocket.instances[1].fireOpen();
    FakeWebSocket.instances[1].fireClose();
    vi.advanceTimersByTime(499);
    expect(FakeWebSocket.instances.length).toBe(2);
    vi.advanceTimersByTime(2);
    expect(FakeWebSocket.instances.length).toBe(3);
  });
});

describe("AtomicWebSocket.updateSettings", () => {
  it("reconnects when URL changes while open", () => {
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    const first = FakeWebSocket.instances[0];
    ws.updateSettings({ ...DEFAULT_SETTINGS, serverUrl: "http://other:9000", authToken: "t" });
    expect(first.close).toHaveBeenCalled();
    expect(FakeWebSocket.instances.length).toBe(2);
    expect(FakeWebSocket.instances[1].url).toContain("other:9000");
  });

  it("does not reconnect if settings unchanged", () => {
    const ws = new AtomicWebSocket({ ...DEFAULT_SETTINGS, authToken: "t" });
    ws.open();
    ws.updateSettings({ ...DEFAULT_SETTINGS, authToken: "t" });
    expect(FakeWebSocket.instances.length).toBe(1);
  });
});
