import { app } from 'electron';
import path, { join } from 'node:path';
import { getPlatform } from './utils';
export const CONFIG_IDENTIFIER = 'dev.slimevr.SlimeVR';

export const getGuiDataFolder = () => {
  const platform = getPlatform();

  switch (platform) {
    case 'linux':
      if (process.env['XDG_DATA_HOME'])
        return join(process.env['XDG_DATA_HOME'], CONFIG_IDENTIFIER);
      return join(app.getPath('home'), '.local/share', CONFIG_IDENTIFIER);
    case 'windows':
      return join(app.getPath('appData'), CONFIG_IDENTIFIER);
    case 'macos':
      return join(
        app.getPath('home'),
        'Library/Application Support',
        CONFIG_IDENTIFIER
      );
    case 'unknown':
      throw 'error';
  }
};

export const getServerDataFolder = () => {
  const platform = getPlatform();

  switch (platform) {
    case 'linux':
    case 'windows':
    case 'macos':
      return join(app.getPath('appData'), CONFIG_IDENTIFIER);
    case 'unknown':
      throw 'error';
  }
};

export const getLogsFolder = () => {
  return join(getGuiDataFolder(), 'logs');
};

export const getExeFolder = () => {
  return path.dirname(app.getPath('exe'));
};

export const getWindowStateFile = () =>
  join(getServerDataFolder(), '.window-state.json');
