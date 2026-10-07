import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  createLogger,
  isLogLevel,
  logLevels,
  type LogLevel,
} from '../src/platform/logging';

test('disabled details are neither serialized nor forwarded to IPC or console', () => {
  const persisted: unknown[] = [];
  const displayed: unknown[] = [];
  const output = (...args: unknown[]) => displayed.push(args);
  const logger = createLogger((level, args) => persisted.push([level, args]), {
    error: output,
    warn: output,
    info: output,
    debug: output,
  });
  const poison = {
    get data() {
      throw new Error('disabled data must not be serialized');
    },
  };
  logger.write('trace', [poison]);
  logger.write('debug', [poison]);
  assert.equal(displayed.length, 0);
  assert.equal(persisted.length, 0);
  logger.write('info', ['connected']);
  assert.deepEqual(persisted, [['info', ['connected']]]);
});

test('every threshold includes higher severities and preserves error details', () => {
  for (const threshold of logLevels) {
    const actual: LogLevel[] = [];
    const output = () => {};
    const logger = createLogger((level) => actual.push(level), {
      error: output,
      warn: output,
      info: output,
      debug: output,
    });
    logger.setLevel(threshold);
    for (const level of logLevels) logger.write(level, [level]);
    assert.deepEqual(actual, logLevels.slice(0, logLevels.indexOf(threshold) + 1));
  }
  let saved: unknown[] = [];
  const output = () => {};
  const logger = createLogger(
    (_level, args) => {
      saved = args;
    },
    { error: output, warn: output, info: output, debug: output }
  );
  logger.write('error', [new Error('disconnected', { cause: new Error('timeout') })]);
  const failure = saved[0] as { message: string; cause: { message: string } };
  assert.equal(failure.message, 'disconnected');
  assert.equal(failure.cause.message, 'timeout');
  assert.equal(isLogLevel('stdout'), false);
  assert.equal(isLogLevel('trace'), true);
});
