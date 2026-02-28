"use client";

import { useParams } from "next/navigation";
import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { useToast } from "@/components/Toast";

const API_BASE =
  typeof window !== "undefined"
    ? process.env.NEXT_PUBLIC_WS_BASE ?? process.env.NEXT_PUBLIC_API_BASE ?? "https://api.balls.bne.sh"
    : process.env.NEXT_PUBLIC_API_BASE ?? "https://api.balls.bne.sh";

export default function GameView({ overrideId }: { overrideId?: string } = {}) {
  const params = useParams();
  const gameId = (overrideId ?? params?.id) as string | undefined;
  const mounted = useRef(false);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const { addToast } = useToast();

  // Listen for game events emitted by the WASM engine via js_game_event → CustomEvent
  useEffect(() => {
    function handleGameEvent(e: Event) {
      const ev = (e as CustomEvent<{ type: string; name?: string; damage?: number; hp?: number; winner?: string; ball?: string }>).detail;
      switch (ev.type) {
        case "hit":
          if (ev.name && ev.damage != null && ev.hp != null) {
            addToast(`${ev.name} took ${ev.damage} damage (${ev.hp} HP left)`, "info");
          }
          break;
        case "died":
          if (ev.name) {
            addToast(`${ev.name} has been eliminated!`, "error");
          }
          break;
        case "turn_start":
          if (ev.name) {
            const label = ev.ball && ev.ball !== ev.name ? `${ev.name} (${ev.ball})` : ev.name;
            addToast(`${label}'s turn`, "info");
          }
          break;
        case "game_over":
          if (ev.winner) {
            addToast(`${ev.winner} wins!`, "success");
          }
          break;
      }
    }
    window.addEventListener("game_event", handleGameEvent);
    return () => window.removeEventListener("game_event", handleGameEvent);
  }, [addToast]);

  // Lock body scroll for the duration of the game page
  useEffect(() => {
    const prev = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    document.documentElement.style.overflow = "hidden";
    return () => {
      document.body.style.overflow = prev;
      document.documentElement.style.overflow = "";
    };
  }, []);

  useEffect(() => {
    if (!gameId || mounted.current) return;
    mounted.current = true;
    setLoadError(null);

    (window as unknown as { __BALLS_WS_BASE?: string }).__BALLS_WS_BASE = API_BASE;

    const base = (typeof process !== "undefined" && process.env?.NEXT_PUBLIC_BASE_PATH) || "";
    const isLocalhost =
      typeof window !== "undefined" &&
      (window.location.hostname === "localhost" || window.location.hostname === "127.0.0.1");

    const wasmUrl = isLocalhost
      ? `${window.location.origin}/api/wasm`
      : `${base}/wasm/game_core.wasm`;

    const loadScript = (src: string): Promise<void> =>
      new Promise((resolve, reject) => {
        const s = document.createElement("script");
        s.src = src.startsWith("http") ? src : base + src;
        s.onload = () => resolve();
        s.onerror = () => reject(new Error("Failed to load " + src));
        document.body.appendChild(s);
      });

    // Prefer explicit API host when provided (e.g. production `NEXT_PUBLIC_API_BASE`).
    const explicitApiBase = typeof window !== "undefined" ? process.env.NEXT_PUBLIC_API_BASE : undefined;
    const apiGlJs = explicitApiBase ? `${explicitApiBase.replace(/\/$/, "")}/api/gl-js` : null;
    // Static loader lives on the web origin under /js/gl.js; prefer that.
    const originGlJs = `${window.location.origin}/js/gl.js`;

    const loadScriptWithFallback = (urls: (string | null)[]) =>
      new Promise<void>((resolve, reject) => {
        const tried: string[] = [];
        const tryNext = (i: number) => {
          if (i >= urls.length) return reject(new Error("All script load attempts failed: " + tried.join(", ")));
          const src = urls[i];
          if (!src) return tryNext(i + 1);
          tried.push(src);
          const s = document.createElement("script");
          s.src = src.startsWith("http") ? src : base + src;
          s.onload = () => resolve();
          s.onerror = () => {
            s.remove();
            tryNext(i + 1);
          };
          document.body.appendChild(s);
        };
        tryNext(0);
      });

    // Try origin static file first, then fall back to API host if needed.
    loadScriptWithFallback([originGlJs, apiGlJs])
      .then(() => loadScript(base + "/js/ws_plugin.js"))
      .then(() => loadScript(base + "/js/mobile_controls.js"))
      .then(() => {
        const load = (window as unknown as { load?: (url: string) => void }).load;
        if (typeof load === "function") {
          load(wasmUrl);
          // Focus canvas so keyboard input works (macroquad/miniquad expects focused canvas)
          setTimeout(() => canvasRef.current?.focus(), 500);
        } else {
          setLoadError("Game loader not found.");
        }
      })
      .catch((e) => {
        const msg = e instanceof Error ? e.message : String(e);
        setLoadError(msg);
        console.error("[game] Load error:", e);
      });
  }, [gameId]);

  if (!gameId) {
    return (
      <main className="min-h-screen flex items-center justify-center bg-[#0d1f0d]">
        <p className="text-stone-400">Missing game id</p>
        <Link href="/" className="ml-4 text-emerald-500 underline">
          Home
        </Link>
      </main>
    );
  }

  if (loadError) {
    return (
      <main className="min-h-screen flex flex-col items-center justify-center bg-[#0d1f0d] gap-4">
        <p className="text-amber-400">Failed to load game</p>
        <p className="text-stone-500 text-sm max-w-md text-center">{loadError}</p>
      </main>
    );
  }

  return (
    <>
      <canvas
        ref={canvasRef}
        id="glcanvas"
        tabIndex={1}
        style={{
          display: "block",
          position: "fixed",
          top: 0,
          left: 0,
          width: "100%",
          height: "100%",
          outline: "none",
        }}
      />
    </>
  );
}
