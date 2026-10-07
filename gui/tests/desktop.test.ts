import assert from 'node:assert/strict';
import { beforeEach, afterEach, test } from 'node:test';
import { clearMocks, mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { createTauriHost } from '../src/platform/tauri';

// Tauri's official IPC mocks let us exercise the real plugin adapters in Node.
Object.assign(globalThis, { window: { crypto: globalThis.crypto } });
const { detectDesktopHost } = await import('../src/platform/index');
const tick = () => new Promise<void>((resolve) => setImmediate(resolve));

beforeEach(() => {
  Object.assign(globalThis, { window: { crypto: globalThis.crypto }, isTauri: false });
  mockWindows('main');
});
afterEach(() => {
  clearMocks();
});

test('browser fallback never requires a desktop IPC bridge', () => {
  assert.equal(detectDesktopHost(), null);
});

test('Tauri is detected through its native IPC marker', () => {
  Object.assign(globalThis, { isTauri: true });
  assert.equal(detectDesktopHost()?.kind, 'tauri');
  assert.equal(detectDesktopHost()?.capabilities.discordPresence, true);
});

test('native picker paths and cancellation preserve the renderer contract', async () => {
  let selection: string | string[] | null = 'C:\\recordings';
  let lastOptions: Record<string, unknown> | undefined;
  mockIPC((command, rawArgs) => {
    const args = rawArgs as Record<string, unknown> | undefined;
    assert.ok(command === 'plugin:dialog|open' || command === 'plugin:dialog|save');
    lastOptions = args?.options as Record<string, unknown>;
    return selection;
  });
  const { api } = createTauriHost();
  assert.deepEqual(await api.openDialog({ properties: ['openDirectory'] }), {
    canceled: false,
    filePaths: ['C:\\recordings'],
  });
  assert.equal(lastOptions?.directory, true);
  selection = ['/tmp/a.bvh', '/tmp/b.bvh'];
  assert.deepEqual(
    await api.openDialog({ properties: ['openFile', 'multiSelections'] }),
    {
      canceled: false,
      filePaths: selection,
    }
  );
  assert.equal(lastOptions?.directory, false);
  assert.equal(lastOptions?.multiple, true);
  selection = null;
  assert.deepEqual(await api.openDialog({}), { canceled: true, filePaths: [] });
  assert.deepEqual(await api.saveDialog({ defaultPath: 'recording.bvh' }), {
    canceled: true,
    filePath: undefined,
  });
  selection = '/tmp/recording.bvh';
  assert.deepEqual(await api.saveDialog({}), { canceled: false, filePath: selection });
});

test('settings and cache retain distinct files and await persistence', async () => {
  const files = new Map<number, Map<string, unknown>>();
  const paths: string[] = [];
  const saved: number[] = [];
  mockIPC((command, rawArgs) => {
    const args = rawArgs as Record<string, unknown> | undefined;
    if (command === 'plugin:store|load') {
      paths.push(args?.path as string);
      const rid = files.size;
      files.set(rid, new Map());
      return rid;
    }
    const rid = args?.rid as number;
    const file = files.get(rid)!;
    const key = args?.key as string;
    switch (command) {
      case 'plugin:store|set':
        file.set(key, args?.value);
        return;
      case 'plugin:store|get':
        return [file.get(key), file.has(key)];
      case 'plugin:store|delete':
        return file.delete(key);
      case 'plugin:store|save':
        saved.push(rid);
        return;
      default:
        throw new Error(`Unexpected IPC: ${command}`);
    }
  });
  const { api } = createTauriHost();
  const settings = await api.getStorage('settings');
  const cache = await api.getStorage('cache');
  await settings.set('config.json', '{"lang":"zh-CN"}');
  assert.equal(await settings.get('config.json'), '{"lang":"zh-CN"}');
  assert.equal(await cache.get('config.json'), undefined);
  assert.equal(await settings.save(), true);
  assert.deepEqual(saved, [0]);
  assert.equal(await settings.delete('config.json'), true);
  assert.equal(await settings.get('config.json'), undefined);
  assert.deepEqual(paths, ['gui-settings.dat', 'gui-cache.dat']);
});

test('disposing before asynchronous event registration still removes the listener', async () => {
  let register: ((value: number) => void) | undefined;
  const removed: number[] = [];
  mockIPC((command, rawArgs) => {
    const args = rawArgs as Record<string, unknown> | undefined;
    if (command === 'plugin:event|listen')
      return new Promise<number>((resolve) => {
        register = resolve;
      });
    if (command === 'plugin:event|unlisten') {
      removed.push(args?.eventId as number);
      return;
    }
    if (command === 'server_status_history')
      throw new Error('Disposed subscriptions must not replay history');
    throw new Error(`Unexpected IPC: ${command}`);
  });
  const dispose = createTauriHost().api.onServerStatus(() =>
    assert.fail('Disposed listener was invoked')
  );
  dispose();
  register!(42);
  await tick();
  assert.deepEqual(removed, [42]);
});

test('Tauri log verbosity and all diagnostic levels use native IPC unchanged', async () => {
  const writes: unknown[] = [];
  mockIPC((command, args) => {
    if (command === 'log_level') return 'trace';
    assert.equal(command, 'write_log');
    writes.push(args);
  });
  const { api } = createTauriHost();
  assert.equal(await api.getLogLevel?.(), 'trace');
  for (const level of ['error', 'warn', 'info', 'debug', 'trace'] as const)
    api.log(level, 'message');
  await tick();
  assert.deepEqual(
    writes,
    ['error', 'warn', 'info', 'debug', 'trace'].map((level) => ({
      level,
      args: ['message'],
    }))
  );
});
