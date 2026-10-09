import resolveConfig from 'tailwindcss/resolveConfig';
import { useMediaQuery } from 'react-responsive';
import tailwindConfig from '../../tailwind.config';

const fullConfig = resolveConfig(tailwindConfig as any);

type BreakpointKey = keyof typeof tailwindConfig.theme.screens;

export function useBreakpoint<K extends BreakpointKey>(breakpointKey: K) {
  // FIXME: Initial media-query resolution can cause layout flicker.
  // A shared AppProvider value could centralize breakpoint initialization.
  const bool = useMediaQuery({
    query: fullConfig.theme.screens[breakpointKey].raw
      ? fullConfig.theme.screens[breakpointKey].raw
      : `(min-width: ${fullConfig.theme.screens[breakpointKey]})`,
  });
  const capitalizedKey =
    breakpointKey.toString()[0].toUpperCase() + breakpointKey.toString().substring(1);
  type Key = `is${Capitalize<K>}`;
  return {
    [`is${capitalizedKey}`]: bool,
  } as Record<Key, boolean>;
}
