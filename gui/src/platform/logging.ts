import { serializeLogArgs } from './logging-values';

export const logLevels = ['error', 'warn', 'info', 'debug', 'trace'] as const;
export type LogLevel = (typeof logLevels)[number];
export function isLogLevel(value: unknown): value is LogLevel {
  return typeof value === 'string' && logLevels.includes(value as LogLevel);
}

/** Filter before serialization/IPC so disabled detail has no desktop overhead. */
export function createLogger(
  persist: (level: LogLevel, args: unknown[]) => void,
  consoleOutput: Pick<Console, 'error' | 'warn' | 'info' | 'debug'> = console
) {
  let threshold: LogLevel = 'info';
  return {
    setLevel(level: LogLevel) {
      threshold = level;
    },
    write(level: LogLevel, args: unknown[]) {
      if (logLevels.indexOf(level) > logLevels.indexOf(threshold)) return;
      consoleOutput[level === 'trace' ? 'debug' : level](...args);
      persist(level, serializeLogArgs(args));
    },
  };
}
