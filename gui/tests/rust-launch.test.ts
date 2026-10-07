import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { test } from 'node:test';
import { findRustServer, rustServerArgs } from '../electron/main/rust-server';

test('Rust discovery respects explicit executables and ignores old JAR files', async () => {
  const root = await mkdtemp(join(tmpdir(), 'slimevr-launch-'));
  try {
    const binary = join(
      root,
      process.platform === 'win32' ? 'slimevr-server.exe' : 'slimevr-server'
    );
    await writeFile(join(root, 'slimevr.jar'), 'legacy');
    assert.equal(findRustServer([root]), undefined);
    assert.throws(() => findRustServer([root], join(root, 'missing')), /not found/);
    await mkdir(binary);
    assert.equal(findRustServer([root]), undefined);
    await rm(binary, { recursive: true });
    await writeFile(binary, 'binary');
    assert.equal(findRustServer([root]), binary);
    const explicit = join(root, 'custom-backend');
    await writeFile(explicit, 'custom');
    assert.equal(findRustServer([root], explicit), resolve(explicit));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test('Rust launch reuses YAML configuration and enables graceful EOF shutdown', async () => {
  const root = await mkdtemp(join(tmpdir(), 'slimevr-launch-'));
  try {
    const yaml = join(root, 'vrconfig.yaml');
    const yml = join(root, 'vrconfig.yml');
    await writeFile(yaml, 'trackers: {}');
    const args = rustServerArgs(root, root);
    assert.equal(args[0], 'listen');
    assert.ok(args.includes('--shutdown-on-stdin-eof'));
    assert.equal(args[args.indexOf('--config') + 1], yaml);
    assert.equal(args[args.indexOf('--api-bind') + 1], '127.0.0.1:21110');
    await writeFile(yml, 'trackers: {}');
    const preferred = rustServerArgs(root, root);
    assert.equal(preferred[preferred.indexOf('--config') + 1], yml);
    const explicit = rustServerArgs(root, root, join(root, 'custom.yml'), 'debug');
    assert.equal(explicit[explicit.indexOf('--config') + 1], join(root, 'custom.yml'));
    assert.equal(explicit[explicit.indexOf('--log-level') + 1], 'debug');
    assert.ok(!explicit.includes('-jar'));
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
