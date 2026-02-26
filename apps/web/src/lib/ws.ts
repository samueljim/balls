"use client";

import { useCallback, useEffect, useRef, useState } from "react";

export function useWebSocket(url: string | null) {
  const [readyState, setReadyState] = useState<number>(WebSocket.CLOSED);
  const [lastMessage, setLastMessage] = useState<unknown>(null);
  const wsRef = useRef<WebSocket | null>(null);

  const send = useCallback((data: object | string) => {
    const ws = wsRef.current;
    if (ws?.readyState === WebSocket.OPEN) {
      ws.send(typeof data === "string" ? data : JSON.stringify(data));
    }
  }, []);

  useEffect(() => {
    if (!url) return;
    const ws = new WebSocket(url);
    wsRef.current = ws;
    ws.onopen = () => setReadyState(WebSocket.OPEN);
    ws.onclose = () => {
      setReadyState(WebSocket.CLOSED);
      wsRef.current = null;
    };
    ws.onmessage = (event) => {
      try {
        setLastMessage(JSON.parse(event.data));
      } catch {
        setLastMessage(event.data);
      }
    };
    return () => {
      ws.close();
      wsRef.current = null;
    };
  }, [url]);

  return { readyState, lastMessage, send };
}

const DEFAULT_WS_BASE = "https://api.worms.bne.sh";
export function getWsUrl(path: string): string {
  const base = process.env.NEXT_PUBLIC_WS_BASE ?? process.env.NEXT_PUBLIC_API_BASE ?? DEFAULT_WS_BASE;
  const host = base.replace(/^http/, "ws");
  return `${host}${path}`;
}
