"use client";

import { useParams, useSearchParams } from "next/navigation";
import { Suspense, useEffect, useState } from "react";
import Link from "next/link";
import { GameCanvas } from "@/components/game/GameCanvas";

function GameInner() {
  const params = useParams();
  const search = useSearchParams();
  const gameId = params.id as string;
  const playerId = search.get("playerId") ?? "";
  const [playerOrder, setPlayerOrder] = useState<{ playerId: string; isBot: boolean; name: string }[]>([]);

  useEffect(() => {
    try {
      const stored = sessionStorage.getItem(`worms:${gameId}`);
      if (stored) setPlayerOrder(JSON.parse(stored));
    } catch (_) {}
  }, [gameId]);

  if (playerOrder.length === 0) {
    return (
      <main className="min-h-screen relative flex flex-col items-center justify-center overflow-hidden bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
        <div className="absolute inset-0 overflow-hidden pointer-events-none">
          <div className="absolute -bottom-20 left-1/4 w-72 h-72 rounded-full bg-[#1a2e1a]/40 blur-3xl" />
          <div className="absolute bottom-0 right-1/3 w-96 h-48 rounded-full bg-[#2d1a0a]/30 blur-3xl" />
        </div>
        <div className="relative z-10 text-center">
          <h1
            className="font-display text-4xl sm:text-5xl mb-4"
            style={{ color: "#22c55e", textShadow: "3px 3px 0 #166534" }}
          >
            LOADING GAME…
          </h1>
          <p className="text-stone-500 text-sm">
            <Link href="/" className="text-emerald-500 hover:text-emerald-400 underline">
              Back home
            </Link>
          </p>
        </div>
      </main>
    );
  }

  return (
    <GameCanvas
      gameId={gameId}
      playerId={playerId}
      playerOrder={playerOrder}
    />
  );
}

export default function GameContent() {
  return (
    <Suspense
      fallback={
        <main className="min-h-screen flex items-center justify-center bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
          <h1 className="font-display text-3xl text-emerald-500">LOADING…</h1>
        </main>
      }
    >
      <GameInner />
    </Suspense>
  );
}
