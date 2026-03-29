import { useEffect, useRef, useCallback } from 'react';
import { useBeaverStore } from '../store';
import type { Op, WsMessage } from '../types';

// In dev mode, Vite proxies /ws to localhost:3001. In prod, connect directly.
const WS_URL = import.meta.env.DEV
  ? `ws://${window.location.hostname}:3001/ws`
  : `ws://${window.location.host}/ws`;
const RECONNECT_BASE_MS = 1000;
const RECONNECT_MAX_MS = 30000;
const HEARTBEAT_MS = 30000;

export function useWebSocket() {
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectAttempt = useRef(0);
  const heartbeatTimer = useRef<ReturnType<typeof setInterval>>();

  const setConnected = useBeaverStore((s) => s.setConnected);
  const handleEvent = useBeaverStore((s) => s.handleEvent);
  const setSendOp = useBeaverStore((s) => s.setSendOp);

  const connect = useCallback(() => {
    const ws = new WebSocket(WS_URL);
    wsRef.current = ws;

    ws.onopen = () => {
      reconnectAttempt.current = 0;
      setConnected(true);

      heartbeatTimer.current = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send(JSON.stringify({ kind: 'ping' }));
        }
      }, HEARTBEAT_MS);
    };

    ws.onmessage = (evt) => {
      try {
        const msg: WsMessage = JSON.parse(evt.data);
        if (msg.kind === 'event') {
          handleEvent(msg.payload);
        }
      } catch {
        // Ignore malformed messages
      }
    };

    ws.onclose = () => {
      setConnected(false);
      clearInterval(heartbeatTimer.current);

      const delay = Math.min(
        RECONNECT_BASE_MS * Math.pow(2, reconnectAttempt.current),
        RECONNECT_MAX_MS,
      );
      reconnectAttempt.current += 1;
      setTimeout(connect, delay);
    };

    ws.onerror = () => {
      ws.close();
    };
  }, [setConnected, handleEvent]);

  useEffect(() => {
    connect();
    return () => {
      wsRef.current?.close();
      clearInterval(heartbeatTimer.current);
    };
  }, [connect]);

  const sendOp = useCallback((op: Op) => {
    const ws = wsRef.current;
    if (ws && ws.readyState === WebSocket.OPEN) {
      const msg: WsMessage = { kind: 'op', payload: op };
      ws.send(JSON.stringify(msg));
    }
  }, []);

  // Register sendOp in the store so any component can use it
  useEffect(() => {
    setSendOp(sendOp);
  }, [sendOp, setSendOp]);

  return { sendOp };
}
