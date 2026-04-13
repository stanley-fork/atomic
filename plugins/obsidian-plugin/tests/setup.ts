// Ensure HTMLElement prototype augmentation runs before any test code loads.
import "./__mocks__/obsidian";

// Polyfill crypto.subtle for happy-dom if missing.
import { webcrypto } from "node:crypto";
if (!globalThis.crypto || !globalThis.crypto.subtle) {
  // @ts-expect-error assigning node webcrypto
  globalThis.crypto = webcrypto;
}
