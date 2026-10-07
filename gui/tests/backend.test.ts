import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn, spawnSync } from 'node:child_process';
import { createInterface } from 'node:readline';
import { createSocket } from 'node:dgram';
import { createConnection } from 'node:net';
import { createServer } from 'node:http';
import { mkdtemp, readFile, writeFile, rm, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { once } from 'node:events';
import * as sx from 'solarxr-protocol';
import { FluentBundle, FluentResource } from '@fluent/bundle';
import { ReactLocalization } from '@fluent/react';
import {
  decodeBackendNotice,
  decodeSolarXR,
  encodeSolarXR,
  ReadRequestCache,
} from '../src/platform/solarxr';

const binary =
  process.env.SLIMEVR_RUST_BINARY ??
  resolve(
    `../server-rust/target/debug/slimevr-server${process.platform === 'win32' ? '.exe' : ''}`
  );
const delay = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

class Client {
  socket: WebSocket;
  packets: any[] = [];
  legacy: any[] = [];
  tx = 0;
  constructor(address: string) {
    this.socket = new WebSocket(`ws://${address}`);
    this.socket.binaryType = 'arraybuffer';
    this.socket.addEventListener('message', ({ data }) => {
      if (typeof data === 'string') {
        const notice = decodeBackendNotice(data);
        if (notice) this.packets.push(notice);
        else {
          this.legacy.push(JSON.parse(data));
          if (this.legacy.length > 300) this.legacy.splice(0, 100);
        }
      } else {
        const bundle = decodeSolarXR(data);
        this.packets.push(
          ...bundle.rpcMsgs,
          ...bundle.dataFeedMsgs,
          ...bundle.pubSubMsgs
        );
      }
      if (this.packets.length > 300) this.packets.splice(0, 100);
    });
  }
  async wait(predicate: (p: any) => boolean, timeout = 3000): Promise<any> {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      const index = this.packets.findIndex(predicate);
      if (index >= 0) return this.packets.splice(index, 1)[0];
      await delay(10);
    }
    throw new Error(
      `Timed out waiting for packet: ${predicate}; recent=${JSON.stringify(this.packets.filter((p) => p?.type || p?.txId).slice(-12), (_, v) => (typeof v === 'bigint' ? String(v) : v))}`
    );
  }
  sendRPC(type: sx.RpcMessage, body: sx.RpcMessageHeaderT['message']) {
    const tx = ++this.tx;
    const bundle = new sx.MessageBundleT();
    bundle.rpcMsgs = [new sx.RpcMessageHeaderT(new sx.TransactionIdT(tx), type, body)];
    this.send(bundle);
    return tx;
  }
  send(bundle: sx.MessageBundleT) {
    this.socket.send(encodeSolarXR(bundle));
  }
  async rpc(
    type: sx.RpcMessage,
    body: sx.RpcMessageHeaderT['message'],
    response: sx.RpcMessage
  ) {
    const tx = this.sendRPC(type, body);
    return (await this.wait((p) => p.messageType === response && p.txId?.id === tx))
      .message;
  }
  feed(config: sx.DataFeedConfigT) {
    const bundle = new sx.MessageBundleT();
    bundle.dataFeedMsgs = [
      new sx.DataFeedMessageHeaderT(
        sx.DataFeedMessage.StartDataFeed,
        new sx.StartDataFeedT([config])
      ),
    ];
    this.send(bundle);
  }
}
function config(): sx.DataFeedConfigT {
  const mask = new sx.TrackerDataMaskT();
  Object.assign(mask, {
    info: true,
    status: true,
    rotation: true,
    position: true,
    rawAcceleration: true,
    linearAcceleration: true,
    rotationReferenceAdjusted: true,
    stayAligned: true,
  });
  const c = new sx.DataFeedConfigT();
  Object.assign(c, {
    dataMask: new sx.DeviceDataMaskT(mask, true),
    minimumTimeSinceLast: 20,
    syntheticTrackersMask: mask,
    boneMask: true,
    serverGuardsMask: true,
  });
  return c;
}
function wire(id: number, seq: number, body: Uint8Array) {
  const header = Buffer.alloc(12);
  header.writeUInt32BE(id);
  header.writeBigInt64BE(BigInt(seq), 4);
  return Buffer.concat([header, body]);
}
function handshake(device: number) {
  const header = Buffer.alloc(28);
  [9, 13, 1, 0, 0, 0, 22].forEach((v, i) => header.writeUInt32BE(v, 4 * i));
  return wire(
    3,
    0,
    Buffer.concat([
      header,
      Buffer.from([5, ...Buffer.from('good'), 0, 2, 0, 0, 0, 0, device]),
    ])
  );
}
async function start(args: string[]) {
  const child = spawn(
    binary,
    [
      'listen',
      '--bind',
      '127.0.0.1:0',
      '--api-bind',
      '127.0.0.1:0',
      '--no-discovery',
      ...(args.includes('--steamvr-endpoint') ? [] : ['--no-steamvr']),
      '--pose-output-ms',
      '1000',
      ...(process.platform === 'win32' && !args.includes('--shutdown-on-stdin-eof')
        ? ['--shutdown-on-stdin-eof']
        : []),
      ...args,
    ],
    {
      stdio: [
        args.includes('--shutdown-on-stdin-eof') || process.platform === 'win32'
          ? 'pipe'
          : 'ignore',
        'pipe',
        'pipe',
      ],
    }
  );
  const lines: any[] = [];
  let stderr = '';
  child.stderr!.on('data', (b) => {
    stderr += b;
  });
  const reader = createInterface({ input: child.stdout! });
  let started: ((value: any) => void) | undefined;
  const ready = new Promise<any>((r, reject) => {
    started = r;
    child.once('error', reject);
    child.once('exit', () => {
      if (!lines.length) reject(new Error(stderr || 'server exited'));
    });
  });
  reader.on('line', (line) => {
    const data = JSON.parse(line);
    lines.push(data);
    if (data.type === 'listening') started?.(data);
  });
  const info = await Promise.race([
    ready,
    delay(5000).then(() => {
      throw new Error('server startup timeout');
    }),
  ]);
  return {
    child,
    info,
    lines,
    get stderr() {
      return stderr;
    },
    async stop() {
      const exited = once(child, 'exit');
      // Windows cannot deliver POSIX SIGINT to this child; use the desktop-owned EOF path.
      if (process.platform === 'win32') child.stdin!.end();
      else child.kill('SIGINT');
      const [code] = await exited;
      assert.equal(code, 0, stderr);
    },
  };
}

test(
  'SteamVR capability, role settings and connection checklist use the existing GUI protocol',
  { timeout: 15000, skip: process.platform === 'win32' },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-steamvr-'));
    const endpoint = join(dir, 'SlimeVRDriver');
    const path = join(dir, 'vrconfig.yml');
    await writeFile(
      path,
      "version: '15'\nvelocityConfig:\n  sendDerivedVelocity: true\nbridges:\n  steamvr:\n    automaticSharedTrackersToggling: false\n    trackers: {waist: true, left_foot: true, right_foot: true}\n"
    );
    const server = await start([
      '--config',
      path,
      '--steamvr-endpoint',
      endpoint,
      '--no-bindings-provider',
    ]);
    const client = new Client(server.info.api_bind);
    const driver = createConnection(endpoint);
    driver.on('data', () => {});
    try {
      await Promise.all([once(driver, 'connect'), once(client.socket, 'open')]);
      const info = await client.wait((p) => p?.type === 'backend_info');
      assert.ok(info.info.capabilities.includes('steamvr'));
      assert.ok(info.info.capabilities.includes('derived_velocity'));
      assert.ok(info.info.capabilities.includes('extended_calibration'));
      const golden = JSON.parse(
        await readFile(
          '../server-rust/crates/slimevr-server/tests/fixtures/steamvr-java-golden.json',
          'utf8'
        )
      );
      for (const name of ['version', 'added0', 'pose0']) {
        const sample = golden.samples.find(
          (s: { name: string; hex: string }) => s.name === name
        );
        const bytes = Buffer.from(sample.hex, 'hex');
        const length = Buffer.alloc(4);
        length.writeUInt32LE(bytes.length + 4);
        driver.write(length);
        driver.write(bytes);
      }
      await client.wait(
        (p) =>
          p.messageType === sx.RpcMessage.TrackingChecklistResponse &&
          p.message.steps?.some(
            (s: sx.TrackingChecklistStepT) =>
              s.id === sx.TrackingChecklistStepId.STEAMVR_DISCONNECTED && s.valid
          )
      );
      const settings = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.equal(settings.steamVrTrackers.waist, true);
      assert.equal(settings.steamVrTrackers.automaticTrackerToggle, false);
      assert.equal(settings.velocitySettings.sendDerivedVelocity, true);
      const shared = new sx.SteamVRTrackersSettingT();
      shared.waist = true;
      shared.chest = true;
      shared.automaticTrackerToggle = false;
      const change = new sx.ChangeSettingsRequestT();
      change.steamVrTrackers = shared;
      change.velocitySettings = new sx.VelocitySettingsT();
      change.velocitySettings.sendDerivedVelocity = false;
      client.sendRPC(sx.RpcMessage.ChangeSettingsRequest, change);
      await client.wait(
        (p) =>
          p.messageType === sx.RpcMessage.SettingsResponse &&
          p.message.steamVrTrackers?.chest === true &&
          p.message.velocitySettings?.sendDerivedVelocity === false &&
          p.message.steamVrTrackers?.leftFoot === false
      );
      const saved = await readFile(path, 'utf8');
      assert.match(saved, /chest: true/);
      assert.match(saved, /left_foot: false/);
      assert.match(saved, /sendDerivedVelocity: false/);
      driver.destroy();
      await client.wait(
        (p) =>
          p.messageType === sx.RpcMessage.TrackingChecklistResponse &&
          p.message.steps?.some(
            (s: sx.TrackingChecklistStepT) =>
              s.id === sx.TrackingChecklistStepId.STEAMVR_DISCONNECTED && !s.valid
          )
      );
    } finally {
      driver.destroy();
      client.socket.close();
      await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test('backend notices and read cache reject unrelated text and retain copied, bounded subscriptions', () => {
  assert.equal(decodeBackendNotice('hello'), null);
  assert.equal(
    decodeBackendNotice('{"type":"backend_info","backend":"rust","capabilities":[1]}'),
    null
  );
  assert.equal(
    decodeBackendNotice('{"type":"backend_error","message":"unsupported"}')?.type,
    'backend_error'
  );
  assert.throws(() => decodeSolarXR(new ArrayBuffer(0)));
  const notice = decodeBackendNotice(
    '{"type":"backend_info","backend":"rust","capabilities":[],"driver_error":"vrpathreg failed"}'
  );
  assert.equal(
    notice?.type === 'backend_info' && notice.info.driverError,
    'vrpathreg failed'
  );
  assert.equal(notice?.type === 'backend_info' && notice.info.driverNotice, null);
  const manualDriver = decodeBackendNotice(
    '{"type":"backend_info","backend":"rust","capabilities":[],"driver_notice":"existing_manual_driver","driver_error":null}'
  );
  assert.equal(
    manualDriver?.type === 'backend_info' && manualDriver.info.driverNotice,
    'existing_manual_driver'
  );
  assert.equal(
    manualDriver?.type === 'backend_info' && manualDriver.info.driverError,
    null
  );
  const unknownNotice = decodeBackendNotice(
    '{"type":"backend_info","backend":"rust","capabilities":[],"driver_notice":"future_notice"}'
  );
  assert.equal(
    unknownNotice?.type === 'backend_info' && unknownNotice.info.driverNotice,
    null
  );
  const change = new sx.ChangeSettingsRequestT();
  change.tapDetectionSettings = new sx.TapDetectionSettingsT();
  change.tapDetectionSettings.setupMode = false;
  change.autoBoneSettings = new sx.AutoBoneSettingsT();
  change.autoBoneSettings.numEpochs = 0;
  const bundle = new sx.MessageBundleT();
  bundle.rpcMsgs = [
    new sx.RpcMessageHeaderT(
      new sx.TransactionIdT(1),
      sx.RpcMessage.ChangeSettingsRequest,
      change
    ),
  ];
  const wire = encodeSolarXR(bundle);
  const decoded = decodeSolarXR(wire.slice().buffer as ArrayBuffer);
  const settings = decoded.rpcMsgs[0].message as sx.ChangeSettingsRequestT;
  assert.equal(settings.tapDetectionSettings?.setupMode, false);
  assert.equal(settings.tapDetectionSettings?.yawResetEnabled, null);
  assert.equal(settings.autoBoneSettings?.numEpochs, 0);

  const cache = new ReadRequestCache();
  const data = new Uint8Array([1]);
  cache.remember('settings', data);
  data[0] = 2;
  assert.equal(cache.frames()[0][0], 1);
  cache.remember('settings', new Uint8Array([3]));
  assert.equal(cache.frames().length, 1);
  for (let i = 0; i < 100; i++) cache.remember(String(i), data);
  assert.equal(cache.frames().length, 64);
});

test('existing-driver information uses Fluent translations and the English fallback', async () => {
  const bundle = async (locale: string) => {
    const value = new FluentBundle(locale);
    value.addResource(
      new FluentResource(
        await readFile(resolve(`public/i18n/${locale}/translation.ftl`), 'utf8')
      )
    );
    return value;
  };
  const english = await bundle('en');
  // These messages are plain text; Node does not need a DOM markup parser.
  const chinese = new ReactLocalization([await bundle('zh-Hans'), english], null);
  assert.equal(
    chinese.getString('steamvr-existing-driver-title'),
    '使用已有的 SlimeVR 驱动'
  );
  assert.equal(
    chinese.getString('steamvr-existing-driver-description'),
    '检测到已有的 SlimeVR 驱动，已保留现有安装并跳过自动注册。'
  );
  const fallback = new ReactLocalization([new FluentBundle('de'), english], null);
  assert.equal(
    fallback.getString('steamvr-existing-driver-title'),
    'Using the existing SlimeVR driver'
  );
});

test(
  'a manual SteamVR driver is preserved and reported as information across reconnects',
  { timeout: 15000, skip: process.platform !== 'linux' },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-manual-driver-'));
    const runtime = join(dir, 'SteamVR');
    const marker = join(runtime, 'drivers', 'slimevr', 'existing-driver');
    await mkdir(join(runtime, 'bin'), { recursive: true });
    await mkdir(join(runtime, 'drivers', 'slimevr'), { recursive: true });
    await writeFile(marker, 'keep this installation');
    await writeFile(
      join(runtime, 'bin', 'vrpathreg.sh'),
      '#!/bin/sh\nif [ "$1" = finddriver ]; then exit 1; fi\nexit 99\n',
      { mode: 0o700 }
    );
    const http = createServer((_req, res) => {
      res.setHeader('Content-Type', 'application/json');
      res.end(
        JSON.stringify({
          jsonid: 'vr_driver_list',
          drivers: [
            {
              enabled: true,
              blocked_by_safe_mode: false,
              manifest: { name: 'slimevr' },
            },
          ],
        })
      );
    });
    http.listen(0, '127.0.0.1');
    await once(http, 'listening');
    const address = http.address() as { port: number };
    const server = await start([
      '--config',
      join(dir, 'vrconfig.yml'),
      '--steamvr-runtime',
      runtime,
      '--steamvr-endpoint',
      join(dir, 'SlimeVRDriver'),
      '--steamvr-http',
      `http://127.0.0.1:${address.port}`,
      '--no-bindings-provider',
    ]);
    const clients: Client[] = [];
    try {
      for (let i = 0; i < 2; i++) {
        const client = new Client(server.info.api_bind);
        clients.push(client);
        await once(client.socket, 'open');
        const notice = await client.wait(
          (p) =>
            p?.type === 'backend_info' &&
            p.info.driverNotice === 'existing_manual_driver'
        );
        assert.equal(notice.info.driverError, null);
        await client.rpc(
          sx.RpcMessage.HeartbeatRequest,
          new sx.HeartbeatRequestT(),
          sx.RpcMessage.HeartbeatResponse
        );
        assert.ok(client.packets.every((p) => p?.type !== 'backend_error'));
        client.socket.close();
      }
      assert.equal(await readFile(marker, 'utf8'), 'keep this installation');
      assert.ok(!server.stderr.includes('registration failed'));
    } finally {
      clients.forEach((client) => client.socket.close());
      await server.stop();
      await new Promise<void>((resolve) => http.close(() => resolve()));
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'existing TypeScript SolarXR bindings operate the real Rust receiver/core, persist and replay',
  { timeout: 25000 },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-api-'));
    const state = join(dir, 'vrconfig.yml');
    const journal = join(dir, 'record.jsonl');
    const seed = join(dir, 'seed.json');
    await writeFile(seed, '{"bindings":[]}');
    const sockets: ReturnType<typeof createSocket>[] = [];
    const clients: Client[] = [];
    const server = await start([
      '--config',
      state,
      '--record',
      journal,
      '--log-level',
      'debug',
    ]);
    let stopped = false;
    try {
      const client = new Client(server.info.api_bind);
      clients.push(client);
      const notice = await client.wait((p) => p?.type === 'backend_info');
      assert.equal(notice.info.persistent, true);
      const settings = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.ok(settings.modelSettings);
      client.feed(config());
      const [host, port] = server.info.bind.split(':');
      for (let n = 1; n <= 6; n++) {
        const socket = createSocket('udp4');
        sockets.push(socket);
        await new Promise<void>((r) => socket.bind(0, '127.0.0.1', r));
        socket.send(handshake(n), Number(port), host);
        const pending = await client.wait(
          (p) => p.messageType === sx.RpcMessage.UnknownDeviceHandshakeNotification
        );
        assert.equal(pending.message.macAddress, `02:00:00:00:00:0${n}`);
        client.sendRPC(
          sx.RpcMessage.AddUnknownDeviceRequest,
          new sx.AddUnknownDeviceRequestT(pending.message.macAddress)
        );
        // A heartbeat fences admission before the firmware's handshake retry.
        await client.rpc(
          sx.RpcMessage.HeartbeatRequest,
          new sx.HeartbeatRequestT(),
          sx.RpcMessage.HeartbeatResponse
        );
        socket.send(handshake(n), Number(port), host);
        socket.send(wire(15, 1, Buffer.from([0, 1, 13])), Number(port), host);
        const payload = Buffer.alloc(19);
        payload[1] = 1;
        payload.writeFloatBE(1, 14);
        payload[18] = 3;
        socket.send(wire(17, 2, payload), Number(port), host);
      }
      const initial = (
        await client.wait(
          (p) =>
            p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
            p.message.devices.length === 6 &&
            p.message.devices.every((d: any) => d.trackers[0]?.rotation)
        )
      ).message;
      assert.equal(initial.syntheticTrackers.length, 11);
      assert.ok(initial.bones.length > 20);
      assert.equal(initial.devices[0].trackers[0].trackerId.trackerNum, 0);
      assert.equal(initial.devices[0].hardwareStatus.batteryPctEstimate, null);
      const bodies = [
        sx.BodyPart.CHEST,
        sx.BodyPart.HIP,
        sx.BodyPart.LEFT_UPPER_LEG,
        sx.BodyPart.RIGHT_UPPER_LEG,
        sx.BodyPart.LEFT_LOWER_LEG,
        sx.BodyPart.RIGHT_LOWER_LEG,
      ];
      for (let i = 0; i < 6; i++) {
        const tracker = initial.devices[i].trackers[0];
        client.sendRPC(
          sx.RpcMessage.AssignTrackerRequest,
          new sx.AssignTrackerRequestT(
            tracker.trackerId,
            bodies[i],
            new sx.QuatT(0, 0, 0, 1),
            `Tracker ${i}`
          )
        );
      }
      const assigned = (
        await client.wait(
          (p) =>
            p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
            p.message.devices.length === 6 &&
            p.message.devices.every(
              (d: any) => d.trackers[0]?.info.bodyPart !== sx.BodyPart.NONE
            )
        )
      ).message;
      assert.ok(
        assigned.devices.every((d: any) => d.trackers[0].rotationReferenceAdjusted)
      );
      const resetTx = client.sendRPC(
        sx.RpcMessage.ResetRequest,
        new sx.ResetRequestT(sx.ResetType.Full, [], 0.01)
      );
      await client.wait(
        (p) =>
          p.messageType === sx.RpcMessage.ResetResponse &&
          p.txId.id === resetTx &&
          p.message.status === sx.ResetStatus.FINISHED
      );
      const afterReset = server.lines
        .filter((l) => l.trackers?.length === 6 && l.reset_count > 0)
        .at(-1);
      client.sendRPC(
        sx.RpcMessage.ChangeSkeletonConfigRequest,
        new sx.ChangeSkeletonConfigRequestT(sx.SkeletonBone.HIPS_WIDTH, 0.32)
      );
      const shape = await client.rpc(
        sx.RpcMessage.SkeletonConfigRequest,
        new sx.SkeletonConfigRequestT(),
        sx.RpcMessage.SkeletonConfigResponse
      );
      assert.ok(
        Math.abs(
          shape.skeletonParts.find((p: any) => p.bone === sx.SkeletonBone.HIPS_WIDTH)
            .value - 0.32
        ) < 1e-6
      );
      const change = new sx.ChangeSettingsRequestT();
      change.filtering = new sx.FilteringSettingsT(sx.FilteringType.SMOOTHING, 0.35);
      client.sendRPC(sx.RpcMessage.ChangeSettingsRequest, change);
      const filtered = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.equal(filtered.filtering.type, sx.FilteringType.SMOOTHING);
      client.sendRPC(
        sx.RpcMessage.SetPauseTrackingRequest,
        new sx.SetPauseTrackingRequestT(true)
      );
      const pause = await client.rpc(
        sx.RpcMessage.TrackingPauseStateRequest,
        new sx.TrackingPauseStateRequestT(),
        sx.RpcMessage.TrackingPauseStateResponse
      );
      assert.equal(pause.trackingPaused, true);
      client.sendRPC(
        sx.RpcMessage.SetPauseTrackingRequest,
        new sx.SetPauseTrackingRequestT(false)
      );
      client.socket.send(
        JSON.stringify({
          type: 'pose_input',
          body: 'head',
          rotation: { w: 1, x: 0, y: 0, z: 0 },
          position: { x: 0, y: 1.7, z: 0 },
        })
      );
      const hmd = (
        await client.wait(
          (p) =>
            p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
            p.message.devices.some((d: any) => d.id.id === 0)
        )
      ).message;
      assert.equal(
        hmd.devices.find((d: any) => d.id.id === 0).trackers[0].info.isHmd,
        true
      );
      // Train through the real worker and require ordered epoch messages before completion.
      const autoSettings = new sx.ChangeSettingsRequestT();
      autoSettings.autoBoneSettings = new sx.AutoBoneSettingsT();
      autoSettings.autoBoneSettings.sampleCount = 8;
      autoSettings.autoBoneSettings.sampleRateMs = 8n;
      autoSettings.autoBoneSettings.numEpochs = 3;
      autoSettings.autoBoneSettings.calcInitError = true;
      client.sendRPC(sx.RpcMessage.ChangeSettingsRequest, autoSettings);
      await client.rpc(
        sx.RpcMessage.HeartbeatRequest,
        new sx.HeartbeatRequestT(),
        sx.RpcMessage.HeartbeatResponse
      );
      client.sendRPC(
        sx.RpcMessage.AutoBoneProcessRequest,
        new sx.AutoBoneProcessRequestT(sx.AutoBoneProcessType.RECORD)
      );
      client.sendRPC(
        sx.RpcMessage.AutoBoneProcessRequest,
        new sx.AutoBoneProcessRequestT(sx.AutoBoneProcessType.SAVE)
      );
      await client.wait(
        (p) =>
          p.messageType === sx.RpcMessage.AutoBoneProcessStatusResponse &&
          p.message.processType === sx.AutoBoneProcessType.RECORD &&
          p.message.completed
      );
      const autoPath = join(dir, 'AutoBone Recordings', 'LastABRecording.pfs');
      const savedAuto = await client.wait(
        (p) =>
          p?.type === 'backend_file_saved' &&
          p.kind === 'autobone' &&
          p.file.path === autoPath
      );
      assert.equal(savedAuto.file.frames, 8);
      const originalBytes = await readFile(autoPath);
      assert.equal(originalBytes[0], 0); // Original PFS frame-interval packet.
      await client.wait(
        (p) =>
          p.messageType === sx.RpcMessage.AutoBoneProcessStatusResponse &&
          p.message.processType === sx.AutoBoneProcessType.SAVE &&
          p.message.completed &&
          p.message.success
      );
      assert.deepEqual(
        await readFile(join(dir, 'AutoBone Recordings', 'ABRecording1.pfs')),
        originalBytes
      );
      client.sendRPC(
        sx.RpcMessage.AutoBoneProcessRequest,
        new sx.AutoBoneProcessRequestT(sx.AutoBoneProcessType.SAVE)
      );
      await client.wait(
        (p) =>
          p?.type === 'backend_file_saved' && p.file.path.endsWith('ABRecording2.pfs')
      );
      assert.deepEqual(
        await readFile(join(dir, 'AutoBone Recordings', 'ABRecording1.pfs')),
        originalBytes
      );

      client.sendRPC(
        sx.RpcMessage.AutoBoneProcessRequest,
        new sx.AutoBoneProcessRequestT(sx.AutoBoneProcessType.PROCESS)
      );
      const training = [];
      for (;;) {
        const packet = await client.wait(
          (p) =>
            p.messageType === sx.RpcMessage.AutoBoneEpochResponse ||
            (p.messageType === sx.RpcMessage.AutoBoneProcessStatusResponse &&
              p.message.processType === sx.AutoBoneProcessType.PROCESS &&
              p.message.completed)
        );
        if (packet.messageType === sx.RpcMessage.AutoBoneProcessStatusResponse) break;
        training.push(packet.message);
      }
      assert.deepEqual(
        training.map((e) => e.currentEpoch),
        [0, 1, 2, 3]
      );
      assert.ok(
        training.every(
          (e) =>
            e.totalEpochs === 3 &&
            Number.isFinite(e.epochError) &&
            e.adjustedSkeletonParts.length > 0
        )
      );
      // Sparse polls must not leak unrequested tracker fields.
      const sparse = new sx.DataFeedConfigT();
      const mask = new sx.TrackerDataMaskT();
      mask.status = true;
      sparse.dataMask = new sx.DeviceDataMaskT(mask, false);
      const bundle = new sx.MessageBundleT();
      bundle.dataFeedMsgs = [
        new sx.DataFeedMessageHeaderT(
          sx.DataFeedMessage.PollDataFeed,
          new sx.PollDataFeedT(sparse)
        ),
      ];
      client.packets = [];
      client.send(bundle);
      const masked = (
        await client.wait(
          (p) =>
            p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
            p.message.devices[0]?.trackers[0]?.info === null
        )
      ).message;
      assert.equal(masked.devices[0].trackers[0].rotation, null);
      assert.equal(masked.devices[0].hardwareInfo, null);
      // Invalid bone updates report failure and leave the accepted configuration intact.
      client.sendRPC(
        sx.RpcMessage.ChangeSkeletonConfigRequest,
        new sx.ChangeSkeletonConfigRequestT(sx.SkeletonBone.HIPS_WIDTH, -1)
      );
      await client.wait((p) => p?.type === 'backend_error');
      const unchanged = await client.rpc(
        sx.RpcMessage.SkeletonConfigRequest,
        new sx.SkeletonConfigRequestT(),
        sx.RpcMessage.SkeletonConfigResponse
      );
      assert.deepEqual(unchanged, shape);
      const temporary = new sx.LegTweaksTmpChangeT(false, false, false, false);
      client.sendRPC(sx.RpcMessage.LegTweaksTmpChange, temporary);
      const temporarySettings = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.equal(
        temporarySettings.modelSettings.toggles.floorClip,
        settings.modelSettings.toggles.floorClip
      );
      client.sendRPC(
        sx.RpcMessage.LegTweaksTmpClear,
        new sx.LegTweaksTmpClearT(true, true, true, true)
      );
      client.sendRPC(
        sx.RpcMessage.ForgetDeviceRequest,
        new sx.ForgetDeviceRequestT('02:00:00:00:00:06')
      );
      await client.wait(
        (p) =>
          p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
          p.message.devices.filter((d: any) => d.id.id !== 0).length === 5
      );
      client.socket.close();
      await server.stop();
      stopped = true;
      const last = server.lines
        .filter((l) => l.trackers?.length === 5 && l.skeleton)
        .at(-1);
      assert.equal(last.reset_count, 1);
      assert.ok(last.trackers.every((t: any) => t.calibration.full_reset_done));
      assert.ok(!afterReset || afterReset.reset_count === 1);
      const replay = spawnSync(binary, ['solve-recording', journal, '--config', seed], {
        encoding: 'utf8',
        maxBuffer: 32 * 1024 * 1024,
      });
      assert.equal(replay.status, 0, replay.stderr);
      const replayed = JSON.parse(replay.stdout.trim().split('\n')[0]);
      const livePose = { ...last };
      delete livePose.level; // Severity is a live diagnostic envelope, not pose data.
      assert.deepEqual(replayed, livePose);
      const saved = await readFile(state, 'utf8');
      assert.equal((saved.match(/designation: body:/g) ?? []).length, 5);
      assert.equal((saved.match(/^- 02:00:00:00:00:/gm) ?? []).length, 5);
      assert.match(saved, /customName: Tracker 0/);
      assert.ok(!saved.includes('udp://02:00:00:00:00:06/0'));
      assert.ok(!saved.includes('\npose:'));
      const restart = await start(['--config', state]);
      try {
        const reconnected = new Client(restart.info.api_bind);
        clients.push(reconnected);
        await reconnected.wait((p) => p?.type === 'backend_info');
        const restored = await reconnected.rpc(
          sx.RpcMessage.SettingsRequest,
          new sx.SettingsRequestT(),
          sx.RpcMessage.SettingsResponse
        );
        assert.equal(restored.filtering.type, sx.FilteringType.SMOOTHING);
        // Restarted training can reuse LastABRecording without live trackers or HMD.
        reconnected.sendRPC(
          sx.RpcMessage.AutoBoneProcessRequest,
          new sx.AutoBoneProcessRequestT(sx.AutoBoneProcessType.PROCESS)
        );
        const resumed = await reconnected.wait(
          (p) =>
            p.messageType === sx.RpcMessage.AutoBoneProcessStatusResponse &&
            p.message.processType === sx.AutoBoneProcessType.PROCESS &&
            p.message.completed
        );
        assert.equal(resumed.message.success, true);
        // Original import directory takes precedence over the valid saved recording.
        await mkdir(join(dir, 'Load AutoBone Recordings'));
        await writeFile(
          join(dir, 'Load AutoBone Recordings', 'bad.pfs'),
          Buffer.from([0])
        );
        reconnected.sendRPC(
          sx.RpcMessage.AutoBoneProcessRequest,
          new sx.AutoBoneProcessRequestT(sx.AutoBoneProcessType.PROCESS)
        );
        const failedImport = await reconnected.wait(
          (p) =>
            p.messageType === sx.RpcMessage.AutoBoneProcessStatusResponse &&
            p.message.processType === sx.AutoBoneProcessType.PROCESS &&
            p.message.completed
        );
        assert.equal(failedImport.message.success, false);
        await reconnected.wait((p) => p?.type === 'backend_error');
        reconnected.sendRPC(
          sx.RpcMessage.AutoBoneApplyRequest,
          new sx.AutoBoneApplyRequestT()
        );
        const refusedApply = await reconnected.wait((p) => p?.type === 'backend_error');
        assert.match(refusedApply.message, /no accepted AutoBone/);
        await rm(join(dir, 'Load AutoBone Recordings'), { recursive: true });
        reconnected.feed(config());
        const [restartHost, restartPort] = restart.info.bind.split(':');
        sockets[0].send(handshake(1), Number(restartPort), restartHost);
        sockets[0].send(
          wire(15, 1, Buffer.from([0, 1, 13])),
          Number(restartPort),
          restartHost
        );
        const restoredDevice = (
          await reconnected.wait(
            (p) =>
              p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
              p.message.devices.some(
                (d: any) => d.trackers[0]?.info?.bodyPart === sx.BodyPart.CHEST
              )
          )
        ).message.devices.find(
          (d: any) => d.trackers[0]?.info?.bodyPart === sx.BodyPart.CHEST
        );
        assert.equal(restoredDevice.id.id, initial.devices[0].id.id);
        reconnected.socket.close();
      } finally {
        await restart.stop();
      }
    } finally {
      clients.forEach((c) => c.socket.close());
      sockets.forEach((s) => s.close());
      if (!stopped) {
        server.child.kill('SIGKILL');
        await once(server.child, 'exit');
      }
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'BVH RPC streams files, synchronizes clients and finalizes on rig change and shutdown',
  { timeout: 15000 },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-bvh-api-'));
    const server = await start(['--config', join(dir, 'vrconfig.yml')]);
    const clients: Client[] = [];
    let stopped = false;
    const status = async (client: Client) =>
      client.rpc(
        sx.RpcMessage.RecordBVHStatusRequest,
        new sx.RecordBVHStatusRequestT(),
        sx.RpcMessage.RecordBVHStatus
      );
    const record = async (client: Client, stop: boolean, path: string | null = null) =>
      client.rpc(
        sx.RpcMessage.RecordBVHRequest,
        new sx.RecordBVHRequestT(stop, path),
        sx.RpcMessage.RecordBVHStatus
      );
    const assertClip = async (path: string, frames?: number) => {
      const text = await readFile(path, 'utf8');
      assert.match(text, /^HIERARCHY\nROOT HIP\n/);
      assert.ok(
        text.includes(
          'CHANNELS 6 Xposition Yposition Zposition Zrotation Xrotation Yrotation'
        )
      );
      assert.ok(text.includes('JOINT LEFT_INDEX_DISTAL'));
      const count = Number(text.match(/Frames: (\d+)/)![1]);
      const rows = text.split('Frame Time: 0.01\n')[1].trim().split('\n');
      assert.equal(rows.length, count);
      if (frames !== undefined) assert.equal(count, frames);
      assert.ok(count > 0);
      const channels = 3 + (text.match(/CHANNELS /g)?.length ?? 0) * 3;
      for (const row of rows) {
        const values = row.trim().split(/\s+/).map(Number);
        assert.equal(values.length, channels);
        assert.ok(values.every(Number.isFinite));
        assert.ok(Math.abs(values[0] - 0.2) < 0.00001);
        assert.ok(values[1] > 0.5 && values[1] < 2); // Metres, not centimetres.
      }
      return count;
    };
    try {
      const client = new Client(server.info.api_bind);
      clients.push(client);
      const info = await client.wait((p) => p?.type === 'backend_info');
      assert.ok(info.info.capabilities.includes('bvh'));
      client.socket.send(
        JSON.stringify({
          type: 'pose_input',
          body: 'head',
          rotation: { w: 1, x: 0, y: 0, z: 0 },
          position: { x: 0.2, y: 1.7, z: -0.1 },
        })
      );
      await delay(30);
      assert.equal((await status(client)).recording, false);
      assert.equal((await record(client, false)).recording, true);
      // A new connection and repeated start share the active recording.
      const observer = new Client(server.info.api_bind);
      clients.push(observer);
      await observer.wait((p) => p?.type === 'backend_info');
      assert.equal((await status(observer)).recording, true);
      assert.equal(
        (await record(observer, false, join(dir, 'unused.bvh'))).recording,
        true
      );
      client.sendRPC(
        sx.RpcMessage.SetPauseTrackingRequest,
        new sx.SetPauseTrackingRequestT(true)
      );
      await delay(60);
      assert.equal((await record(observer, true)).recording, false);
      const saved = await client.wait((p) => p?.type === 'backend_file_saved');
      assert.equal(saved.file.path, join(dir, 'recordings', 'BVH-Recording1.bvh'));
      await assertClip(saved.file.path, saved.file.frames);
      await client.wait(
        (p) =>
          p.messageType === sx.RpcMessage.RecordBVHStatus &&
          p.txId?.id === 0 &&
          !p.message.recording
      );
      assert.equal((await record(client, true)).recording, false);
      client.sendRPC(
        sx.RpcMessage.SetPauseTrackingRequest,
        new sx.SetPauseTrackingRequestT(false)
      );
      // A rejected path leaves an existing file intact and recording idle.
      const existing = join(dir, 'existing.bvh');
      await writeFile(existing, 'do not overwrite');
      client.sendRPC(
        sx.RpcMessage.RecordBVHRequest,
        new sx.RecordBVHRequestT(false, existing)
      );
      await client.wait((p) => p?.type === 'backend_error');
      assert.equal(await readFile(existing, 'utf8'), 'do not overwrite');
      assert.equal((await status(client)).recording, false);
      // Native save-dialog paths and fixed-rig lifecycle.
      const explicit = join(dir, 'chosen.bvh');
      assert.equal((await record(client, false, explicit)).recording, true);
      await delay(40);
      client.sendRPC(
        sx.RpcMessage.ChangeSkeletonConfigRequest,
        new sx.ChangeSkeletonConfigRequestT(sx.SkeletonBone.HIPS_WIDTH, 0.32)
      );
      await client.wait(
        (p) =>
          p?.type === 'backend_error' &&
          p.message.includes('bone lengths or hierarchy changed')
      );
      const changed = await client.wait(
        (p) => p?.type === 'backend_file_saved' && p.file.path === explicit
      );
      await assertClip(explicit, changed.file.frames);
      assert.equal((await status(client)).recording, false);
      // Ctrl-C finalizes the current count without a stop RPC.
      assert.equal((await record(client, false)).recording, true);
      await delay(70);
      await server.stop();
      stopped = true;
      await assertClip(join(dir, 'recordings', 'BVH-Recording2.bvh'));
    } finally {
      clients.forEach((c) => c.socket.close());
      if (!stopped) await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'desktop parent pipe closure cleanly finalizes an active BVH and replay journal',
  { timeout: 10000 },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-bvh-parent-'));
    const journal = join(dir, 'journal.jsonl');
    const server = await start([
      '--config',
      join(dir, 'vrconfig.yml'),
      '--record',
      journal,
      '--shutdown-on-stdin-eof',
    ]);
    const client = new Client(server.info.api_bind);
    let stopped = false;
    try {
      await client.wait((p) => p?.type === 'backend_info');
      await delay(20);
      const state = await client.rpc(
        sx.RpcMessage.RecordBVHRequest,
        new sx.RecordBVHRequestT(false),
        sx.RpcMessage.RecordBVHStatus
      );
      assert.equal(state.recording, true);
      await delay(50);
      const exited = once(server.child, 'exit');
      server.child.stdin!.end();
      const [code] = await exited;
      stopped = true;
      assert.equal(code, 0);
      const clip = await readFile(
        join(dir, 'recordings', 'BVH-Recording1.bvh'),
        'utf8'
      );
      const count = Number(clip.match(/Frames: (\d+)/)![1]);
      assert.ok(count > 0);
      assert.equal(
        clip.split('Frame Time: 0.01\n')[1].trim().split('\n').length,
        count
      );
      const replay = spawnSync(binary, ['replay', journal], { encoding: 'utf8' });
      assert.equal(replay.status, 0, replay.stderr);
    } finally {
      client.socket.close();
      if (!stopped) await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'original vrconfig.yml supplies GUI settings and assignments and preserves unsupported fields through edits and reset',
  { timeout: 15000 },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-original-config-'));
    const path = join(dir, 'vrconfig.yml');
    await writeFile(
      path,
      await readFile(
        '../server-rust/crates/slimevr-server/tests/fixtures/vrconfig-v15.yml',
        'utf8'
      )
    );
    const server = await start(['--config', path]);
    const client = new Client(server.info.api_bind);
    const sockets: ReturnType<typeof createSocket>[] = [];
    try {
      await client.wait((p) => p?.type === 'backend_info');
      const settings = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.equal(settings.filtering.type, sx.FilteringType.SMOOTHING);
      assert.ok(Math.abs(settings.filtering.amount - 0.35) < 0.000001);
      assert.equal(
        settings.resetsSettings.armsMountingResetMode,
        sx.ArmsMountingResetMode.TPOSE_UP
      );
      assert.equal(settings.modelSettings.toggles.extendedSpine, false);
      assert.equal(settings.autoBoneSettings.sampleCount, 1200);
      assert.equal(settings.resetsSettings.resetHmdPitch, true);
      assert.equal(settings.resetsSettings.resetMountingFeet, true);
      assert.equal(settings.stayAligned.extraYawCorrection, false);
      client.feed(config());
      const [host, port] = server.info.bind.split(':');
      for (let n = 1; n <= 2; n++) {
        const socket = createSocket('udp4');
        sockets.push(socket);
        await new Promise<void>((r) => socket.bind(0, '127.0.0.1', r));
        socket.send(handshake(n), Number(port), host);
        socket.send(wire(15, 1, Buffer.from([n - 1, 1, 13])), Number(port), host);
        const payload = Buffer.alloc(19);
        payload[0] = n - 1;
        payload[1] = 1;
        payload.writeFloatBE(1, 14);
        payload[18] = 3;
        socket.send(wire(17, 2, payload), Number(port), host);
      }
      const feed = (
        await client.wait(
          (p) =>
            p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
            p.message.devices.filter((d: any) => d.trackers[0]?.rotation).length === 2
        )
      ).message;
      assert.equal(feed.devices[0].trackers[0].info.bodyPart, sx.BodyPart.CHEST);
      assert.equal(
        feed.devices[1].trackers[0].info.bodyPart,
        sx.BodyPart.LEFT_LOWER_LEG
      );
      assert.equal(feed.devices[1].trackers[0].trackerId.trackerNum, 1);
      const change = new sx.ChangeSettingsRequestT();
      change.filtering = new sx.FilteringSettingsT(sx.FilteringType.PREDICTION, 0.6);
      change.resetsSettings = new sx.ResetsSettingsT();
      change.resetsSettings.resetHmdPitch = true;
      change.resetsSettings.resetMountingFeet = false;
      change.stayAligned = new sx.StayAlignedSettingsT();
      change.stayAligned.extraYawCorrection = true;
      change.autoBoneSettings = new sx.AutoBoneSettingsT();
      change.autoBoneSettings.saveRecordings = true;
      change.autoBoneSettings.calcInitError = true;
      change.autoBoneSettings.useSkeletonHeight = true;
      client.sendRPC(sx.RpcMessage.ChangeSettingsRequest, change);
      const updated = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.equal(updated.filtering.type, sx.FilteringType.PREDICTION);
      assert.equal(updated.resetsSettings.resetHmdPitch, true);
      assert.equal(updated.resetsSettings.resetMountingFeet, false);
      assert.equal(updated.stayAligned.extraYawCorrection, false);
      assert.equal(updated.autoBoneSettings.saveRecordings, true);
      assert.equal(updated.autoBoneSettings.calcInitError, true);
      assert.equal(updated.autoBoneSettings.useSkeletonHeight, true);
      let text = await readFile(path, 'utf8');
      assert.match(text, /type: prediction/);
      assert.match(text, /saveRecordings: true/);
      for (const content of [
        'description: Leave this alone',
        'customProperty:',
        'hid://device/0',
        'resetHmdPitch: true',
        'correctConstraints: true',
        'printEveryNumEpochs: 5',
        'customRoute:',
      ])
        assert.ok(text.includes(content), content);
      // Reset affects implemented settings while keeping unsupported YAML sections and assignments.
      client.sendRPC(
        sx.RpcMessage.SettingsResetRequest,
        new sx.SettingsResetRequestT()
      );
      const reset = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.equal(reset.autoBoneSettings.sampleCount, 1500);
      assert.equal(reset.modelSettings.toggles.extendedSpine, true);
      text = await readFile(path, 'utf8');
      assert.match(text, /description: Leave this alone/);
      assert.match(text, /customRoute:/);
      assert.match(text, /designation: body:chest/);
      assert.match(text, /designation: body:left_lower_leg/);
      await assert.rejects(readFile(join(dir, 'rust-backend.json')));
      assert.ok((await readFile(path + '.bak', 'utf8')).includes('customRoute:'));
    } finally {
      client.socket.close();
      sockets.forEach((s) => s.close());
      await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'sensor 0 tap assignment and magnetometer commands use native settings and firmware ACK',
  { timeout: 15000 },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-device-controls-'));
    const path = join(dir, 'vrconfig.yml');
    const server = await start([
      '--config',
      path,
      '--accept-new-devices',
      '--log-level',
      'debug',
    ]);
    const client = new Client(server.info.api_bind);
    const udp = createSocket('udp4');
    const commands: Buffer[] = [];
    udp.on('message', (bytes) => {
      if (bytes.length >= 4 && bytes.readUInt32BE(0) === 25) commands.push(bytes);
    });
    try {
      await client.wait((p) => p?.type === 'backend_info');
      await new Promise<void>((r) => udp.bind(0, '127.0.0.1', r));
      const [host, port] = server.info.bind.split(':');
      let seq = 1;
      const send = (id: number, body: Buffer) =>
        udp.send(wire(id, seq++, body), Number(port), host);
      udp.send(handshake(1), Number(port), host);
      await delay(30);
      send(15, Buffer.from([0, 1, 13, 0, 2, 1, 0, 0]));
      client.feed(config());
      const device = (
        await client.wait(
          (p) =>
            p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
            p.message.devices.some(
              (d: any) =>
                d.trackers[0]?.info?.magnetometer === sx.MagnetometerStatus.DISABLED
            )
        )
      ).message.devices.find((d: any) => d.id.id !== 0);
      const tracker = new sx.TrackerIdT(new sx.DeviceIdT(device.id.id), 0);
      const toggle = client.sendRPC(
        sx.RpcMessage.ChangeMagToggleRequest,
        new sx.ChangeMagToggleRequestT(null, true)
      );
      for (let n = 0; n < 100 && !commands.length; n++) await delay(10);
      assert.equal(
        commands[0]?.toString('hex'),
        wire(25, 0, Buffer.from([0, 0, 1, 1])).toString('hex')
      );
      assert.ok(
        !client.packets.some(
          (p) =>
            p.messageType === sx.RpcMessage.MagToggleResponse && p.txId?.id === toggle
        )
      );
      send(24, Buffer.from([0, 0, 1]));
      assert.equal(
        (
          await client.wait(
            (p) =>
              p.messageType === sx.RpcMessage.MagToggleResponse && p.txId?.id === toggle
          )
        ).message.enable,
        true
      );
      await client.wait(
        (p) =>
          p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
          p.message.devices.some(
            (d: any) =>
              d.trackers[0]?.info?.magnetometer === sx.MagnetometerStatus.ENABLED
          )
      );
      const per = client.sendRPC(
        sx.RpcMessage.ChangeMagToggleRequest,
        new sx.ChangeMagToggleRequestT(tracker, false)
      );
      for (let n = 0; n < 100 && commands.length < 2; n++) await delay(10);
      assert.equal(commands[1]?.at(-1), 0);
      send(24, Buffer.from([0, 0, 1]));
      assert.equal(
        (
          await client.wait(
            (p) =>
              p.messageType === sx.RpcMessage.MagToggleResponse && p.txId?.id === per
          )
        ).message.enable,
        false
      );
      const setup = new sx.ChangeSettingsRequestT();
      setup.tapDetectionSettings = new sx.TapDetectionSettingsT();
      setup.tapDetectionSettings.setupMode = true;
      client.sendRPC(sx.RpcMessage.ChangeSettingsRequest, setup);
      await client.rpc(
        sx.RpcMessage.HeartbeatRequest,
        new sx.HeartbeatRequestT(),
        sx.RpcMessage.HeartbeatResponse
      );
      const accel = (value: number) => {
        const body = Buffer.alloc(13);
        body.writeFloatBE(value, 0);
        send(4, body);
      };
      accel(0);
      await delay(25);
      accel(10);
      await delay(25);
      accel(0);
      await delay(100);
      accel(10);
      const tap = (
        await client.wait(
          (p) => p.messageType === sx.RpcMessage.TapDetectionSetupNotification
        )
      ).message;
      assert.equal(tap.trackerId.trackerNum, 0);
      assert.equal(tap.trackerId.deviceId.id, device.id.id);
      assert.ok(server.lines.some((p) => p.type === 'pose_snapshot'));
      assert.ok(!server.lines.some((p) => p.reset_count > 0));
      setup.tapDetectionSettings.setupMode = false;
      client.sendRPC(sx.RpcMessage.ChangeSettingsRequest, setup);
      await client.rpc(
        sx.RpcMessage.HeartbeatRequest,
        new sx.HeartbeatRequestT(),
        sx.RpcMessage.HeartbeatResponse
      );
      client.packets = [];
      accel(0);
      await delay(100);
      accel(10);
      await delay(30);
      accel(0);
      await delay(100);
      accel(10);
      await delay(50);
      assert.ok(
        !client.packets.some(
          (p) => p.messageType === sx.RpcMessage.TapDetectionSetupNotification
        )
      );
      const yaml = await readFile(path, 'utf8');
      assert.match(yaml, /useMagnetometerOnAllTrackers: true/);
      assert.match(yaml, /shouldHaveMagEnabled: false/);
      assert.match(yaml, /setupMode: false/);
      client.sendRPC(
        sx.RpcMessage.ForgetDeviceRequest,
        new sx.ForgetDeviceRequestT('02:00:00:00:00:01')
      );
      await client.rpc(
        sx.RpcMessage.HeartbeatRequest,
        new sx.HeartbeatRequestT(),
        sx.RpcMessage.HeartbeatResponse
      );
      assert.ok(!(await readFile(path, 'utf8')).includes('udp://02:00:00:00:00:01/0'));
    } finally {
      client.socket.close();
      udp.close();
      await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'SteamVR enable RPC updates the original checklist using actual driver state',
  { timeout: 15000, skip: process.platform === 'win32' },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-driver-management-'));
    let enabled = false,
      blocked = true;
    const routes: string[] = [];
    const http = createServer((req, res) => {
      let body = '';
      req.on('data', (b) => (body += b));
      req.on('end', () => {
        routes.push(`${req.method} ${req.url}`);
        assert.ok(req.headers.referer?.endsWith('/dashboard/index.html'));
        if (req.url === '/drivers/unblock') {
          assert.equal(JSON.parse(body).driver, 'slimevr');
          blocked = false;
        }
        if (req.url === '/drivers/setenable') {
          enabled = JSON.parse(body).enable;
        }
        res.setHeader('Content-Type', 'application/json');
        res.end(
          JSON.stringify({
            jsonid: 'vr_driver_list',
            drivers: [
              { enabled, blocked_by_safe_mode: blocked, manifest: { name: 'slimevr' } },
            ],
          })
        );
      });
    });
    http.listen(0, '127.0.0.1');
    await once(http, 'listening');
    const address = http.address() as { port: number };
    const server = await start([
      '--config',
      join(dir, 'vrconfig.yml'),
      '--steamvr-endpoint',
      join(dir, 'driver'),
      '--steamvr-http',
      `http://127.0.0.1:${address.port}`,
      '--no-driver-install',
      '--no-steamvr-restart',
      '--no-bindings-provider',
    ]);
    const client = new Client(server.info.api_bind);
    const extra = (p: any) =>
      p.message?.steps?.find(
        (s: any) => s.id === sx.TrackingChecklistStepId.STEAMVR_DISCONNECTED
      )?.extraData;
    try {
      await client.wait((p) => p?.type === 'backend_info');
      for (let n = 0; n < 20; n++) {
        client.sendRPC(
          sx.RpcMessage.TrackingChecklistRequest,
          new sx.TrackingChecklistRequestT()
        );
        if (routes.length) break;
        await delay(20);
      }
      const initial = extra(
        await client.wait(
          (p) =>
            p.messageType === sx.RpcMessage.TrackingChecklistResponse &&
            extra(p)?.driverBlockedBySafeMode
        )
      );
      assert.equal(initial.driverInstalled, true);
      assert.equal(initial.driverEnabled, false);
      client.sendRPC(
        sx.RpcMessage.EnableSteamVRDriverRequest,
        new sx.EnableSteamVRDriverRequestT()
      );
      await client.wait(
        (p) =>
          p.messageType === sx.RpcMessage.TrackingChecklistResponse &&
          extra(p)?.driverEnabled &&
          !extra(p)?.driverBlockedBySafeMode
      );
      assert.ok(routes.includes('POST /drivers/unblock'));
      assert.ok(routes.includes('POST /drivers/setenable'));
    } finally {
      client.socket.close();
      await server.stop();
      http.closeAllConnections();
      await new Promise<void>((r) => http.close(() => r()));
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'overlay pub/sub routes mapped topics between clients and persists display settings',
  { timeout: 15000 },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-overlay-'));
    const path = join(dir, 'vrconfig.yml');
    const server = await start(['--config', path]);
    const a = new Client(server.info.api_bind);
    const b = new Client(server.info.api_bind);
    const outsider = new Client(server.info.api_bind);
    const pub = (
      client: Client,
      type: sx.PubSubUnion,
      value: sx.PubSubHeaderT['u']
    ) => {
      const bundle = new sx.MessageBundleT();
      bundle.pubSubMsgs = [new sx.PubSubHeaderT(type, value)];
      client.send(bundle);
    };
    try {
      await Promise.all([
        once(a.socket, 'open'),
        once(b.socket, 'open'),
        once(outsider.socket, 'open'),
      ]);
      const topic = new sx.TopicIdT('SlimeVR', 'Overlay', 'tracking');
      pub(
        a,
        sx.PubSubUnion.SubscriptionRequest,
        new sx.SubscriptionRequestT(sx.Topic.TopicId, topic)
      );
      pub(
        b,
        sx.PubSubUnion.SubscriptionRequest,
        new sx.SubscriptionRequestT(sx.Topic.TopicId, topic)
      );
      const ma = await a.wait((p) => p.uType === sx.PubSubUnion.TopicMapping);
      const mb = await b.wait((p) => p.uType === sx.PubSubUnion.TopicMapping);
      assert.equal(ma.u.handle.id, mb.u.handle.id);
      pub(
        a,
        sx.PubSubUnion.Message,
        new sx.MessageT(
          sx.Topic.TopicHandle,
          new sx.TopicHandleT(ma.u.handle.id),
          sx.Payload.KeyValues,
          new sx.KeyValuesT(['trackers'], ['6'])
        )
      );
      const message = await b.wait((p) => p.uType === sx.PubSubUnion.Message);
      assert.deepEqual(message.u.payload.keys, ['trackers']);
      assert.deepEqual(message.u.payload.values, ['6']);
      await delay(100);
      assert.ok(!a.packets.some((p) => p.uType === sx.PubSubUnion.Message));
      assert.ok(!outsider.packets.some((p) => p.uType === sx.PubSubUnion.Message));
      pub(b, sx.PubSubUnion.Message, new sx.MessageT(sx.Topic.TopicId, topic));
      assert.equal(
        (await a.wait((p) => p.uType === sx.PubSubUnion.Message)).u.payloadType,
        sx.Payload.NONE
      );
      const current = await a.rpc(
        sx.RpcMessage.OverlayDisplayModeRequest,
        new sx.OverlayDisplayModeRequestT(),
        sx.RpcMessage.OverlayDisplayModeResponse
      );
      assert.equal(typeof current.isVisible, 'boolean');
      a.sendRPC(
        sx.RpcMessage.OverlayDisplayModeChangeRequest,
        new sx.OverlayDisplayModeChangeRequestT(false, true)
      );
      await a.wait(
        (p) =>
          p.messageType === sx.RpcMessage.OverlayDisplayModeResponse &&
          p.message.isMirrored === true &&
          p.message.isVisible === false
      );
      const yaml = await readFile(path, 'utf8');
      assert.match(yaml, /isMirrored: true/);
      assert.match(yaml, /isVisible: false/);
      const binds = await a.rpc(
        sx.RpcMessage.KeybindRequest,
        new sx.KeybindRequestT(),
        sx.RpcMessage.KeybindResponse
      );
      assert.equal(binds.keybind.length, 5);
      assert.equal(binds.keybind[0].keybindValue, 'CTRL+ALT+SHIFT+Y');
    } finally {
      a.socket.close();
      b.socket.close();
      outsider.socket.close();
      await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'native SolarXR and WebSocket share pub/sub topic mappings and delivery',
  { timeout: 15000, skip: process.platform === 'win32' },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-native-overlay-'));
    const server = await start([
      '--config',
      join(dir, 'vrconfig.yml'),
      '--steamvr-endpoint',
      join(dir, 'driver'),
      '--no-bindings-provider',
      '--no-driver-install',
      '--no-steamvr-restart',
    ]);
    const ws = new Client(server.info.api_bind);
    const native = createConnection(join(dir, 'SlimeVRRpc'));
    let pending = Buffer.alloc(0);
    const packets: any[] = [];
    native.on('data', (data) => {
      pending = Buffer.concat([pending, data]);
      while (pending.length >= 4 && pending.length >= pending.readUInt32LE(0)) {
        const size = pending.readUInt32LE(0);
        const bytes = pending.subarray(4, size);
        const bundle = decodeSolarXR(Uint8Array.from(bytes).buffer);
        packets.push(...bundle.pubSubMsgs);
        pending = pending.subarray(size);
      }
    });
    const send = (socket: Client | typeof native, header: sx.PubSubHeaderT) => {
      const bundle = new sx.MessageBundleT();
      bundle.pubSubMsgs = [header];
      if (socket instanceof Client) socket.send(bundle);
      else {
        const bytes = encodeSolarXR(bundle);
        const frame = Buffer.alloc(4 + bytes.length);
        frame.writeUInt32LE(frame.length);
        frame.set(bytes, 4);
        socket.write(frame);
      }
    };
    const wait = async (predicate: (p: any) => boolean) => {
      for (let i = 0; i < 300; i++) {
        const index = packets.findIndex(predicate);
        if (index >= 0) return packets.splice(index, 1)[0];
        await delay(10);
      }
      throw new Error('Native pub/sub timeout');
    };
    try {
      await Promise.all([once(ws.socket, 'open'), once(native, 'connect')]);
      const topic = new sx.TopicIdT('SlimeVR', 'Overlay', 'native');
      const subscribe = new sx.PubSubHeaderT(
        sx.PubSubUnion.SubscriptionRequest,
        new sx.SubscriptionRequestT(sx.Topic.TopicId, topic)
      );
      send(ws, subscribe);
      send(native, subscribe);
      const a = await ws.wait((p) => p.uType === sx.PubSubUnion.TopicMapping);
      const b = await wait((p) => p.uType === sx.PubSubUnion.TopicMapping);
      assert.equal(a.u.handle.id, b.u.handle.id);
      send(
        ws,
        new sx.PubSubHeaderT(
          sx.PubSubUnion.Message,
          new sx.MessageT(
            sx.Topic.TopicHandle,
            new sx.TopicHandleT(a.u.handle.id),
            sx.Payload.solarxr_protocol_datatypes_StringTable,
            new sx.StringTableT('from websocket')
          )
        )
      );
      assert.equal(
        (await wait((p) => p.uType === sx.PubSubUnion.Message)).u.payload.s,
        'from websocket'
      );
      send(
        native,
        new sx.PubSubHeaderT(
          sx.PubSubUnion.Message,
          new sx.MessageT(sx.Topic.TopicId, topic)
        )
      );
      assert.equal(
        (await ws.wait((p) => p.uType === sx.PubSubUnion.Message)).u.payloadType,
        sx.Payload.NONE
      );
    } finally {
      native.destroy();
      ws.socket.close();
      await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'OSC settings, VRM size limit and incoming VRChat head use the existing GUI protocol',
  { timeout: 20000 },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-osc-gui-'));
    const path = join(dir, 'vrconfig.yml');
    const udp = createSocket('udp4');
    udp.bind(0, '127.0.0.1');
    await once(udp, 'listening');
    const port = (udp.address() as { port: number }).port;
    udp.close();
    await once(udp, 'close');
    const source = createSocket('udp4');
    const server = await start(['--config', path]);
    const client = new Client(server.info.api_bind);
    try {
      await once(client.socket, 'open');
      const change = new sx.ChangeSettingsRequestT();
      change.vrcOsc = new sx.VRCOSCSettingsT(
        new sx.OSCSettingsT(true, port, 9000, '127.0.0.1'),
        null,
        false
      );
      change.vmcOsc = new sx.VMCOSCSettingsT(
        new sx.OSCSettingsT(false, 39540, 39539, '127.0.0.1'),
        false,
        true
      );
      change.vrm = new sx.VRMSettingsT(
        JSON.stringify({
          extensions: { VRMC_vrm: { humanoid: { humanBones: { hips: { node: 0 } } } } },
          nodes: [{ translation: [0, 1, 0] }],
          extras: { padding: 'x'.repeat(4 * 1024 * 1024 - 256) },
        })
      );
      client.sendRPC(sx.RpcMessage.ChangeSettingsRequest, change);
      const settings = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.equal(settings.vrcOsc.oscSettings.portIn, port);
      assert.equal(settings.vrcOsc.oscSettings.enabled, true);
      assert.equal(settings.vrcOsc.oscqueryEnabled, false);
      assert.equal(settings.vmcOsc.anchorHip, false);
      assert.equal(settings.vmcOsc.mirrorTracking, true);
      assert.equal(settings.vrm.vrmJson, change.vrm.vrmJson);
      client.feed(config());
      const string = (s: string) => {
        const b = Buffer.alloc(Math.ceil((Buffer.byteLength(s) + 1) / 4) * 4);
        b.write(s);
        return b;
      };
      const values = Buffer.alloc(24);
      [1, 1.7, 3, 0, 90, 0].forEach((v, i) => values.writeFloatBE(v, 4 * i));
      const packet = Buffer.concat([
        string('/tracking/vrsystem/head/pose'),
        string(',ffffff'),
        values,
      ]);
      await delay(100);
      source.send(packet, port, '127.0.0.1');
      const response = await client.wait(
        (p) =>
          p.messageType === sx.DataFeedMessage.DataFeedUpdate &&
          p.message.devices?.some((d: any) =>
            d.trackers?.some(
              (t: any) =>
                t.info?.bodyPart === sx.BodyPart.HEAD &&
                t.info.isImu === false &&
                Math.abs((t.position?.y ?? 0) - 1.7) < 1e-5
            )
          )
      );
      assert.ok(
        response.message.devices
          .flatMap((d: any) => d.trackers)
          .some((t: any) => t.position?.z === -3)
      );
      client.sendRPC(
        sx.RpcMessage.ClearMountingResetRequest,
        new sx.ClearMountingResetRequestT()
      );
      await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      change.vrcOsc.oscSettings!.enabled = false;
      change.vrm = new sx.VRMSettingsT('');
      client.sendRPC(sx.RpcMessage.ChangeSettingsRequest, change);
      const disabled = await client.rpc(
        sx.RpcMessage.SettingsRequest,
        new sx.SettingsRequestT(),
        sx.RpcMessage.SettingsResponse
      );
      assert.equal(disabled.vrcOsc.oscSettings.enabled, false);
      assert.equal(disabled.vrm.vrmJson, null);
      const yaml = await readFile(path, 'utf8');
      assert.match(yaml, /mirrorTracking: true/);
      assert.match(yaml, /VRChat head/);
    } finally {
      source.close();
      client.socket.close();
      await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test(
  'legacy JSON WebSocket preserves original tracker ordering, HMD offset and actions',
  { timeout: 15000 },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), 'slimevr-legacy-'));
    const server = await start(['--config', join(dir, 'vrconfig.yml')]);
    const client = new Client(server.info.api_bind);
    try {
      const notice = await client.wait((p) => p?.type === 'backend_info');
      assert.ok(notice.info.capabilities.includes('legacy_websocket'));
      const initialHeight = await client.rpc(
        sx.RpcMessage.HeightRequest,
        new sx.HeightRequestT(),
        sx.RpcMessage.HeightResponse
      );
      assert.equal(initialHeight.minHeight, 0);
      assert.equal(initialHeight.maxHeight, 0);
      const until = async (predicate: () => boolean) => {
        const deadline = Date.now() + 3000;
        while (!predicate()) {
          assert.ok(Date.now() < deadline, 'legacy response timed out');
          await delay(10);
        }
      };
      await until(() => client.legacy.length === 11);
      assert.deepEqual(
        client.legacy.map((p) => p.location),
        [
          'head',
          'chest',
          'waist',
          'left_knee',
          'right_knee',
          'left_foot',
          'right_foot',
          'left_elbow',
          'right_elbow',
          'left_hand',
          'right_hand',
        ]
      );
      assert.ok(
        client.legacy.every(
          (p, i) =>
            p.type === 'config' &&
            p.tracker_id === `SlimeVR Tracker ${i + 1}` &&
            p.tracker_type === p.location
        )
      );
      client.legacy = [];
      const input = {
        type: 'pos',
        tracker_id: 0,
        x: 0.3,
        y: 1.5,
        z: -0.1,
        qw: 1,
        qx: 0,
        qy: 0,
        qz: 0,
      };
      client.socket.send(JSON.stringify(input));
      await until(() => client.legacy.length === 11);
      await delay(30);
      client.legacy = [];
      client.socket.send(JSON.stringify(input));
      await until(() => client.legacy.length === 11);
      assert.ok(
        client.legacy.every(
          (p) => p.type === 'pos' && p.src === 'full' && Number.isFinite(p.qw)
        )
      );
      const head = client.legacy[0];
      // Original computed HEAD is at the neck endpoint (default neck length 0.1 m).
      assert.ok(Math.abs(head.y - 1.6) < 1e-5);
      assert.ok(Math.abs(head.x - 0.3) < 1e-5);
      const sourceHeight = await client.rpc(
        sx.RpcMessage.HeightRequest,
        new sx.HeightRequestT(),
        sx.RpcMessage.HeightResponse
      );
      assert.ok(Math.abs(sourceHeight.minHeight - 1.7) < 1e-5);
      assert.ok(Math.abs(sourceHeight.maxHeight - 1.7) < 1e-5);
      client.sendRPC(
        sx.RpcMessage.ClearDriftCompensationRequest,
        new sx.ClearDriftCompensationRequestT()
      );
      const pause = async () =>
        (
          await client.rpc(
            sx.RpcMessage.TrackingPauseStateRequest,
            new sx.TrackingPauseStateRequestT(),
            sx.RpcMessage.TrackingPauseStateResponse
          )
        ).trackingPaused;
      assert.equal(await pause(), false);
      client.socket.send(
        JSON.stringify({ type: 'action', name: 'toggle_pause_tracking' })
      );
      assert.equal(await pause(), true);
      client.socket.send(
        JSON.stringify({ type: 'action', name: 'toggle_pause_tracking' })
      );
      assert.equal(await pause(), false);
      for (const name of [
        'full_calibrate',
        'calibrate',
        'mounting_calibrate',
        'mounting_clear',
      ]) {
        client.socket.send(JSON.stringify({ type: 'action', name }));
        await client.rpc(
          sx.RpcMessage.SettingsRequest,
          new sx.SettingsRequestT(),
          sx.RpcMessage.SettingsResponse
        );
      }
      client.legacy = [];
      client.socket.send(JSON.stringify({ ...input, tracker_id: 9 }));
      client.socket.send(JSON.stringify({ type: 'config', tracker_id: 0 }));
      await pause();
      assert.equal(client.legacy.length, 0);
      assert.ok(
        !(await readFile(join(dir, 'vrconfig.yml'), 'utf8')).includes(
          'mounting_calibrate'
        )
      );
    } finally {
      client.socket.close();
      await server.stop();
      await rm(dir, { recursive: true, force: true });
    }
  }
);

test('invalid WebSocket input logs its reason while the receiver and other clients stay alive', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'slimevr-ws-diagnostic-'));
  const server = await start(['--config', join(dir, 'vrconfig.yml')]);
  const broken = new Client(server.info.api_bind);
  let healthy: Client | undefined;
  try {
    await once(broken.socket, 'open');
    const closed = once(broken.socket, 'close');
    broken.socket.send(new Uint8Array([0, 1, 2, 3]));
    await Promise.race([
      closed,
      delay(3000).then(() => {
        throw new Error('malformed client was not closed');
      }),
    ]);
    const deadline = Date.now() + 3000;
    while (!server.stderr.includes('api_connection_error') && Date.now() < deadline)
      await delay(10);
    const diagnostic = server.stderr
      .split('\n')
      .filter(Boolean)
      .map((line) => JSON.parse(line))
      .find((entry) => entry.type === 'api_connection_error');
    assert.equal(diagnostic?.error, 'invalid SolarXR frame');
    assert.equal(typeof diagnostic.client_id, 'number');
    assert.ok(diagnostic.peer.startsWith('127.0.0.1:'));
    assert.equal(server.child.exitCode, null);
    healthy = new Client(server.info.api_bind);
    await once(healthy.socket, 'open');
    await healthy.rpc(
      sx.RpcMessage.HeartbeatRequest,
      new sx.HeartbeatRequestT(),
      sx.RpcMessage.HeartbeatResponse
    );
  } finally {
    broken.socket.close();
    healthy?.socket.close();
    await server.stop();
    await rm(dir, { recursive: true, force: true });
  }
});
