import { existsSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

export function findRustServer(directories: string[], explicit?: string) {
  const filename =
    process.platform === 'win32' ? 'slimevr-server.exe' : 'slimevr-server';
  const candidates = explicit
    ? [resolve(explicit)]
    : directories.map((directory) => join(directory, filename));
  const binary = candidates.find(
    (candidate) => existsSync(candidate) && statSync(candidate).isFile()
  );
  if (explicit && !binary) throw new Error(`Rust server not found: ${explicit}`);
  return binary;
}

export function rustServerArgs(
  configDirectory: string,
  resourcesDirectory: string,
  configFile?: string,
  logLevel = 'info'
) {
  const yml = join(configDirectory, 'vrconfig.yml');
  const config = configFile
    ? resolve(configFile)
    : !existsSync(yml) && existsSync(join(configDirectory, 'vrconfig.yaml'))
      ? join(configDirectory, 'vrconfig.yaml')
      : yml;
  const args = [
    'listen',
    '--shutdown-on-stdin-eof',
    '--pose-output-ms',
    '1000',
    '--api-bind',
    '127.0.0.1:21110',
    '--config',
    config,
    '--log-level',
    logLevel,
  ];
  if (process.platform !== 'darwin') {
    const platform =
      process.platform === 'win32'
        ? 'win64'
        : process.arch === 'arm64'
          ? 'linuxarm64'
          : 'linux64';
    const driverPlatform =
      process.platform === 'win32'
        ? 'win64'
        : process.arch === 'arm64'
          ? 'aarch64-linux'
          : 'x64-linux';
    const driver = join(
      resourcesDirectory,
      'drivers',
      `slimevr-openvr-driver-${driverPlatform}`
    );
    if (existsSync(join(driver, 'driver.vrdrivermanifest'))) {
      args.push('--steamvr-driver', driver);
    }
    const provider = join(
      resourcesDirectory,
      'bindings',
      platform,
      process.platform === 'win32'
        ? 'SlimeVR-Bindings-Provider.exe'
        : 'slimevr-bindings-provider'
    );
    if (existsSync(provider)) args.push('--bindings-provider', provider);
  }
  return args;
}
