import { Localized, useLocalization } from '@fluent/react';
import { useEffect, useState } from 'react';
import {
  RecordBVHRequestT,
  RecordBVHStatusRequestT,
  RecordBVHStatusT,
  RpcMessage,
} from 'solarxr-protocol';
import { useWebsocketAPI } from '@/hooks/websocket-api';
import { BigButton } from './commons/BigButton';
import { RecordIcon } from './commons/icon/RecordIcon';
import classNames from 'classnames';
import { useConfig } from '@/hooks/config';
import { useDesktop } from '@/hooks/desktop';

export function BVHButton(props: React.HTMLAttributes<HTMLButtonElement>) {
  const desktop = useDesktop();
  const { config } = useConfig();
  const { useRPCPacket, sendRPCPacket, backendInfo } = useWebsocketAPI();
  const [recording, setRecording] = useState(false);
  const [saving, setSaving] = useState(false);
  const { l10n } = useLocalization();

  useEffect(() => {
    sendRPCPacket(
      RpcMessage.RecordBVHStatusRequest,
      new RecordBVHStatusRequestT()
    );
  }, []);

  const toggleBVH = async () => {
    const record = new RecordBVHRequestT(recording);

    if (desktop.isDesktop && !recording) {
      if (config?.bvhDirectory) {
        record.path = config.bvhDirectory;
      } else {
        setSaving(true);
        const save = await desktop.api.saveDialog({
          title: l10n.getString('bvh-save_title'),
          filters: [
            {
              name: 'BVH',
              extensions: ['bvh'],
            },
          ],
          defaultPath: 'bvh-recording.bvh',
        });
        record.path = save.filePath ?? null;
        setSaving(false);
        if (save.canceled) return;
      }
    }

    sendRPCPacket(RpcMessage.RecordBVHRequest, record);
  };

  useRPCPacket(RpcMessage.RecordBVHStatus, (data: RecordBVHStatusT) => {
    setRecording(data.recording);
  });

  return (
    <Localized id={recording ? 'bvh-recording' : 'bvh-start_recording'}>
      <BigButton
        icon={<RecordIcon width={20} />}
        onClick={toggleBVH}
        disabled={
          saving ||
          (backendInfo?.backend === 'rust' &&
            !backendInfo.capabilities.includes('bvh'))
        }
        className={classNames(
          props.className,
          'border',
          recording ? 'border-status-critical' : 'border-transparent'
        )}
      />
    </Localized>
  );
}
