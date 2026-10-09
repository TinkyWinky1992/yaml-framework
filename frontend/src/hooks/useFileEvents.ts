import { useEffect, useRef, useState } from 'react';
import { wsUrl } from '../api';
import type { FileEvent } from '../types';

const RECONNECT_MS = 3000;

/** Subscribes to the backend's `/ws` file-watcher stream, reconnecting automatically. */
export function useFileEvents(onEvent: (event: FileEvent) => void): boolean {
  const [connected, setConnected] = useState(false);
  const handler = useRef(onEvent);

  useEffect(() => {
    handler.current = onEvent;
  }, [onEvent]);

  useEffect(() => {
    let socket: WebSocket | null = null;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let disposed = false;

    const connect = () => {
      socket = new WebSocket(wsUrl('/ws'));
      socket.onopen = () => setConnected(true);
      socket.onmessage = (msg) => {
        try {
          handler.current(JSON.parse(msg.data) as FileEvent);
        } catch {
          // ignore malformed frames
        }
      };
      socket.onclose = () => {
        setConnected(false);
        if (!disposed) timer = setTimeout(connect, RECONNECT_MS);
      };
    };

    connect();
    return () => {
      disposed = true;
      clearTimeout(timer);
      socket?.close();
    };
  }, []);

  return connected;
}
