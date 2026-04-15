import { Capacitor, registerPlugin } from '@capacitor/core';

interface SharedConfigPlugin {
  set(config: { serverURL?: string; apiToken?: string; databaseId?: string }): Promise<void>;
  clear(): Promise<void>;
}

const plugin = registerPlugin<SharedConfigPlugin>('SharedConfig');

// The native side writes these keys to UserDefaults(suiteName: "group.com.atomic.mobile"),
// where the ShareExtension reads them synchronously when the user taps Atomic
// in the iOS share sheet. No-op on web and Android builds.

export async function syncSharedConfig(config: {
  serverURL?: string | null;
  apiToken?: string | null;
  databaseId?: string | null;
}): Promise<void> {
  if (!Capacitor.isNativePlatform() || Capacitor.getPlatform() !== 'ios') return;
  try {
    await plugin.set({
      serverURL: config.serverURL ?? undefined,
      apiToken: config.apiToken ?? undefined,
      databaseId: config.databaseId ?? undefined,
    });
  } catch (err) {
    console.warn('SharedConfig.set failed:', err);
  }
}

export async function clearSharedConfig(): Promise<void> {
  if (!Capacitor.isNativePlatform() || Capacitor.getPlatform() !== 'ios') return;
  try {
    await plugin.clear();
  } catch (err) {
    console.warn('SharedConfig.clear failed:', err);
  }
}
