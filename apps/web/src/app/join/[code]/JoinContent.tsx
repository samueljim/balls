"use client";

import { useParams, useRouter } from "next/navigation";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { FunPageLayout, FunPanel, funInputClass, funButtonPrimaryClass } from "@/components/layout/FunPageLayout";
import { API_BASE, apiJson } from "@/lib/api";
import { pickRandomFunnyName } from "@/lib/funnyNames";

function useSearchDict(searchString: string) {
  const params = new URLSearchParams(searchString || (typeof window !== "undefined" ? window.location.search : ""));
  return { get: (k: string) => params.get(k) };
}

export default function JoinContent({
  overrideCode,
  overrideSearch,
}: { overrideCode?: string; overrideSearch?: string } = {}) {
  const params = useParams();
  const router = useRouter();
  const code = (overrideCode ?? (params.code as string))?.toUpperCase() ?? "";
  const [name, setName] = useState("");
  const [joining, setJoining] = useState(false);
  const [error, setError] = useState("");

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setJoining(true);
    setError("");
    try {
      const playerName = name.trim() || pickRandomFunnyName();
      const res = await fetch(`${API_BASE}/lobby/join`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code, playerName }),
      });
      const data = await apiJson<{
        error?: string;
        lobbyId?: string;
        playerId?: string;
        playerName?: string;
        gameId?: string;
        playerOrder?: { playerId: string; isBot: boolean; name: string }[];
      }>(res);
      if (data.error) throw new Error(data.error);
      if (data.gameId && data.playerOrder?.length) {
        try {
          sessionStorage.setItem(`balls:${data.gameId}`, JSON.stringify(data.playerOrder));
        } catch (_) {}
        router.push(
          `/game/${data.gameId}?playerId=${data.playerId}&playerName=${encodeURIComponent(data.playerName ?? "")}`
        );
      } else {
        router.push(
          `/lobby/${data.lobbyId}?playerId=${data.playerId}&playerName=${encodeURIComponent(data.playerName ?? "")}`
        );
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : "Join failed");
      setJoining(false);
    }
  }

  return (
    <FunPageLayout title="JOIN GAME" tagline={`CODE: ${code}`}>
      <FunPanel>
        <form onSubmit={handleSubmit} className="space-y-5">
          <div>
            <label className="block font-display text-sm text-emerald-600/90 tracking-wider mb-2">
              YOUR NAME <span className="text-stone-500 font-normal">(optional)</span>
            </label>
            <Input
              placeholder="Leave blank for a funny name"
              value={name}
              onChange={(e) => setName(e.target.value)}
              maxLength={32}
              autoFocus
              className={funInputClass}
            />
          </div>
          {error && (
            <div className="rounded-xl bg-amber-950/80 border border-amber-600/50 px-4 py-3 text-amber-200 text-sm">
              {error}
            </div>
          )}
          <Button type="submit" className={funButtonPrimaryClass} disabled={joining}>
            {joining ? "JOINING…" : "JOIN & PLAY"}
          </Button>
        </form>
      </FunPanel>
      <p className="mt-8 text-sm text-stone-500">No sign-up. Name optional — you can change it in the lobby.</p>
    </FunPageLayout>
  );
}
