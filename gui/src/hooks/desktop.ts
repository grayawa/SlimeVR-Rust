import { createContext, useContext, useEffect, useState } from 'react';
import { desktopHost } from '@/platform';
import type { DesktopHost, OSStats } from '@/platform/types';

type DesktopContext =
  | (DesktopHost & { isDesktop: true; data(): { os: OSStats } })
  | { isDesktop: false };

export const DesktopContextC = createContext<DesktopContext | undefined>(undefined);

export function useDesktopProvider(): DesktopContext {
  const [os, setOS] = useState<OSStats>({ type: 'unknown' });

  useEffect(() => {
    let active = true;
    desktopHost?.api
      .osStats()
      .then((value) => {
        if (active) setOS(value);
      })
      .catch(console.error);
    return () => {
      active = false;
    };
  }, []);

  return desktopHost
    ? { ...desktopHost, isDesktop: true, data: () => ({ os }) }
    : { isDesktop: false };
}

export function useDesktop() {
  const context = useContext(DesktopContextC);
  if (!context) throw new Error('useDesktop must be within a DesktopContext Provider');
  return context;
}
