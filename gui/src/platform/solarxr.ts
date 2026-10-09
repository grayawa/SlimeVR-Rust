import { Builder, ByteBuffer } from 'flatbuffers';
import {
  MessageBundle,
  MessageBundleT,
  Message as PubSubMessage,
  MessageT as PubSubMessageT,
  Payload,
  PubSubHeaderT,
} from 'solarxr-protocol';

export type DriverNotice = 'existing_manual_driver';

export interface BackendInfo {
  backend: 'rust';
  capabilities: string[];
  persistent: boolean;
  driverError: string | null;
  driverNotice: DriverNotice | null;
}
export interface SavedBVH {
  path: string;
  frames: number;
}

export function decodeBackendNotice(
  text: string
):
  | { type: 'backend_info'; info: BackendInfo }
  | { type: 'backend_error'; message: string }
  | { type: 'backend_file_saved'; kind: 'bvh' | 'autobone'; file: SavedBVH }
  | null {
  try {
    const data: unknown = JSON.parse(text);
    if (!data || typeof data !== 'object') return null;
    const record = data as Record<string, unknown>;
    if (record.type === 'backend_error' && typeof record.message === 'string') {
      return { type: 'backend_error', message: record.message };
    }
    if (
      record.type === 'backend_file_saved' &&
      typeof record.path === 'string' &&
      Number.isSafeInteger(record.frames) &&
      (record.frames as number) >= 0
    ) {
      return {
        type: 'backend_file_saved',
        kind: record.kind === 'autobone' ? 'autobone' : 'bvh',
        file: { path: record.path, frames: record.frames as number },
      };
    }
    if (
      record.type === 'backend_info' &&
      record.backend === 'rust' &&
      Array.isArray(record.capabilities) &&
      record.capabilities.every((v) => typeof v === 'string')
    ) {
      return {
        type: 'backend_info',
        info: {
          backend: 'rust',
          capabilities: record.capabilities,
          persistent: record.persistent === true,
          driverError:
            typeof record.driver_error === 'string' ? record.driver_error : null,
          driverNotice:
            record.driver_notice === 'existing_manual_driver'
              ? record.driver_notice
              : null,
        },
      };
    }
  } catch {
    /* Recognized backend notifications are handled above; other text is ignored. */
  }
  return null;
}

export function decodeSolarXR(buffer: ArrayBuffer) {
  if (buffer.byteLength < 8 || buffer.byteLength > 8 * 1024 * 1024) {
    throw new Error('Invalid SolarXR frame size');
  }
  return MessageBundle.getRootAsMessageBundle(
    new ByteBuffer(new Uint8Array(buffer))
  ).unpack();
}

/** Cache read subscriptions for restoration after reconnect. */
export class ReadRequestCache {
  private requests = new Map<string, Uint8Array>();
  remember(key: string, frame: Uint8Array) {
    if (!this.requests.has(key) && this.requests.size >= 64) {
      this.requests.delete(this.requests.keys().next().value!);
    }
    this.requests.set(key, frame.slice());
  }
  frames() {
    return [...this.requests.values()];
  }
}

// Generated optional-scalar setters use zero defaults; keep false distinct from absent.
export function encodeSolarXR(message: MessageBundleT): Uint8Array {
  const builder = new ScalarBuilder(1024);
  // Absent NONE union tags and absent payloads must agree for the Rust verifier.
  const encoded = Object.assign(new MessageBundleT(), message);
  encoded.pubSubMsgs = message.pubSubMsgs.map((header) => {
    if (!(header.u instanceof PubSubMessageT) || header.u.payloadType !== Payload.NONE)
      return header;
    const value = Object.assign(new PubSubMessageT(), header.u);
    value.pack = (b) => {
      const topic = b.createObjectOffset(value.topic);
      PubSubMessage.startMessage(b);
      PubSubMessage.addTopicType(b, value.topicType);
      PubSubMessage.addTopic(b, topic);
      return PubSubMessage.endMessage(b);
    };
    return new PubSubHeaderT(header.uType, value);
  });
  builder.finish(encoded.pack(builder));
  return builder.asUint8Array();
}

// Force scalar defaults only. Forcing zero table offsets with this FlatBuffers
// version produces invalid pointers for absent message unions.
class ScalarBuilder extends Builder {
  override addFieldInt8(field: number, value: number): void {
    super.addFieldInt8(field, value, NaN);
  }
  override addFieldInt16(field: number, value: number): void {
    super.addFieldInt16(field, value, NaN);
  }
  override addFieldInt32(field: number, value: number): void {
    super.addFieldInt32(field, value, NaN);
  }
  override addFieldInt64(field: number, value: bigint): void {
    super.addFieldInt64(field, value, value + 1n);
  }
  override addFieldFloat32(field: number, value: number): void {
    super.addFieldFloat32(field, value, NaN);
  }
  override addFieldFloat64(field: number, value: number): void {
    super.addFieldFloat64(field, value, NaN);
  }
}
