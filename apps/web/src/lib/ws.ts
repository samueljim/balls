"use client";

import { useCallback, useEffect, useRef, useState } from "react";

const RECONNECT_DELAY_MS = 2000;
const RECONNECT_MAX_DELAY_MS = 30000;

export function useWebSocket(url: string | null, options?: { reconnect?: boolean }) {
  const reconnect = options?.reconnect !== false;
  const [readyState, setReadyState] = useState<number>(WebSocket.CLOSED);
  const [lastMessage, setLastMessage] = useState<unknown>(null);
  /** Increments on every message so we always re-run effects that depend on it (no missed updates). */
  const [messageVersion, setMessageVersion] = useState(0);
  const [reconnectAttempt, setReconnectAttempt] = useState(0);
  const wsRef = useRef<WebSocket | null>(null);
  const wasOpenRef = useRef(false);
  const reconnectTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** Every message received (so consumers can process all, not just latest). */
  const messageQueueRef = useRef<unknown[]>([]);
  /** Queue for messages sent while connection is not open. */
  const sendQueueRef = useRef<string[]>([]);

  const send = useCallback((data: object | string) => {
    const ws = wsRef.current;
    const message = typeof data === "string" ? data : JSON.stringify(data);
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(message);
    } else {
      // Queue message to be sent when connection is established
      sendQueueRef.current.push(message);
    }
  }, []);

  useEffect(() => {
    if (!url) return;
    const ws = new WebSocket(url);
    wsRef.current = ws;

    ws.onopen = () => {
      wasOpenRef.current = true;
      setReadyState(WebSocket.OPEN);
      // Send any queued messages that were attempted while connection was down
      const queuedMessages = sendQueueRef.current;
      sendQueueRef.current = [];
      for (const msg of queuedMessages) {
        try {
          ws.send(msg);
        } catch (e) {
          console.error("Failed to send queued message:", e);
        }
      }
    };

    ws.onclose = () => {
      setReadyState(WebSocket.CLOSED);
      wsRef.current = null;
      if (reconnect && wasOpenRef.current) {
        wasOpenRef.current = false;
        if (reconnectTimeoutRef.current) clearTimeout(reconnectTimeoutRef.current);
        const delay = Math.min(
          RECONNECT_DELAY_MS * Math.pow(2, Math.min(reconnectAttempt, 4)),
          RECONNECT_MAX_DELAY_MS
        );
        reconnectTimeoutRef.current = setTimeout(() => {
          reconnectTimeoutRef.current = null;
          setReconnectAttempt((a) => a + 1);
        }, delay);
      }
    };

    ws.onmessage = (event) => {
      try {
        const parsed = JSON.parse(event.data);
        messageQueueRef.current.push(parsed);
        setLastMessage(parsed);
        setMessageVersion((v) => v + 1);
      } catch {
        const data = event.data;
        messageQueueRef.current.push(data);
        setLastMessage(data);
        setMessageVersion((v) => v + 1);
      }
    };

    return () => {
      if (reconnectTimeoutRef.current) {
        clearTimeout(reconnectTimeoutRef.current);
        reconnectTimeoutRef.current = null;
      }
      messageQueueRef.current = [];
      sendQueueRef.current = [];
      ws.close();
      wsRef.current = null;
    };
  }, [url, reconnectAttempt, reconnect]);

  return { readyState, lastMessage, messageVersion, messageQueueRef, send };
}

const DEFAULT_WS_BASE = "https://api.balls.bne.sh";
export function getWsUrl(path: string): string {
  const base = process.env.NEXT_PUBLIC_WS_BASE ?? process.env.NEXT_PUBLIC_API_BASE ?? DEFAULT_WS_BASE;
  const host = base.replace(/^http/, "ws");
  return `${host}${path}`;
}
