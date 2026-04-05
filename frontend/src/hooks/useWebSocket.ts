import { useEffect, useRef } from 'react';
import { useStore } from '../store';
import type { Event, Op, WsMessage } from '../types';

const WS_URL = 'ws://localhost:3001/ws';
const MAX_RECONNECT_DELAY = 30000;
const BASE_RECONNECT_DELAY = 1000;

export function useWebSocket() {
  const wsRef = useRef<WebSocket | null>(null);
  const reconnectAttempt = useRef(0);
  const reconnectTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const setConnected = useStore((s) => s.setConnected);
  const setSendOp = useStore((s) => s.setSendOp);
  const handleEvent = useStore((s) => s.handleEvent);

  useEffect(() => {
    function connect() {
      if (wsRef.current?.readyState === WebSocket.OPEN) return;

      const ws = new WebSocket(WS_URL);
      wsRef.current = ws;

      ws.onopen = () => {
        console.log('[WS] connected');
        setConnected(true);
        reconnectAttempt.current = 0;

        // Register sendOp
        const sendOp = (op: Op) => {
          if (ws.readyState === WebSocket.OPEN) {
            const envelope: WsMessage = { kind: 'op', payload: op };
            ws.send(JSON.stringify(envelope));
          } else {
            console.warn('[WS] cannot send, not connected');
          }
        };
        setSendOp(sendOp);
      };

      ws.onmessage = (e) => {
        try {
          const msg: WsMessage = JSON.parse(e.data);
          if (msg.kind === 'event') {
            handleEvent(msg.payload as Event);
          }
        } catch (err) {
          console.error('[WS] parse error:', err);
        }
      };

      ws.onclose = () => {
        console.log('[WS] disconnected');
        setConnected(false);
        setSendOp(null);
        scheduleReconnect();
      };

      ws.onerror = (err) => {
        console.error('[WS] error:', err);
        ws.close();
      };
    }

    function scheduleReconnect() {
      const delay = Math.min(
        BASE_RECONNECT_DELAY * Math.pow(2, reconnectAttempt.current),
        MAX_RECONNECT_DELAY
      );
      reconnectAttempt.current += 1;
      console.log(`[WS] reconnecting in ${delay}ms (attempt ${reconnectAttempt.current})`);
      reconnectTimer.current = setTimeout(connect, delay);
    }

    connect();

    return () => {
      if (reconnectTimer.current) clearTimeout(reconnectTimer.current);
      if (wsRef.current) {
        wsRef.current.onclose = null; // prevent reconnect on cleanup
        wsRef.current.close();
      }
    };
  }, [setConnected, setSendOp, handleEvent]);
}
