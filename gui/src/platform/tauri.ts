import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { open, save } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { load } from '@tauri-apps/plugin-store';
import type { DesktopHost, OSStats, ServerStatusEvent } from './types';

const report = (operation: Promise<unknown>) => {
  void operation.catch(console.error);
};

export function createTauriHost(): DesktopHost {
  const appWindow = getCurrentWindow();
  return {
    kind: 'tauri',
    capabilities: { tray: true, discordPresence: true },
    api: {
      onServerStatus: (callback) => {
        let disposed = false;
        let unlisten: (() => void) | undefined;
        listen<ServerStatusEvent>('server-status', ({ payload }) => {
          if (!disposed) callback(payload);
        })
          .then((stop) => {
            if (disposed) stop();
            else {
              unlisten = stop;
              report(
                invoke<ServerStatusEvent[]>('server_status_history').then((events) => {
                  if (!disposed) events.forEach(callback);
                })
              );
            }
          })
          .catch(console.error);
        return () => {
          disposed = true;
          unlisten?.();
        };
      },
      setPresence: (settings) => report(invoke('set_presence', { settings })),
      openUrl,
      osStats: () => invoke<OSStats>('os_stats'),
      openLogsFolder: () => invoke('open_folder', { folder: 'logs' }),
      openConfigFolder: () => invoke('open_folder', { folder: 'config' }),
      close: () => report(appWindow.close()),
      hide: () => report(appWindow.hide()),
      minimize: () => report(appWindow.minimize()),
      toggleMaximize: () => report(appWindow.toggleMaximize()),
      showDecorations: (decorations) => report(appWindow.setDecorations(decorations)),
      // Electron currently also uses a no-op; the native tray uses fixed labels.
      setTranslations: () => {},
      i18nOverride: () => invoke<string | false>('i18n_override'),
      getStorage: async (type) => {
        const store = await load(
          type === 'settings' ? 'gui-settings.dat' : 'gui-cache.dat',
          {
            defaults: {},
            autoSave: type === 'settings' ? 1000 : 100,
          }
        );
        return {
          get: <T>(key: string) => store.get<T>(key),
          set: (key, value) => store.set(key, value),
          delete: (key) => store.delete(key),
          save: async () => {
            await store.save();
            return true;
          },
        };
      },
      openDialog: async (options) => {
        const selection = await open({
          title: options.title,
          defaultPath: options.defaultPath,
          filters: options.filters,
          directory: options.properties?.includes('openDirectory'),
          multiple: options.properties?.includes('multiSelections'),
        });
        return {
          canceled: selection === null,
          filePaths:
            selection === null
              ? []
              : Array.isArray(selection)
                ? selection
                : [selection],
        };
      },
      saveDialog: async (options) => {
        const filePath = await save(options);
        return { canceled: filePath === null, filePath: filePath ?? undefined };
      },
      log: (level, ...args) => report(invoke('write_log', { level, args })),
      getLogLevel: () => invoke('log_level'),
      openFile: (path) => report(invoke('open_managed_path', { path })),
      ghGet: (options) => invoke('github_get', { options }),
      getInstallDir: () => invoke<string>('install_dir'),
      isSteam: () => invoke<boolean>('is_steam'),
    },
  };
}
