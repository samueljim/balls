"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { API_BASE } from "@/lib/api";
import { getWsUrl, useWebSocket } from "@/lib/ws";

interface GameCanvasProps {
  gameId: string;
  playerId: string;
  playerOrder: { playerId: string; isBot: boolean; name: string }[];
}

export function GameCanvas({ gameId, playerId, playerOrder }: GameCanvasProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [inputLog, setInputLog] = useState<string[]>([]);
  const [currentTurnIndex, setCurrentTurnIndex] = useState(0);
  const [inited, setInited] = useState(false);
  const gameRef = useRef<{ apply_input: (s: string) => void; get_state_json: () => string; get_terrain_buffer: () => Uint8Array; terrain_width: () => number; terrain_height: () => number; tick: () => void; init_round: (seed: number, tid: number, positions: number[]) => void } | null>(null);

  const wsPath = `/game/${gameId}?playerId=${encodeURIComponent(playerId)}`;
  const { readyState, lastMessage, send } = useWebSocket(getWsUrl(wsPath));

  const seedRef = useRef(0);
  if (seedRef.current === 0) {
    let h = 0;
    for (let i = 0; i < gameId.length; i++) h = (h << 5) - h + gameId.charCodeAt(i);
    seedRef.current = Math.abs(h >>> 0);
  }
  const seed = seedRef.current;

  const initSent = useRef(false);
  useEffect(() => {
    if (readyState !== WebSocket.OPEN || initSent.current || playerOrder.length === 0 || !seed) return;
    initSent.current = true;
    fetch(`${API_BASE}/game/${gameId}/init`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        playerOrder,
        rngSeed: seed,
        terrainId: 0,
      }),
    })
      .then((res) => res.ok && setInited(true))
      .catch(console.error);
  }, [readyState, gameId, playerOrder, seed]);

  useEffect(() => {
    if (!lastMessage || typeof lastMessage !== "object") return;
    const m = lastMessage as { type: string; input?: string; turnIndex?: number; state?: { inputLog?: string[]; currentTurnIndex?: number } };
    if (m.type === "input" && m.input) {
      setInputLog((prev) => [...prev, m.input!]);
      gameRef.current?.apply_input(m.input);
    }
    if (m.type === "turn_advanced" && m.turnIndex !== undefined) setCurrentTurnIndex(m.turnIndex);
    if (m.type === "state" && m.state) {
      if (m.state.inputLog) {
        setInputLog(m.state.inputLog);
        const g = gameRef.current;
        if (g) {
          m.state.inputLog.forEach((input) => g.apply_input(input));
        }
      }
      if (m.state.currentTurnIndex !== undefined) setCurrentTurnIndex(m.state.currentTurnIndex);
    }
  }, [lastMessage]);

  const sendInput = useCallback(
    (input: string) => {
      send({ type: "input", input });
    },
    [send]
  );

  const pendingEndTurnRef = useRef(false);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const wasmPath = "/wasm/game_core.js";
        const wasm = await import(/* webpackIgnore: true */ wasmPath as string);
        await wasm.default();
        const { Game } = wasm;
        const g = new Game();
        const positions: number[] = [];
        const w = 800;
        const h = 400;
        for (let i = 0; i < playerOrder.length; i++) {
          positions.push(Math.floor((w * (i + 1)) / (playerOrder.length + 1)), h - 80);
        }
        g.init_round(seed, 0, positions);
        if (cancelled) return;
        gameRef.current = g;
      } catch (e) {
        console.error("WASM load failed, using placeholder:", e);
        gameRef.current = null;
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [playerOrder.length]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas || !gameRef.current) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const w = gameRef.current.terrain_width();
    const h = gameRef.current.terrain_height();
    canvas.width = w;
    canvas.height = h;

    const draw = () => {
      const g = gameRef.current;
      if (!g) return;
      const buf = g.get_terrain_buffer();
      const stateJson = g.get_state_json();
      let state: { worms?: { x: number; y: number; health: number; facing: number }[] } = {};
      try {
        state = JSON.parse(stateJson);
      } catch {}
      ctx.fillStyle = "#1a472a";
      ctx.fillRect(0, 0, w, h);
      if (buf.length === w * h) {
        const imageData = ctx.createImageData(w, h);
        for (let i = 0; i < buf.length; i++) {
          const v = buf[i];
          imageData.data[i * 4] = v ? 139 : 34;
          imageData.data[i * 4 + 1] = v ? 90 : 139;
          imageData.data[i * 4 + 2] = v ? 43 : 69;
          imageData.data[i * 4 + 3] = 255;
        }
        ctx.putImageData(imageData, 0, 0);
      }
      (state.worms ?? []).forEach((worm, i) => {
        ctx.fillStyle = i === currentTurnIndex ? "#fbbf24" : "#22c55e";
        ctx.beginPath();
        ctx.arc(worm.x, worm.y, 10, 0, Math.PI * 2);
        ctx.fill();
        ctx.fillStyle = "#fff";
        ctx.font = "10px sans-serif";
        ctx.fillText(String(worm.health), worm.x - 4, worm.y - 12);
      });
    };

    const interval = setInterval(() => {
      if (gameRef.current) {
        gameRef.current.tick();
        const stateJson = gameRef.current.get_state_json();
        try {
          const s = JSON.parse(stateJson) as { phase?: string };
          if (s.phase === "TurnEnd" && pendingEndTurnRef.current) {
            pendingEndTurnRef.current = false;
            send({ type: "end_turn" });
          }
        } catch (_) {}
        draw();
      }
    }, 50);
    draw();
    return () => clearInterval(interval);
  }, [currentTurnIndex, inputLog, send]);

  const myTurnIndex = playerOrder.findIndex((p) => p.playerId === playerId);
  const isMyTurn = myTurnIndex === currentTurnIndex && !playerOrder[currentTurnIndex]?.isBot;
  const [aimAngle, setAimAngle] = useState(45);
  const [power, setPower] = useState(70);
  const [weapon, setWeapon] = useState<"Bazooka" | "Grenade">("Bazooka");

  const handleFire = () => {
    pendingEndTurnRef.current = true;
    sendInput(
      JSON.stringify({
        Fire: { weapon, angle_deg: aimAngle, power_percent: power },
      })
    );
  };

  return (
    <div className="min-h-screen flex flex-col bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
      <div className="p-3 flex flex-wrap items-center justify-between gap-3 border-b-2 border-emerald-800/60 bg-stone-900/80">
        <span className="font-display text-sm text-emerald-400/90 tracking-wider">
          TURN: {playerOrder[currentTurnIndex]?.name ?? "—"}
        </span>
        {isMyTurn && (
          <div className="flex flex-wrap items-center gap-3">
            <div className="flex items-center gap-2">
              <span className="font-display text-xs text-emerald-600/90">WEAPON</span>
              <select
                className="rounded-lg border-2 border-stone-600 bg-stone-950 px-2 py-1.5 text-sm text-stone-100 font-display"
                value={weapon}
                onChange={(e) => setWeapon(e.target.value as "Bazooka" | "Grenade")}
              >
                <option value="Bazooka">Bazooka</option>
                <option value="Grenade">Grenade</option>
              </select>
            </div>
            <div className="flex items-center gap-2">
              <span className="font-display text-xs text-emerald-600/90">ANGLE</span>
              <input
                type="range"
                min="0"
                max="180"
                value={aimAngle}
                onChange={(e) => setAimAngle(Number(e.target.value))}
                className="w-24 accent-emerald-500"
              />
              <span className="font-display text-xs w-8 text-stone-300">{aimAngle}°</span>
            </div>
            <div className="flex items-center gap-2">
              <span className="font-display text-xs text-emerald-600/90">POWER</span>
              <input
                type="range"
                min="10"
                max="100"
                value={power}
                onChange={(e) => setPower(Number(e.target.value))}
                className="w-24 accent-emerald-500"
              />
              <span className="font-display text-xs w-8 text-stone-300">{power}%</span>
            </div>
            <button
              className="font-display px-4 py-2 rounded-xl bg-gradient-to-b from-emerald-500 to-emerald-700 hover:from-emerald-400 hover:to-emerald-600 text-emerald-950 border-2 border-emerald-400/50 text-sm shadow-lg hover:scale-105 active:scale-95 transition-transform"
              onClick={handleFire}
            >
              FIRE
            </button>
            <button
              className="font-display px-4 py-2 rounded-xl border-2 border-stone-500 bg-stone-800 text-stone-100 text-sm hover:bg-stone-700 transition-colors"
              onClick={() => send({ type: "end_turn" })}
            >
              END TURN
            </button>
          </div>
        )}
      </div>
      <canvas
        ref={canvasRef}
        className="flex-1 w-full"
        style={{ imageRendering: "pixelated" }}
      />
    </div>
  );
}
