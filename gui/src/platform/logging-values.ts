/** Error fields are non-enumerable and would otherwise be lost at the JSON IPC boundary. */
export function serializeLogArgs(args: unknown[]): unknown[] {
  const seen = new WeakSet<object>();
  return JSON.parse(
    JSON.stringify(args, (_key, value: unknown) => {
      if (typeof value === 'bigint') return value.toString();
      if (value && typeof value === 'object') {
        if (seen.has(value)) return '[Circular]';
        seen.add(value);
        if (value instanceof Error)
          return {
            name: value.name,
            message: value.message,
            stack: value.stack,
            cause: value.cause,
          };
      }
      return value;
    })
  );
}
