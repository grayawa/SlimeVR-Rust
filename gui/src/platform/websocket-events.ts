/** Keep listener ownership tied to a socket, including callbacks already queued by the browser. */
export function listenToWebSocket(
  socket: WebSocket,
  isCurrent: () => boolean,
  handlers: {
    open: (event: Event) => void;
    close: (event: CloseEvent) => void;
    error: (event: Event) => void;
    message: (event: MessageEvent<ArrayBuffer | Blob | string>) => void;
  }
): () => void {
  let disposed = false;
  const open = (event: Event) => {
    if (!disposed && isCurrent()) handlers.open(event);
  };
  const close = (event: CloseEvent) => {
    if (!disposed && isCurrent()) handlers.close(event);
  };
  const error = (event: Event) => {
    if (!disposed && isCurrent()) handlers.error(event);
  };
  const message = (event: MessageEvent<ArrayBuffer | Blob | string>) => {
    if (!disposed && isCurrent()) handlers.message(event);
  };
  socket.addEventListener('open', open);
  socket.addEventListener('close', close);
  socket.addEventListener('error', error);
  socket.addEventListener('message', message);
  return () => {
    disposed = true;
    socket.removeEventListener('open', open);
    socket.removeEventListener('close', close);
    socket.removeEventListener('error', error);
    socket.removeEventListener('message', message);
  };
}
