import { useLocalization } from '@fluent/react';
import { useEffect, useState } from 'react';
import {
  RecordBVHRequestT,
  RecordBVHStatusRequestT,
  RecordBVHStatusT,
  RpcMessage,
} from 'solarxr-protocol';
import { useWebsocketAPI } from './websocket-api';
import { useConfig } from './config';
import { useDesktop } from './desktop';

export function useBHV() {
  const desktop = useDesktop();
  const { config } = useConfig();
  const { useRPCPacket, sendRPCPacket, backendInfo } = useWebsocketAPI();
  const [state, setState] = useState<'idle' | 'recording' | 'saving'>('idle');
  const { l10n } = useLocalization();

  useEffect(() => {
    sendRPCPacket(RpcMessage.RecordBVHStatusRequest, new RecordBVHStatusRequestT());
  }, []);

  const toggle = async () => {
    if (state === 'saving') return;
    const record = new RecordBVHRequestT(state === 'recording');

    if (desktop.isDesktop && state === 'idle') {
      if (config?.bvhDirectory) {
        record.path = config.bvhDirectory;
      } else {
        setState('saving');
        const open = await desktop.api.saveDialog({
          title: l10n.getString('bvh-save_title'),
          filters: [
            {
              name: 'BVH',
              extensions: ['bvh'],
            },
          ],
          defaultPath: 'bvh-recording.bvh',
        });
        record.path = open.filePath ?? null;
        setState('idle');
        if (open.canceled) return;
      }
    }

    sendRPCPacket(RpcMessage.RecordBVHRequest, record);
  };

  useRPCPacket(RpcMessage.RecordBVHStatus, (data: RecordBVHStatusT) => {
    setState(data.recording ? 'recording' : 'idle');
  });

  return {
    available:
      backendInfo?.backend !== 'rust' || backendInfo.capabilities.includes('bvh'),
    state,
    toggle,
  };
}
