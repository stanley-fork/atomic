import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

interface SettingsStore {
  settings: Record<string, string>;
  isLoading: boolean;
  error: string | null;
  
  fetchSettings: () => Promise<void>;
  setSetting: (key: string, value: string) => Promise<void>;
  testOpenRouterConnection: (apiKey: string) => Promise<boolean>;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  settings: {},
  isLoading: false,
  error: null,
  
  fetchSettings: async () => {
    set({ isLoading: true, error: null });
    try {
      const settings = await invoke<Record<string, string>>('get_settings');
      set({ settings, isLoading: false });
    } catch (e) {
      set({ error: String(e), isLoading: false });
    }
  },
  
  setSetting: async (key: string, value: string) => {
    try {
      await invoke('set_setting', { key, value });
      set((state) => ({
        settings: { ...state.settings, [key]: value }
      }));
    } catch (e) {
      set({ error: String(e) });
      throw e;
    }
  },
  
  testOpenRouterConnection: async (apiKey: string) => {
    const result = await invoke<boolean>('test_openrouter_connection', { apiKey });
    return result;
  },
}));

