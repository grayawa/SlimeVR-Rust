import assert from 'node:assert/strict';
import { test } from 'node:test';
import { listenToWebSocket } from '../src/platform/websocket-events';
import { serializeLogArgs } from '../src/platform/logging-values';

class FakeSocket extends EventTarget {
  callbacks = new Map<string, EventListener>();
  override addEventListener(type: string, listener: EventListener) {
    this.callbacks.set(type, listener);
    super.addEventListener(type, listener);
  }
  asWebSocket() {
    return this as unknown as WebSocket;
  }
}

test('old socket events queued before reconnect cannot change the new connection', () => {
  const oldSocket = new FakeSocket();
  const newSocket = new FakeSocket();
  let current = oldSocket;
  let connected = false;
  let messages = 0;
  let errors = 0;
  const handlers = {
    open: () => {
      connected = true;
    },
    close: () => {
      connected = false;
    },
    error: () => {
      errors++;
    },
    message: () => {
      messages++;
    },
  };
  const disposeOld = listenToWebSocket(
    oldSocket.asWebSocket(),
    () => current === oldSocket,
    handlers
  );
  // A browser callback can already be queued when we remove its listener.
  const queued = new Map(oldSocket.callbacks);
  current = newSocket;
  const disposeNew = listenToWebSocket(
    newSocket.asWebSocket(),
    () => current === newSocket,
    handlers
  );
  newSocket.dispatchEvent(new Event('open'));
  assert.equal(connected, true);
  for (const [type, callback] of queued) callback(new Event(type));
  assert.equal(connected, true);
  assert.equal(messages, 0);
  assert.equal(errors, 0);
  disposeOld();
  // An unmount must invalidate callbacks even without changing the current socket.
  const queuedNew = new Map(newSocket.callbacks);
  disposeNew();
  for (const [type, callback] of queuedNew) callback(new Event(type));
  assert.equal(connected, true);
  assert.equal(messages, 0);
  assert.equal(errors, 0);
});

test('current socket events work and disposing removes the exact registered listeners', () => {
  const socket = new FakeSocket();
  const received: string[] = [];
  const handle = (event: Event) => received.push(event.type);
  const dispose = listenToWebSocket(socket.asWebSocket(), () => true, {
    open: handle,
    close: handle,
    error: handle,
    message: handle,
  });
  for (const type of ['open', 'message', 'error', 'close'])
    socket.dispatchEvent(new Event(type));
  assert.deepEqual(received, ['open', 'message', 'error', 'close']);
  dispose();
  for (const type of ['open', 'message', 'error', 'close'])
    socket.dispatchEvent(new Event(type));
  assert.equal(received.length, 4);
});

test('JSON log transport preserves Error messages, stacks and nested causes', () => {
  const cause = new Error('connection refused');
  const error = new Error('backend unavailable', { cause });
  const logged = JSON.parse(JSON.stringify(serializeLogArgs(['failed', error])));
  assert.equal(logged[1].name, 'Error');
  assert.equal(logged[1].message, error.message);
  assert.equal(logged[1].stack, error.stack);
  assert.equal(logged[1].cause.message, cause.message);
  assert.equal(logged[1].cause.stack, cause.stack);
});

test('logging handles circular error causes and bigint metadata', () => {
  const error = new Error('circular');
  error.cause = error;
  const logged = serializeLogArgs([error, { count: 42n }]) as [
    { cause: string },
    { count: string },
  ];
  assert.equal(logged[0].cause, '[Circular]');
  assert.equal(logged[1].count, '42');
});
