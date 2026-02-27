"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Button } from "@/components/ui/button";

import { API_BASE, apiJson } from "@/lib/api";
import { pickRandomFunnyName } from "@/lib/funnyNames";

export default function HomePage() {
  const router = useRouter();
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleCreate() {
    setError(null);
    setCreating(true);
    try {
      const res = await fetch(`${API_BASE}/lobby/create`, { method: "POST" });
      const data = await apiJson<{
        error?: string;
        lobbyId?: string;
        code?: string;
      }>(res);
      if (data.error) throw new Error(data.error);
      const playerName = pickRandomFunnyName();
      router.push(
        `/lobby/${data.lobbyId}?host=1&code=${encodeURIComponent(data.code ?? "")}&playerName=${encodeURIComponent(playerName)}`,
      );
    } catch (e) {
      console.error(e);
      setCreating(false);
      if (e instanceof TypeError && e.message === "Failed to fetch") {
        setError("Can't reach the game server 😩🪱");
      } else {
        setError(e instanceof Error ? e.message : "Something went wrong");
      }
    }
  }

  return (
    <main className="min-h-screen relative flex flex-col items-center justify-center overflow-hidden bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
      {/* Background craters / blobs */}
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -bottom-20 left-1/4 w-72 h-72 rounded-full bg-[#1a2e1a]/40 blur-3xl" />
        <div className="absolute bottom-0 right-1/3 w-96 h-48 rounded-full bg-[#2d1a0a]/30 blur-3xl" />
        <div className="absolute top-1/3 right-0 w-64 h-64 rounded-full bg-[#1a3d1a]/20 blur-3xl" />
        <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[600px] h-32 bg-gradient-to-b from-[#22c55e]/10 to-transparent blur-2xl" />
      </div>

      <div className="relative z-10 w-full max-w-md px-4 flex flex-col items-center">
        {/* Title */}
        <h1
          className="font-display text-6xl sm:text-7xl md:text-8xl text-center mb-1 select-none animate-[float_3s_ease-in-out_infinite]"
          style={{
            color: "#22c55e",
            textShadow: [
              "3px 3px 0 #166534",
              "6px 6px 0 rgba(0,0,0,0.4)",
              "0 0 20px rgba(34, 197, 94, 0.4)",
            ].join(", "),
          }}
        >
          BALLS&nbsp;🔴
        </h1>
        <p className="font-display text-lg text-emerald-400/90 tracking-widest mb-10 -mt-1">
          BLOW STUFF UP WITH FRIENDS
        </p>

        {/* Menu panel */}
        <div
          className="w-full rounded-2xl border-4 border-emerald-800/80 bg-gradient-to-b from-stone-900/95 to-stone-950/98 p-8 shadow-2xl animate-[crater-shine_4s_ease-in-out_infinite]"
          style={{
            boxShadow:
              "inset 0 1px 0 rgba(255,255,255,0.06), 0 25px 50px -12px rgba(0,0,0,0.6)",
          }}
        >
          <div className="space-y-6">
            {error && (
              <div className="rounded-xl bg-amber-950/80 border border-amber-600/50 px-4 py-3 text-amber-200 text-sm">
                {error}
              </div>
            )}
            <Button
              className="w-full h-14 text-xl font-display rounded-xl bg-gradient-to-b from-emerald-500 to-emerald-700 hover:from-emerald-400 hover:to-emerald-600 text-emerald-950 border-2 border-emerald-400/50 shadow-lg hover:shadow-emerald-500/25 hover:scale-[1.02] active:scale-[0.98] transition-all duration-200"
              onClick={handleCreate}
              disabled={creating}
            >
              {creating ? "LAUNCHING…" : "CREATE GAME"}
            </Button>
          </div>
        </div>

        <p className="mt-8 text-sm text-stone-500">
          No sign-up. Share the lobby link; they enter their name and play.
        </p>
      </div>
    </main>
  );
}
