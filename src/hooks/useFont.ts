import { useEffect } from 'react';
import { useSettingsStore } from '../stores/settings';

export type Font = 'system' | 'satoshi' | 'ibm-plex-sans' | 'source-sans' | 'literata';

export const FONTS: { value: Font; label: string }[] = [
  { value: 'system', label: 'System Default' },
  { value: 'satoshi', label: 'Satoshi' },
  { value: 'ibm-plex-sans', label: 'IBM Plex Sans' },
  { value: 'source-sans', label: 'Source Sans' },
  { value: 'literata', label: 'Literata (Serif)' },
];

export function useFont() {
  const settings = useSettingsStore(s => s.settings);
  const font = (settings.font as Font) || 'ibm-plex-sans';

  useEffect(() => {
    document.documentElement.setAttribute('data-font', font);
  }, [font]);

  return font;
}
