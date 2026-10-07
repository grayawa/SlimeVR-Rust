import { useWebsocketAPI } from '@/hooks/websocket-api';
import { BaseModal } from './commons/BaseModal';
import { Button } from './commons/Button';
import { Typography } from './commons/Typography';
import { useLocalization } from '@fluent/react';

export function BackendError() {
  const { backendError, clearBackendError } = useWebsocketAPI();
  const { l10n } = useLocalization();
  return (
    <BaseModal
      isOpen={backendError !== null}
      onRequestClose={clearBackendError}
    >
      <div className="flex flex-col gap-3">
        <Typography variant="main-title">
          {l10n.getString('backend-operation-failed')}
        </Typography>
        <Typography>{backendError}</Typography>
        <Button variant="primary" onClick={clearBackendError}>
          {l10n.getString('backend-error-dismiss')}
        </Button>
      </div>
    </BaseModal>
  );
}
