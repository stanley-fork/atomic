export interface FileSyncInfo {
  atomId: string;
  contentHash: string;
  lastSynced: number;
}

export interface SyncStateData {
  files: Record<string, FileSyncInfo>;
}

export class SyncState {
  private data: SyncStateData;

  constructor(data?: SyncStateData) {
    this.data = data ?? { files: {} };
  }

  getFile(path: string): FileSyncInfo | undefined {
    return this.data.files[path];
  }

  setFile(path: string, info: FileSyncInfo): void {
    this.data.files[path] = info;
  }

  removeFile(path: string): void {
    delete this.data.files[path];
  }

  renameFile(oldPath: string, newPath: string): void {
    const info = this.data.files[oldPath];
    if (info) {
      this.data.files[newPath] = info;
      delete this.data.files[oldPath];
    }
  }

  getAllPaths(): string[] {
    return Object.keys(this.data.files);
  }

  toJSON(): SyncStateData {
    return this.data;
  }

  static fromJSON(data: SyncStateData): SyncState {
    return new SyncState(data);
  }
}

/** Simple hash of a string using Web Crypto (SHA-256, hex-encoded). */
export async function hashContent(content: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(content);
  const hashBuffer = await crypto.subtle.digest("SHA-256", data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map((b) => b.toString(16).padStart(2, "0")).join("");
}
