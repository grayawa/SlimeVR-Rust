import { useEffect, useMemo } from 'react';
import { useConfig } from './config';
import { useLocalization } from '@fluent/react';
import { connectedIMUTrackersAtom } from '@/store/app-store';
import { useAtomValue } from 'jotai';
import { useDesktop } from './desktop';

export function useDiscordPresence() {
  const desktop = useDesktop();

  const { config } = useConfig();
  const { l10n } = useLocalization();
  const imuTrackers = useAtomValue(connectedIMUTrackersAtom);
  const imuTrackersCount = useMemo(() => imuTrackers.length, [imuTrackers.length]);

  useEffect(() => {
    if (!desktop.isDesktop || !desktop.capabilities.discordPresence) return;
    if (config?.discordPresence === false) {
      desktop.api.setPresence?.({ enable: false });
      return;
    }

    desktop.api.setPresence?.({
      enable: true,
      activity: l10n.getString('settings-general-interface-discord_presence-message', {
        amount: imuTrackersCount,
      }),
      iconText: (__VERSION_TAG__ || __COMMIT_HASH__) + (__GIT_CLEAN__ ? '' : '-dirty'),
    });
  }, [config?.discordPresence, imuTrackersCount]);
}
