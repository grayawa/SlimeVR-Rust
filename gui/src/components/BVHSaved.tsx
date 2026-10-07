import { useWebsocketAPI } from '@/hooks/websocket-api';
import { BaseModal } from './commons/BaseModal';
import { Button } from './commons/Button';
import { Typography } from './commons/Typography';
import { useLocalization } from '@fluent/react';

export function BVHSaved() {
  const { savedBVH, clearSavedBVH } = useWebsocketAPI();
  const { l10n } = useLocalization();
  return (
    <BaseModal isOpen={savedBVH !== null} onRequestClose={clearSavedBVH}>
      <div className="flex flex-col gap-3">
        <Typography variant="main-title">
          {l10n.getString('bvh-saved-title')}
        </Typography>
        <Typography>
          {l10n.getString('bvh-saved-description', {
            frames: savedBVH?.frames ?? 0,
          })}
        </Typography>
        <Typography>
          <span className="break-all select-text">{savedBVH?.path}</span>
        </Typography>
        <Button variant="primary" onClick={clearSavedBVH}>
          {l10n.getString('backend-error-dismiss')}
        </Button>
      </div>
    </BaseModal>
  );
}
