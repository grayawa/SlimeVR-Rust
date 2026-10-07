import { createContext, useEffect, useState } from 'react';
import { HashRouter as Router, Outlet, Route, Routes } from 'react-router-dom';
import { Home } from './components/home/Home';
import { MainLayout } from './components/MainLayout';
import { AppContextProvider } from './components/providers/AppContext';
import { GeneralSettings } from './components/settings/pages/GeneralSettings';
import { Serial } from './components/settings/pages/Serial';
import { SettingsLayout } from './components/settings/SettingsLayout';
import {
  useProvideWebsocketApi,
  WebSocketApiContext,
} from './hooks/websocket-api';

import { OnboardingContextProvider } from './components/onboarding/OnboardingContextProvider';
import { OnboardingLayout } from './components/onboarding/OnboardingLayout';
import { AutomaticProportionsPage } from './components/onboarding/pages/body-proportions/AutomaticProportions';
import { ManualProportionsPage } from './components/onboarding/pages/body-proportions/ManualProportions';
import { ConnectTrackersPage } from './components/onboarding/pages/ConnectTracker';
import { HomePage } from './components/onboarding/pages/Home';
import { ErrorCollectingConsentPage } from './components/onboarding/pages/ErrorCollectingConsent';
import { AutomaticMountingPage } from './components/onboarding/pages/mounting/AutomaticMounting';
import { ManualMountingPage } from './components/onboarding/pages/mounting/ManualMounting';
import { TrackersAssignPage } from './components/onboarding/pages/trackers-assign/TrackerAssignment';
import { WifiCredsPage } from './components/onboarding/pages/WifiCreds';
import { DonglePage } from './components/onboarding/pages/Dongle';
import { ConfigContextProvider } from './components/providers/ConfigContext';
import { SerialDetectionModal } from './components/SerialDetectionModal';
import { VRCOSCSettings } from './components/settings/pages/VRCOSCSettings';
import { TopBar } from './components/TopBar';
import { TrackerSettingsPage } from './components/tracker/TrackerSettings';
import { OSCRouterSettings } from './components/settings/pages/OSCRouterSettings';
import { VMCSettings } from './components/settings/pages/VMCSettings';
import { MountingChoose } from './components/onboarding/pages/mounting/MountingChoose';
import { VersionUpdateModal } from './components/VersionUpdateModal';
import semver from 'semver';
import { useBreakpoint } from './hooks/breakpoint';
import { VRModePage } from './components/vr-mode/VRModePage';
import { InterfaceSettings } from './components/settings/pages/InterfaceSettings';
import { error, log } from './utils/logging';
import { FirmwareToolSettings } from './components/firmware-tool/FirmwareTool';
import { AppLayout } from './AppLayout';
import { Preload } from './components/Preload';
import { BackendError } from './components/BackendError';
import { BackendNotice } from './components/BackendNotice';
import { BVHSaved } from './components/BVHSaved';
import { UnknownDeviceModal } from './components/UnknownDeviceModal';
import { useDiscordPresence } from './hooks/discord-presence';
import { withSentryReactRouterV6Routing } from '@sentry/react';
import { ScaledProportionsPage } from './components/onboarding/pages/body-proportions/ScaledProportions';
import { AdvancedSettings } from './components/settings/pages/AdvancedSettings';
import { FirmwareUpdate } from './components/firmware-update/FirmwareUpdate';
import { ConnectionLost } from './components/onboarding/pages/ConnectionLost';
import { VRCWarningsPage } from './components/vrc/VRCWarningsPage';
import { StayAlignedSetup } from './components/onboarding/pages/stay-aligned/StayAlignedSetup';
import { TrackingChecklistProvider } from './components/tracking-checklist/TrackingChecklistProvider';
import { HomeScreenSettings } from './components/settings/pages/HomeScreenSettings';
import { ChecklistPage } from './components/tracking-checklist/TrackingChecklist';
import { QuizSlimeSetQuestion } from './components/onboarding/pages/quiz/SlimeSetQuestion';
import { QuizUsageQuestion } from './components/onboarding/pages/quiz/UsageQuestion';
import { QuizRuntimeQuestion } from './components/onboarding/pages/quiz/RuntimeQuestion';
import { QuizMocapPosQuestion } from './components/onboarding/pages/quiz/MocapPreferencesQuestions';
import { DesktopContextC, useDesktopProvider } from './hooks/desktop';
import { AppLocalizationProvider } from './i18n/config';
import { openUrl } from './hooks/crossplatform';
import { desktopHost } from './platform';
import { UdevRulesModal } from './components/onboarding/UdevRulesModal';

export const GH_REPO =
  import.meta.env.VITE_RELEASE_REPOSITORY || 'SlimeVR/SlimeVR-Server';
export const VersionContext = createContext('');
export const DOCS_SITE = 'https://docs.slimevr.dev';
export const SLIMEVR_DISCORD = 'https://discord.gg/slimevr';

const SentryRoutes = withSentryReactRouterV6Routing(Routes);

function Layout() {
  const { isMobile } = useBreakpoint('mobile');
  useDiscordPresence();

  return (
    <>
      <SerialDetectionModal />
      <VersionUpdateModal />
      <UnknownDeviceModal />
      <BackendError />
      <BackendNotice />
      <BVHSaved />
      <UdevRulesModal />
      <SentryRoutes>
        <Route element={<AppLayout />}>
          <Route
            path="/"
            element={
              <MainLayout isMobile={isMobile} full>
                <Home />
              </MainLayout>
            }
          />
          <Route
            path="/firmware-update"
            element={
              <MainLayout isMobile={isMobile}>
                <FirmwareUpdate />
              </MainLayout>
            }
          />
          <Route
            path="/vr-mode"
            element={
              <MainLayout isMobile={isMobile} full>
                <VRModePage />
              </MainLayout>
            }
          />
          <Route
            path="/checklist"
            element={
              <MainLayout isMobile={isMobile}>
                <ChecklistPage />
              </MainLayout>
            }
          />
          <Route
            path="/tracker/:trackernum/:deviceid"
            element={
              <MainLayout background={false} isMobile={isMobile}>
                <TrackerSettingsPage />
              </MainLayout>
            }
          />
          <Route
            path="/vrc-warnings"
            element={
              <MainLayout isMobile={isMobile}>
                <VRCWarningsPage />
              </MainLayout>
            }
          />
          <Route
            path="/settings"
            element={
              <SettingsLayout>
                <Outlet />
              </SettingsLayout>
            }
          >
            <Route path="firmware-tool" element={<FirmwareToolSettings />} />
            <Route path="trackers" element={<GeneralSettings />} />
            <Route path="serial" element={<Serial />} />
            <Route path="osc/router" element={<OSCRouterSettings />} />
            <Route path="osc/vrchat" element={<VRCOSCSettings />} />
            <Route path="osc/vmc" element={<VMCSettings />} />
            <Route path="interface" element={<InterfaceSettings />} />
            <Route path="interface/home" element={<HomeScreenSettings />} />
            <Route path="advanced" element={<AdvancedSettings />} />
          </Route>
          <Route
            path="/onboarding"
            element={
              <OnboardingLayout>
                <Outlet />
              </OnboardingLayout>
            }
          >
            <Route path="home" element={<HomePage />} />
            <Route
              path="error-collecting-consent"
              element={<ErrorCollectingConsentPage />}
            />
            <Route path="wifi-creds" element={<WifiCredsPage />} />
            <Route path="quiz/slime-set" element={<QuizSlimeSetQuestion />} />
            <Route path="quiz/usage" element={<QuizUsageQuestion />} />
            <Route path="quiz/runtime" element={<QuizRuntimeQuestion />} />
            <Route path="quiz/mocap-pos" element={<QuizMocapPosQuestion />} />
            <Route path="dongle" element={<DonglePage />} />
            <Route path="firmware-tool" element={<FirmwareToolSettings />} />
            <Route path="connect-trackers" element={<ConnectTrackersPage />} />
            <Route path="trackers-assign" element={<TrackersAssignPage />} />
            <Route path="mounting/choose" element={<MountingChoose />} />
            <Route path="mounting/auto" element={<AutomaticMountingPage />} />
            <Route path="mounting/manual" element={<ManualMountingPage />} />
            <Route
              path="body-proportions/auto"
              element={<AutomaticProportionsPage />}
            />
            <Route
              path="body-proportions/manual"
              element={<ManualProportionsPage />}
            />
            <Route
              path="body-proportions/scaled"
              element={<ScaledProportionsPage />}
            />
            <Route path="stay-aligned" element={<StayAlignedSetup />} />
          </Route>
          <Route path="*" element={<TopBar />} />
        </Route>
      </SentryRoutes>
    </>
  );
}

export default function App() {
  const websocketAPI = useProvideWebsocketApi();
  const [updateFound, setUpdateFound] = useState('');
  const desktop = useDesktopProvider();

  useEffect(() => {
    const onKeydown: (arg0: KeyboardEvent) => void = function (event) {
      // prevent search bar keybind
      if (
        event.key === 'F3' ||
        (event.ctrlKey && event.key === 'f') ||
        (event.metaKey && event.key === 'f')
      ) {
        event.preventDefault();
      }
    };

    window.addEventListener('keydown', onKeydown);
    return () => window.removeEventListener('keydown', onKeydown);
  }, []);

  useEffect(() => {
    // don't show update stuff when on android
    if (window.__ANDROID__?.isThere()) return;
    if (
      desktopHost?.kind === 'tauri' &&
      !import.meta.env.VITE_RELEASE_REPOSITORY
    )
      return;

    if (!semver.valid(__VERSION_TAG__)) {
      log(
        { version: __VERSION_TAG__ || 'development' },
        'Non semver version, skipping the server update check'
      );
      return;
    }

    async function fetchReleases() {
      const releases = await fetch(
        `https://api.github.com/repos/${GH_REPO}/releases`
      )
        .then((res) => res.json())
        .catch(() => null)
        .then((json: any[]) =>
          Array.isArray(json)
            ? json.filter((rl) => rl?.prerelease === false && !rl?.draft)
            : null
        );

      if (!releases?.length) return;

      if (typeof releases[0].tag_name !== 'string') return;

      const version = semver.coerce(releases[0].tag_name);

      if (version && semver.gt(version, __VERSION_TAG__)) {
        setUpdateFound(releases[0].tag_name);
      }
    }
    fetchReleases().catch((e) => error(e, 'failed to fetch releases'));
  }, []);

  useEffect(() => {
    if (!desktop.isDesktop) return;
    const unlisten = desktop.api.onServerStatus(({ type, message, level }) => {
      if (type === 'stdout' || type === 'stderr') {
        // Native host already persisted this entry; mirror its severity in DevTools.
        const severity = level ?? (type === 'stderr' ? 'warn' : 'info');
        const output = severity === 'trace' ? 'debug' : severity;
        console[output]('[SERVER] %s', message);
      } else if (type === 'error') {
        if (desktopHost?.kind === 'tauri') console.error('Error: %s', message);
        else error('Error: %s', message);
      } else if (type === 'terminated') {
        if (desktopHost?.kind === 'tauri')
          console.error('Server Process Terminated: %s', message);
        else error('Server Process Terminated: %s', message);
      } else if (type === 'other') {
        if (desktopHost?.kind === 'tauri')
          console.info('Other process event: %s', message);
        else log('Other process event: %s', message);
      }
    });

    return () => {
      unlisten();
    };
  }, []);

  useEffect(() => {
    function onKeyboard(ev: KeyboardEvent) {
      if (ev.key === 'F1') {
        return openUrl(DOCS_SITE);
      }
    }

    document.addEventListener('keyup', onKeyboard);
    return () => document.removeEventListener('keyup', onKeyboard);
  }, []);

  return (
    <DesktopContextC.Provider value={desktop}>
      <AppLocalizationProvider>
        <Router>
          <ConfigContextProvider>
            <WebSocketApiContext.Provider value={websocketAPI}>
              <AppContextProvider>
                <OnboardingContextProvider>
                  <TrackingChecklistProvider>
                    <VersionContext.Provider value={updateFound}>
                      <div className="h-full w-full text-standard bg-background-80 text-background-10">
                        <Preload />
                        {!websocketAPI.isConnected && <ConnectionLost />}
                        {websocketAPI.isConnected && <Layout />}
                      </div>
                    </VersionContext.Provider>
                  </TrackingChecklistProvider>
                </OnboardingContextProvider>
              </AppContextProvider>
            </WebSocketApiContext.Provider>
          </ConfigContextProvider>
        </Router>
      </AppLocalizationProvider>
    </DesktopContextC.Provider>
  );
}
