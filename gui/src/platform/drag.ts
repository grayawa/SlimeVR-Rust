import { getCurrentWindow } from '@tauri-apps/api/window';
import { desktopHost } from './index';

/** Reuse the renderer's drag regions; Chromium's CSS rule is Electron-only. */
export function installWindowDragging() {
  if (desktopHost?.kind !== 'tauri') return;
  document.addEventListener('mousedown', (event) => {
    if (event.button !== 0 || !(event.target instanceof Element)) return;
    if (event.target.closest('button, a, input, select, textarea, [role="button"]'))
      return;
    if (event.target.closest('[data-desktop-drag-region="true"]')) {
      void getCurrentWindow().startDragging().catch(console.error);
    }
  });
}
