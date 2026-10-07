import { useWebsocketAPI } from '@/hooks/websocket-api';
import { BaseModal } from './commons/BaseModal';
import { Button } from './commons/Button';
import { Typography } from './commons/Typography';
import { useLocalization } from '@fluent/react';

export function BackendNotice() {
  const { backendNotice, backendError, clearBackendNotice } = useWebsocketAPI();
  const { l10n } = useLocalization();
  return (
    <BaseModal
      isOpen={backendNotice !== null && backendError === null}
      onRequestClose={clearBackendNotice}
    >
      <div className="flex flex-col gap-3">
        <Typography variant="main-title">
          {l10n.getString('steamvr-existing-driver-title')}
        </Typography>
        <Typography>
          {l10n.getString('steamvr-existing-driver-description')}
        </Typography>
        <Button variant="primary" onClick={clearBackendNotice}>
          {l10n.getString('backend-error-dismiss')}
        </Button>
      </div>
    </BaseModal>
  );
}
