import { isTauri } from '@tauri-apps/api/core';
import { createTauriHost } from './tauri';
import type { DesktopHost } from './types';

/** Select once, before configuration/cache modules and React are initialized. */
export function detectDesktopHost(): DesktopHost | null {
  return isTauri() ? createTauriHost() : null;
}

export const desktopHost = detectDesktopHost();
