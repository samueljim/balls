"use client";

import { useParams, useSearchParams } from "next/navigation";
import { useEffect, useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { FunPageLayout, FunPanel, funButtonPrimaryClass, funButtonSecondaryClass } from "@/components/layout/FunPageLayout";
import { getWsUrl, useWebSocket } from "@/lib/ws";

interface Player {
  id: string;
  name: string;
  ready?: boolean;
  isBot?: boolean;
}

function useSearchDict(searchString: string) {
  const params = new URLSearchParams(searchString || (typeof window !== "undefined" ? window.location.search : ""));
  return {
    get: (k: string) => params.get(k),
  };
}

export default function LobbyContent({
  overrideId,
  overrideSearch,
}: { overrideId?: string; overrideSearch?: string } = {}) {
  const params = useParams();
  const searchFromRouter = useSearchParams();
  const search = overrideSearch !== undefined ? useSearchDict(overrideSearch) : searchFromRouter;
  const lobbyId = (overrideId ?? (params.id as string)) ?? "";
  const isHost = search.get("host") === "1";
  const playerId = search.get("playerId") ?? lobbyId;
  const playerName = search.get("playerName") ?? "Host";
  const [players, setPlayers] = useState<Player[]>([]);
  const [copied, setCopied] = useState(false);
  const code = search.get("code") ?? null;

  if (!lobbyId) {
    return (
      <main className="min-h-screen flex items-center justify-center bg-[#0d1f0d]">
        <p className="text-stone-400">Missing lobby id</p>
      </main>
    );
  }

  const wsPath = `/lobby/${lobbyId}?playerId=${encodeURIComponent(playerId)}&playerName=${encodeURIComponent(playerName)}`;
  const { readyState, lastMessage, send } = useWebSocket(getWsUrl(wsPath));

  useEffect(() => {
    if (!lastMessage || typeof lastMessage !== "object") return;
    const m = lastMessage as { type: string; players?: Player[]; gameId?: string };
    if (m.type === "player_list" && m.players) setPlayers(m.players);
    if (m.type === "game_started" && m.gameId) {
      const order = (lastMessage as { playerOrder?: unknown }).playerOrder ?? [];
      try {
        sessionStorage.setItem(`worms:${m.gameId}`, JSON.stringify(order));
      } catch (_) {}
      window.location.href = `/game/${m.gameId}?playerId=${encodeURIComponent(playerId)}`;
    }
  }, [lastMessage, playerId]);

  const inviteLink = typeof window !== "undefined" && code ? `${window.location.origin}/join/${code}` : "";
  const copyLink = () => {
    if (inviteLink) {
      navigator.clipboard.writeText(inviteLink);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const startGame = () => send({ type: "start_game" });
  const addBot = () => send({ type: "add_bot" });
  const removeBot = (pid: string) => send({ type: "remove_bot", playerId: pid });

  return (
    <FunPageLayout title="LOBBY" tagline={code ? `CODE: ${code}` : undefined}>
      <FunPanel>
        <div className="space-y-5">
          {readyState !== WebSocket.OPEN && (
            <p className="font-display text-sm text-emerald-600/90 tracking-wider">Connecting…</p>
          )}
          <div>
            <p className="font-display text-sm text-emerald-600/90 tracking-wider mb-3">PLAYERS</p>
            <ul className="space-y-2">
              {players.map((p) => (
                <li
                  key={p.id}
                  className="flex items-center justify-between rounded-xl border-2 border-stone-600 bg-stone-950/80 px-4 py-3"
                >
                  <span className="text-stone-100 font-medium">
                    {p.name}
                    {p.isBot && (
                      <span className="ml-2 text-xs text-amber-400/90">(bot)</span>
                    )}
                  </span>
                  {isHost && p.isBot && (
                    <Button
                      variant="ghost"
                      onClick={() => removeBot(p.id)}
                      className="text-amber-400 hover:text-amber-300 hover:bg-amber-950/50 rounded-lg px-2 py-1 text-sm"
                    >
                      Remove
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          </div>
          {isHost && (
            <>
              <Button variant="outline" className={funButtonSecondaryClass} onClick={addBot}>
                ADD BOT
              </Button>
              {inviteLink && (
                <Button variant="secondary" className={funButtonSecondaryClass} onClick={copyLink}>
                  {copied ? "COPIED!" : "COPY INVITE LINK"}
                </Button>
              )}
              <Button className={funButtonPrimaryClass} onClick={startGame} disabled={players.length < 2}>
                START GAME
              </Button>
            </>
          )}
        </div>
      </FunPanel>
      <p className="mt-8 text-sm text-stone-500">
        <Link href="/" className="text-emerald-500 hover:text-emerald-400 underline">
          Back home
        </Link>
      </p>
    </FunPageLayout>
  );
}
