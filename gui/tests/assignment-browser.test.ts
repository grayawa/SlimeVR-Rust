import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { existsSync } from 'node:fs';
import { mkdtemp, writeFile, rm } from 'node:fs/promises';
import { createServer } from 'node:net';
import { createInterface } from 'node:readline';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { chromium } from 'playwright-core';
import * as sx from 'solarxr-protocol';
import { decodeSolarXR, encodeSolarXR } from '../src/platform/solarxr';

const binary =
  process.env.SLIMEVR_RUST_BINARY ??
  resolve(
    `../server-rust/target/debug/slimevr-server${process.platform === 'win32' ? '.exe' : ''}`
  );
const browserExecutable =
  process.env.SLIMEVR_BROWSER_EXECUTABLE ??
  [
    '/usr/bin/chromium',
    '/usr/bin/google-chrome',
    join(
      process.env['ProgramFiles(x86)'] ?? '',
      'Microsoft/Edge/Application/msedge.exe'
    ),
    join(process.env.ProgramFiles ?? '', 'Microsoft/Edge/Application/msedge.exe'),
  ].find(existsSync);
const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

test(
  'the assignment page remains connected across settings refreshes, navigation and reconnects',
  { timeout: 60000, skip: !browserExecutable },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-assignment-browser-'));
    const config = join(dir, 'vrconfig.yml');
    await writeFile(
      config,
      "version: '15'\ntapDetection:\n  fullResetDelay: 2.75\n  yawResetDelay: 1.25\n  mountingResetDelay: 3.5\n  fullResetEnabled: true\n  yawResetEnabled: false\n  mountingResetEnabled: true\n"
    );
    const backend = spawn(
      binary,
      [
        'listen',
        '--bind',
        '127.0.0.1:0',
        '--api-bind',
        '127.0.0.1:0',
        '--config',
        config,
        '--no-discovery',
        '--no-steamvr',
        '--pose-output-ms',
        '1000',
        '--shutdown-on-stdin-eof',
      ],
      { stdio: ['pipe', 'pipe', 'pipe'] }
    );
    let backendError = '';
    backend.stderr.on('data', (data) => (backendError += data.toString()));
    const output = createInterface({ input: backend.stdout });
    const ready = new Promise<string>((resolve, reject) => {
      backend.once('error', reject);
      backend.once('exit', () =>
        reject(new Error(backendError || 'backend exited before listening'))
      );
      output.on('line', (line) => {
        const value = JSON.parse(line);
        if (value.type === 'listening') resolve(value.api_bind);
      });
    });
    const free = createServer();
    free.listen(0, '127.0.0.1');
    await once(free, 'listening');
    const port = (free.address() as { port: number }).port;
    await new Promise<void>((resolve) => free.close(() => resolve()));
    // Test the built application, including its actual page effects and binary protocol.
    const preview = spawn(
      process.execPath,
      [
        'node_modules/vite/bin/vite.js',
        'preview',
        '--host',
        '127.0.0.1',
        '--port',
        String(port),
        '--strictPort',
      ],
      { stdio: ['ignore', 'pipe', 'pipe'] }
    );
    let previewError = '';
    preview.stderr.on('data', (data) => (previewError += data.toString()));
    const previewReady = new Promise<void>((resolve, reject) => {
      preview.once('error', reject);
      preview.once('exit', () =>
        reject(new Error(previewError || 'preview exited before listening'))
      );
      preview.stdout.on('data', (data) => {
        if (data.toString().includes(`:${port}/`)) resolve();
      });
    });
    let actor: WebSocket | undefined;
    let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;
    try {
      const [api] = await Promise.all([ready, previewReady]);
      actor = new WebSocket(`ws://${api}`);
      actor.binaryType = 'arraybuffer';
      const replies: sx.RpcMessageHeaderT[] = [];
      actor.addEventListener('message', ({ data }) => {
        if (typeof data !== 'string') replies.push(...decodeSolarXR(data).rpcMsgs);
      });
      await once(actor, 'open');
      let tx = 0;
      const send = (type: sx.RpcMessage, message: sx.RpcMessageHeaderT['message']) => {
        const id = ++tx;
        const bundle = new sx.MessageBundleT();
        bundle.rpcMsgs = [
          new sx.RpcMessageHeaderT(new sx.TransactionIdT(id), type, message),
        ];
        actor!.send(encodeSolarXR(bundle));
        return id;
      };
      const settings = async () => {
        const id = send(sx.RpcMessage.SettingsRequest, new sx.SettingsRequestT());
        const deadline = Date.now() + 5000;
        while (Date.now() < deadline) {
          const reply = replies.find(
            (p) => p.txId?.id === id && p.messageType === sx.RpcMessage.SettingsResponse
          );
          if (reply) return reply.message as sx.SettingsResponseT;
          await delay(10);
        }
        throw new Error('backend did not respond to SettingsRequest');
      };
      const tapValues = (value: sx.SettingsResponseT) => {
        const { setupMode: _setup, ...rest } = value.tapDetectionSettings!;
        return rest;
      };
      const before = tapValues(await settings());
      browser = await chromium.launch({
        executablePath: browserExecutable,
        headless: true,
        args: ['--no-sandbox', '--disable-dev-shm-usage'],
      });
      const page = await browser.newPage();
      await page.addInitScript(() => {
        localStorage.setItem(
          'config.json',
          JSON.stringify({
            lang: 'en',
            doneOnboarding: true,
            errorTracking: false,
            watchNewDevices: false,
          })
        );
        const Native = window.WebSocket;
        const sockets: WebSocket[] = [];
        Object.assign(window, { assignmentTestSockets: sockets });
        window.WebSocket = class extends Native {
          constructor(url: string | URL, protocols?: string | string[]) {
            super(url, protocols);
            sockets.push(this);
          }
        };
      });
      await page.route('**/*', (route) =>
        new URL(route.request().url()).hostname === '127.0.0.1'
          ? route.continue()
          : route.abort()
      );
      const connections: { closed: boolean; updates: number }[] = [];
      const tapWrites: sx.TapDetectionSettingsT[] = [];
      const errors: string[] = [];
      page.on('pageerror', (error) => errors.push(error.message));
      page.on('websocket', (socket) => {
        if (!socket.url().startsWith(`ws://${api}`)) return;
        const state = { closed: false, updates: 0 };
        connections.push(state);
        socket.on('close', () => (state.closed = true));
        socket.on('framereceived', ({ payload }) => {
          if (typeof payload !== 'string')
            state.updates += decodeSolarXR(
              Uint8Array.from(payload).buffer
            ).dataFeedMsgs.length;
        });
        socket.on('framesent', ({ payload }) => {
          if (typeof payload === 'string') return;
          for (const header of decodeSolarXR(Uint8Array.from(payload).buffer).rpcMsgs) {
            if (header.messageType === sx.RpcMessage.ChangeSettingsRequest) {
              const taps = (header.message as sx.ChangeSettingsRequestT)
                .tapDetectionSettings;
              if (taps?.setupMode !== null && taps?.setupMode !== undefined)
                tapWrites.push(taps);
            }
          }
        });
      });
      await page.goto(
        `http://127.0.0.1:${port}/?ip=127.0.0.1&port=${api.split(':')[1]}`
      );
      await page
        .getByRole('link', { name: 'Tracker Assignment', exact: true })
        .waitFor();
      await page.getByRole('link', { name: 'Tracker Assignment', exact: true }).click();
      await page.getByText('Assign trackers', { exact: true }).waitFor();
      await page.waitForTimeout(8000);
      // Switching onboarding layouts can remount the page once. That finite
      // enter/leave sequence is different from writing on every settings reply.
      assert.ok(
        tapWrites.length >= 1 && tapWrites.length <= 3,
        `assignment page wrote tap settings ${tapWrites.length} times`
      );
      const firstEntryWrites = tapWrites.length;
      assert.equal(tapWrites.at(-1)!.setupMode, true);
      assert.equal(
        connections.length,
        1,
        'unexpected reconnect while assigning trackers'
      );
      assert.equal(connections[0].closed, false);
      assert.ok(connections[0].updates > 100, 'data feeds stalled');
      assert.equal((await settings()).tapDetectionSettings!.setupMode, true);

      // An unrelated update broadcasts a full SettingsResponse while the page stays open.
      const change = new sx.ChangeSettingsRequestT();
      change.filtering = new sx.FilteringSettingsT(sx.FilteringType.SMOOTHING, 0.3);
      send(sx.RpcMessage.ChangeSettingsRequest, change);
      await settings();
      await page.waitForTimeout(1000);
      assert.equal(
        tapWrites.length,
        firstEntryWrites,
        'SettingsResponse restarted setup mode'
      );
      assert.deepEqual(tapValues(await settings()), before);

      await page.getByRole('link', { name: 'Home', exact: true }).click();
      await page.waitForTimeout(300);
      assert.equal(tapWrites.length, firstEntryWrites + 1);
      assert.equal(tapWrites.at(-1)!.setupMode, false);
      assert.equal((await settings()).tapDetectionSettings!.setupMode, false);
      await page.getByRole('link', { name: 'Tracker Assignment', exact: true }).click();
      await page.getByText('Assign trackers', { exact: true }).waitFor();
      await page.waitForTimeout(300);
      const reentryWrites = tapWrites.length - firstEntryWrites - 1;
      assert.ok(reentryWrites >= 1 && reentryWrites <= 3);
      assert.equal(tapWrites.at(-1)!.setupMode, true);
      const beforeReconnectWrites = tapWrites.length;

      await page.evaluate((url) => {
        const sockets = (window as unknown as { assignmentTestSockets: WebSocket[] })
          .assignmentTestSockets;
        sockets
          .find((socket) => socket.url === url && socket.readyState === WebSocket.OPEN)!
          .close(1000, 'test reconnect');
      }, `ws://${api}/`);
      await page.getByText('Assign trackers', { exact: true }).waitFor();
      await page.waitForTimeout(4500);
      assert.equal(connections.length, 2, 'reconnect did not settle');
      assert.equal(connections[1].closed, false);
      assert.equal(tapWrites.length, beforeReconnectWrites + 1);
      assert.equal(tapWrites.at(-1)!.setupMode, true);
      assert.equal(
        await page
          .getByText('Could not complete the operation', { exact: true })
          .count(),
        0,
        'disconnect cleanup displayed an error'
      );
      await page.getByRole('link', { name: 'Home', exact: true }).click();
      await page.waitForTimeout(300);
      assert.equal(tapWrites.length, beforeReconnectWrites + 2);
      assert.equal(tapWrites.at(-1)!.setupMode, false);
      assert.equal((await settings()).tapDetectionSettings!.setupMode, false);
      assert.deepEqual(tapValues(await settings()), before);
      for (const value of tapWrites) {
        const { setupMode: _mode, ...rest } = value;
        assert.ok(
          Object.values(rest).every((v) => v === null),
          'setup mode overwrote unrelated tap settings'
        );
      }
      assert.deepEqual(errors, []);
      console.log(
        `assignment stable: ${connections.length} connections (one intentional reconnect), ${tapWrites.length} setup writes`
      );
    } finally {
      await browser?.close();
      actor?.close();
      preview.kill('SIGTERM');
      const exited = once(backend, 'exit');
      backend.stdin.end();
      await exited;
      output.close();
      await rm(dir, { recursive: true, force: true });
    }
  }
);
