import type { LogLevel } from './logging';

/** Desktop capabilities shared by the renderer and host adapters. No Electron types. */
export type ServerStatusEvent = {
  type: 'stdout' | 'stderr' | 'error' | 'terminated' | 'other';
  message: string;
  level?: LogLevel;
};

export type OSStats = { type: 'linux' | 'windows' | 'macos' | 'unknown' };

export interface CrossStorage {
  set(key: string, value: unknown): Promise<void>;
  get<T>(key: string): Promise<T | undefined>;
  delete(key: string): Promise<boolean>;
  save(): Promise<boolean>;
}

export type FileFilter = { name: string; extensions: string[] };
export type OpenDialogOptions = {
  title?: string;
  defaultPath?: string;
  filters?: FileFilter[];
  properties?: ('openFile' | 'openDirectory' | 'multiSelections' | 'createDirectory')[];
};
export type OpenDialogReturnValue = { canceled: boolean; filePaths: string[] };
export type SaveDialogOptions = {
  title?: string;
  defaultPath?: string;
  filters?: FileFilter[];
};
export type SaveDialogReturnValue = { canceled: boolean; filePath?: string };

export type GHGet = { type: 'fw-releases' } | { type: 'asset'; url: string };
export type GHReturn = {
  asset: [number, string][] | null;
  'fw-releases':
    | {
        assets: { browser_download_url: string; name: string; digest: string }[];
        prerelease: boolean;
        tag_name: string;
        body: string;
      }[]
    | null;
};
export type DiscordPresence =
  | { enable: false }
  | { enable: true; activity: string; iconText: string | undefined };

export interface DesktopAPI {
  onServerStatus(cb: (data: ServerStatusEvent) => void): () => void;
  openUrl(url: string): Promise<void>;
  osStats(): Promise<OSStats>;
  openLogsFolder(): Promise<void>;
  openConfigFolder(): Promise<void>;
  close(): void;
  hide(): void;
  minimize(): void;
  toggleMaximize(): void;
  showDecorations(decorations: boolean): void;
  setTranslations(translations: Record<string, string>): void;
  i18nOverride(): Promise<string | false>;
  getStorage(type: 'settings' | 'cache'): Promise<CrossStorage>;
  openDialog(options: OpenDialogOptions): Promise<OpenDialogReturnValue>;
  saveDialog(options: SaveDialogOptions): Promise<SaveDialogReturnValue>;
  log(type: LogLevel, ...args: unknown[]): void;
  getLogLevel?(): Promise<LogLevel>;
  openFile(path: string): void;
  ghGet<T extends GHGet>(options: T): Promise<GHReturn[T['type']]>;
  setPresence?(options: DiscordPresence): void;
  getInstallDir(): Promise<string>;
  isSteam(): Promise<boolean>;
}

export type DesktopHost = {
  kind: 'electron' | 'tauri';
  api: DesktopAPI;
  capabilities: { tray: boolean; discordPresence: boolean };
};

declare global {
  interface Window {
    readonly electronAPI?: DesktopAPI;
  }
}
