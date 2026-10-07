import { createContext, useContext, useEffect, useRef, useState } from 'react';

import {
  DataFeedMessage,
  DataFeedMessageHeaderT,
  MessageBundleT,
  PubSubHeaderT,
  PubSubUnion,
  RpcMessage,
  RpcMessageHeaderT,
  TransactionIdT,
} from 'solarxr-protocol';

import {
  BackendInfo,
  DriverNotice,
  decodeBackendNotice,
  decodeSolarXR,
  encodeSolarXR,
  ReadRequestCache,
  SavedBVH,
} from '@/platform/solarxr';
import { useInterval, useTimeout } from './timeout';
import { log, warn, error } from '@/utils/logging';
import { listenToWebSocket } from '@/platform/websocket-events';

export interface WebSocketApi {
  isConnected: boolean;
  isFirstConnection: boolean;
  timedOut: boolean;
  reconnect: () => void;
  backendInfo: BackendInfo | null;
  backendError: string | null;
  clearBackendError: () => void;
  backendNotice: DriverNotice | null;
  clearBackendNotice: () => void;
  savedBVH: SavedBVH | null;
  clearSavedBVH: () => void;
  useRPCPacket: <T>(type: RpcMessage, callback: (packet: T) => void) => void;
  useDataFeedPacket: <T>(type: DataFeedMessage, callback: (packet: T) => void) => void;
  sendRPCPacket: (
    type: RpcMessage,
    data: RPCPacketType,
    options?: { ignoreIfDisconnected?: boolean }
  ) => void;
  sendDataFeedPacket: (type: DataFeedMessage, data: DataFeedPacketType) => void;
  usePubSubPacket: <T>(type: PubSubUnion, callback: (packet: T) => void) => void;
  sendPubSubPacket: (type: PubSubUnion, data: PubSubPacketType) => void;
}

export const WebSocketApiContext = createContext<WebSocketApi>(undefined as never);

export type RPCPacketType = RpcMessageHeaderT['message'];
export type PubSubPacketType = PubSubHeaderT['u'];
export type DataFeedPacketType = DataFeedMessageHeaderT['message'];
// export type OutboundPacketType = OutboundPacketT['packet'];

export function useProvideWebsocketApi(): WebSocketApi {
  const rpcPacketCounterRef = useRef<number>(0);
  const readRequests = useRef(new ReadRequestCache());
  const [backendInfo, setBackendInfo] = useState<BackendInfo | null>(null);
  const [backendError, setBackendError] = useState<string | null>(null);
  const [backendNotice, setBackendNotice] = useState<DriverNotice | null>(null);
  const seenBackendNotices = useRef(new Set<DriverNotice>());
  const [savedBVH, setSavedBVH] = useState<SavedBVH | null>(null);
  const webSocketRef = useRef<WebSocket | null>(null);
  const socketCleanupRef = useRef<(() => void) | null>(null);
  const rpclistenerRef = useRef<EventTarget>(new EventTarget());
  const pubsublistenerRef = useRef<EventTarget>(new EventTarget());
  const datafeedlistenerRef = useRef<EventTarget>(new EventTarget());
  const [isFirstConnection, setFirstConnection] = useState(true);
  const [timedOut, setTimedOut] = useState(false);
  const [isConnected, setConnected] = useState(false);

  const urlParams = new URLSearchParams(window.location.search);
  const targetIp = urlParams.get('ip') ?? '127.0.0.1';
  const targetPort = urlParams.get('port') ?? '21110';

  useInterval(() => {
    if (webSocketRef.current && !isConnected) {
      log('Attempting to reconnect');
      reconnect();
    }
  }, 3000);

  const onConnected = () => {
    if (!webSocketRef.current) return;
    log('WebSocket connected', { url: webSocketRef.current.url });
    setFirstConnection(false);
    setTimedOut(false);
    setConnected(true);
    for (const frame of readRequests.current.frames()) webSocketRef.current.send(frame);
  };

  const onConnectionClose = (event: CloseEvent) => {
    const reportClose = event.code === 1000 ? log : warn;
    reportClose('WebSocket closed', {
      code: event.code,
      reason: event.reason,
      wasClean: event.wasClean,
      url: webSocketRef.current?.url,
    });
    setConnected(false);
    setBackendInfo(null);
  };

  const onMessage = async (event: MessageEvent<ArrayBuffer | Blob | string>) => {
    const connection = webSocketRef.current;
    if (event.target !== connection) return;
    if (typeof event.data === 'string') {
      const notice = decodeBackendNotice(event.data);
      if (notice?.type === 'backend_info') {
        setBackendInfo(notice.info);
        if (notice.info.driverError) setBackendError(notice.info.driverError);
        const driverNotice = notice.info.driverNotice;
        if (driverNotice && !seenBackendNotices.current.has(driverNotice)) {
          seenBackendNotices.current.add(driverNotice);
          setBackendNotice(driverNotice);
        }
      }
      if (notice?.type === 'backend_error') setBackendError(notice.message);
      if (notice?.type === 'backend_file_saved' && notice.kind === 'bvh')
        setSavedBVH(notice.file);
      return;
    }
    try {
      const buffer =
        event.data instanceof Blob ? await event.data.arrayBuffer() : event.data;
      if (connection !== webSocketRef.current) return;
      const message = decodeSolarXR(buffer);
      message.rpcMsgs.forEach((header) => {
        rpclistenerRef.current.dispatchEvent(
          new CustomEvent(RpcMessage[header.messageType], { detail: header.message })
        );
      });
      message.dataFeedMsgs.forEach((header) => {
        datafeedlistenerRef.current.dispatchEvent(
          new CustomEvent(DataFeedMessage[header.messageType], {
            detail: header.message,
          })
        );
      });
      message.pubSubMsgs.forEach((header) => {
        pubsublistenerRef.current.dispatchEvent(
          new CustomEvent(PubSubUnion[header.uType], { detail: header.u })
        );
      });
    } catch (cause) {
      error('Unable to decode server message', cause);
      setBackendError('Unable to read the server response. Please reconnect.');
    }
  };

  const send = (
    frame: Uint8Array,
    readKey?: string,
    ignoreIfDisconnected = false
  ): void => {
    if (readKey) readRequests.current.remember(readKey, frame);
    if (webSocketRef.current?.readyState === WebSocket.OPEN) {
      webSocketRef.current.send(frame);
    } else if (!readKey && !ignoreIfDisconnected) {
      setBackendError(
        'The server is disconnected. Please reconnect before making changes.'
      );
    }
  };

  const sendRPCPacket = (
    type: RpcMessage,
    data: RPCPacketType,
    options?: { ignoreIfDisconnected?: boolean }
  ): void => {
    const message = new MessageBundleT();

    const rpcHeader = new RpcMessageHeaderT();
    rpcHeader.messageType = type;
    rpcHeader.txId = new TransactionIdT(rpcPacketCounterRef.current >>> 0);
    rpcHeader.message = data;

    message.rpcMsgs = [rpcHeader];

    const readOnly = [
      RpcMessage.SettingsRequest,
      RpcMessage.SkeletonConfigRequest,
      RpcMessage.TrackingPauseStateRequest,
      RpcMessage.TrackingChecklistRequest,
      RpcMessage.ServerInfosRequest,
      RpcMessage.InstalledInfoRequest,
      RpcMessage.VRCConfigStateRequest,
      RpcMessage.HeartbeatRequest,
      RpcMessage.RecordBVHStatusRequest,
      RpcMessage.OverlayDisplayModeRequest,
      RpcMessage.SerialDevicesRequest,
      RpcMessage.KeybindRequest,
      RpcMessage.MagToggleRequest,
    ].includes(type);
    send(
      encodeSolarXR(message),
      readOnly ? `rpc:${type}:${JSON.stringify(data)}` : undefined,
      options?.ignoreIfDisconnected
    );
    rpcPacketCounterRef.current = (rpcPacketCounterRef.current + 1) >>> 0;
  };

  const sendDataFeedPacket = (
    type: DataFeedMessage,
    data: DataFeedPacketType
  ): void => {
    const message = new MessageBundleT();

    const datafeedHeader = new DataFeedMessageHeaderT();
    datafeedHeader.messageType = type;
    datafeedHeader.message = data;

    message.dataFeedMsgs = [datafeedHeader];

    send(
      encodeSolarXR(message),
      type === DataFeedMessage.StartDataFeed ? 'datafeed' : undefined
    );
  };

  const sendPubSubPacket = (type: PubSubUnion, data: PubSubPacketType): void => {
    const message = new MessageBundleT();

    const pubSubHeader = new PubSubHeaderT();
    pubSubHeader.uType = type;
    pubSubHeader.u = data;

    message.pubSubMsgs = [pubSubHeader];

    send(
      encodeSolarXR(message),
      type === PubSubUnion.SubscriptionRequest
        ? `pubsub:${JSON.stringify(data)}`
        : undefined
    );
  };

  const connect = () => {
    const socket = new WebSocket(`ws://${targetIp}:${targetPort}`);
    socket.binaryType = 'arraybuffer';
    webSocketRef.current = socket;
    socketCleanupRef.current = listenToWebSocket(
      socket,
      () => webSocketRef.current === socket,
      {
        open: onConnected,
        close: onConnectionClose,
        message: onMessage,
        error: () =>
          warn('WebSocket error', { url: socket.url, readyState: socket.readyState }),
      }
    );
  };

  const disconnect = () => {
    const socket = webSocketRef.current;
    webSocketRef.current = null;
    socketCleanupRef.current?.();
    socketCleanupRef.current = null;
    socket?.close();
    setConnected(false);
    setBackendInfo(null);
  };

  const reconnect = () => {
    disconnect();
    connect();
  };

  useTimeout(() => {
    if (!isConnected && isFirstConnection) {
      setTimedOut(true);
    }
  }, 10_000); // Show the user that the server timed out if no connection after 10s

  useEffect(() => {
    connect();
    return () => {
      disconnect();
    };
  }, []);

  return {
    isConnected,
    backendInfo,
    backendError,
    clearBackendError: () => setBackendError(null),
    backendNotice,
    clearBackendNotice: () => setBackendNotice(null),
    savedBVH,
    clearSavedBVH: () => setSavedBVH(null),
    isFirstConnection,
    timedOut,
    reconnect,
    useDataFeedPacket: <T>(type: DataFeedMessage, callback: (packet: T) => void) => {
      useEffect(() => {
        const onEvent = (event: CustomEventInit) => {
          callback(event.detail);
        };
        datafeedlistenerRef.current.addEventListener(DataFeedMessage[type], onEvent);
        return () => {
          datafeedlistenerRef.current.removeEventListener(
            DataFeedMessage[type],
            onEvent
          );
        };
      }, [callback, type]);
    },
    useRPCPacket: <T>(type: RpcMessage, callback: (packet: T) => void) => {
      useEffect(() => {
        const onEvent = (event: CustomEventInit) => {
          callback(event.detail);
        };
        rpclistenerRef.current.addEventListener(RpcMessage[type], onEvent);
        return () => {
          rpclistenerRef.current.removeEventListener(RpcMessage[type], onEvent);
        };
      }, [callback, type]);
    },
    usePubSubPacket: <T>(type: PubSubUnion, callback: (packet: T) => void) => {
      useEffect(() => {
        const onEvent = (event: CustomEventInit) => {
          callback(event.detail);
        };
        pubsublistenerRef.current.addEventListener(PubSubUnion[type], onEvent);
        return () => {
          pubsublistenerRef.current.removeEventListener(PubSubUnion[type], onEvent);
        };
      }, [callback, type]);
    },
    sendRPCPacket,
    sendDataFeedPacket,
    sendPubSubPacket,
  };
}

export function useWebsocketAPI(): WebSocketApi {
  const context = useContext<WebSocketApi>(WebSocketApiContext);
  if (!context) {
    throw new Error('useWebsocketAPI must be within a WebSocketApi Provider');
  }
  return context;
}
