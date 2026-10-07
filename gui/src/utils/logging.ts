import { desktopHost } from '@/platform';
import { createLogger, isLogLevel } from '@/platform/logging';

const logger = createLogger((level, args) => desktopHost?.api.log(level, ...args));
if (desktopHost?.api.getLogLevel) {
  void desktopHost.api
    .getLogLevel()
    .then((level) => {
      if (isLogLevel(level)) logger.setLevel(level);
    })
    .catch((cause) =>
      logger.write('warn', ['Unable to read diagnostic log level', cause])
    );
}

export function log(...msgs: unknown[]) {
  logger.write('info', msgs);
}
export function debug(...msgs: unknown[]) {
  logger.write('debug', msgs);
}
export function trace(...msgs: unknown[]) {
  logger.write('trace', msgs);
}
export function error(...msgs: unknown[]) {
  logger.write('error', msgs);
}
export function warn(...msgs: unknown[]) {
  logger.write('warn', msgs);
}
