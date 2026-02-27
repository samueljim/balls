"use client";

import { useParams, useSearchParams } from "next/navigation";
import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  FunPageLayout,
  FunPanel,
  funButtonPrimaryClass,
  funButtonSecondaryClass,
  funInputClass,
} from "@/components/layout/FunPageLayout";
import { getWsUrl, useWebSocket } from "@/lib/ws";
import { LobbyLoading } from "@/components/LobbyLoading";
import { useToast } from "@/components/Toast";

interface Player {
  id: string;
  name: string;
  ready?: boolean;
  isBot?: boolean;
}

function useSearchDict(searchString: string) {
  const params = new URLSearchParams(
    searchString ||
      (typeof window !== "undefined" ? window.location.search : ""),
  );
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
  const search =
    overrideSearch !== undefined
      ? useSearchDict(overrideSearch)
      : searchFromRouter;
  const lobbyId = overrideId ?? (params.id as string) ?? "";
  const isHost = search.get("host") === "1";
  const playerId = search.get("playerId") ?? lobbyId;
  const playerNameFromUrl = search.get("playerName") ?? "";
  const [players, setPlayers] = useState<Player[]>([]);
  const [copied, setCopied] = useState(false);
  const [myEditName, setMyEditName] = useState(playerNameFromUrl);
  const [isStarting, setIsStarting] = useState(false);
  const code = search.get("code") ?? null;
  const hasRedirectedRef = useRef(false);

  const myPlayer = players.find((p) => p.id === playerId);
  useEffect(() => {
    if (myPlayer?.name && !myEditName.trim()) setMyEditName(myPlayer.name);
  }, [myPlayer?.name]);

  if (!lobbyId) {
    return (
      <main className="min-h-screen flex items-center justify-center bg-[#0d1f0d]">
        <p className="text-stone-400">Missing lobby id</p>
      </main>
    );
  }

  const wsPath = `/lobby/${lobbyId}?playerId=${encodeURIComponent(playerId)}&playerName=${encodeURIComponent(playerNameFromUrl)}`;
  const { readyState, lastMessage, messageVersion, messageQueueRef, send } = useWebSocket(getWsUrl(wsPath));
  const { addToast } = useToast();
  const prevReadyStateRef = useRef<number>(WebSocket.CLOSED);
  const hadConnectionDropRef = useRef(false);
  const requestedSyncRef = useRef(false);
  /** Previous player list (id + name) to detect joins, leaves, renames. */
  const prevPlayersRef = useRef<{ id: string; name: string }[]>([]);

  useEffect(() => {
    if (
      prevReadyStateRef.current === WebSocket.OPEN &&
      readyState === WebSocket.CLOSED
    ) {
      hadConnectionDropRef.current = true;
      requestedSyncRef.current = false;
      addToast("Connection lost. Reconnecting…", "error");
    } else if (
      prevReadyStateRef.current !== WebSocket.OPEN &&
      readyState === WebSocket.OPEN
    ) {
      if (hadConnectionDropRef.current) {
        hadConnectionDropRef.current = false;
        addToast("Reconnected", "success");
      }
      if (!requestedSyncRef.current) {
        requestedSyncRef.current = true;
        send({ type: "get_player_list" });
      }
    }
    prevReadyStateRef.current = readyState;
  }, [readyState, addToast, send]);

  // When tab/window gains focus, request latest lobby state so we don't miss joins/leaves
  useEffect(() => {
    const onFocus = () => {
      if (readyState === WebSocket.OPEN) send({ type: "get_player_list" });
    };
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [readyState, send]);

  // Process every message from the queue so we never miss a join/leave/rename
  useEffect(() => {
    const queue = messageQueueRef.current;
    const hadMessages = queue.length > 0;
    messageQueueRef.current = [];

    let latestPlayerList: Player[] | null = null;
    let gameStarted: { gameId: string; playerOrder?: unknown; rngSeed?: number } | null = null;

    for (const msg of queue) {
      if (!msg || typeof msg !== "object") continue;
      const m = msg as {
        type: string;
        message?: string;
        players?: Player[];
        gameId?: string;
        playerOrder?: unknown;
        rngSeed?: number;
      };
      if (m.type === "error" && m.message) {
        setIsStarting(false);
        addToast(m.message, "error");
      } else if (m.type === "player_list" && m.players) {
        latestPlayerList = m.players;
      } else if (m.type === "game_started" && m.gameId) {
        gameStarted = { gameId: m.gameId, playerOrder: m.playerOrder, rngSeed: m.rngSeed };
      }
    }

    // Fallback: when queue was empty, use lastMessage so we still apply the latest broadcast
    if (!hadMessages && lastMessage && typeof lastMessage === "object") {
      const m = lastMessage as {
        type?: string;
        message?: string;
        players?: Player[];
        gameId?: string;
        playerOrder?: unknown;
        rngSeed?: number;
      };
      if (m.type === "player_list" && Array.isArray(m.players)) latestPlayerList = m.players;
      else if (m.type === "game_started" && m.gameId) gameStarted = { gameId: m.gameId, playerOrder: m.playerOrder, rngSeed: m.rngSeed };
      else if (m.type === "error" && m.message) {
        setIsStarting(false);
        addToast(m.message, "error");
      }
    }

    if (latestPlayerList) {
      const prev = prevPlayersRef.current;
      const newIds = new Set(latestPlayerList.map((p) => p.id));
      const prevById = new Map(prev.map((p) => [p.id, p]));
      if (prev.length > 0) {
        for (const p of latestPlayerList) {
          if (!prevById.has(p.id) && p.id !== playerId) {
            addToast(`${p.name} joined`, "success");
          } else if (p.id !== playerId) {
            const old = prevById.get(p.id);
            if (old && old.name !== p.name) {
              addToast(`${old.name} is now ${p.name}`, "success");
            }
          }
        }
        for (const p of prev) {
          if (!newIds.has(p.id) && p.id !== playerId) {
            addToast(`${p.name} left`, "success");
          }
        }
      }
      prevPlayersRef.current = latestPlayerList.map((p) => ({ id: p.id, name: p.name }));
      setPlayers(latestPlayerList);
      const me = latestPlayerList.find((p) => p.id === playerId);
      // Sync local name edit field with server's confirmed value
      if (me && myEditName && myEditName !== me.name) {
        setMyEditName(me.name);
      }
    }

    if (gameStarted && !hasRedirectedRef.current) {
      hasRedirectedRef.current = true;
      try {
        sessionStorage.setItem(`worms:${gameStarted.gameId}`, JSON.stringify({ 
          playerOrder: gameStarted.playerOrder ?? [],
          rngSeed: gameStarted.rngSeed,
        }));
      } catch (_) {}
      window.location.href = `/game/${gameStarted.gameId}?playerId=${encodeURIComponent(playerId)}`;
    }
  }, [messageVersion, lastMessage, playerId, addToast]);

  // Periodic sync so host and all players see joins/renames even if a broadcast was missed
  useEffect(() => {
    if (readyState !== WebSocket.OPEN) return;
    const interval = setInterval(() => {
      send({ type: "get_player_list" });
    }, 3000);
    return () => clearInterval(interval);
  }, [readyState, send]);

  // Timeout: if we asked to start but never got game_started, re-enable the button
  useEffect(() => {
    if (!isStarting) return;
    const t = setTimeout(() => {
      setIsStarting(false);
      addToast("Could not start game. Try again.", "error");
    }, 8000);
    return () => clearTimeout(t);
  }, [isStarting, addToast]);

  const copyLink = () => {
    if (!code) return;
    const url =
      typeof window !== "undefined"
        ? `${window.location.origin}/join/${code}`
        : "";
    if (url) {
      navigator.clipboard.writeText(url);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }
  };

  const startGame = () => {
    if (readyState !== WebSocket.OPEN) {
      addToast("Not connected. Wait for the connection.", "error");
      return;
    }
    if (players.length < 2) {
      addToast("Add at least one more player (or bot) to start.", "error");
      return;
    }
    setIsStarting(true);
    send({ type: "start_game" });
  };
  const addBot = () => {
    if (readyState !== WebSocket.OPEN) {
      addToast("Not connected. Wait for the connection.", "error");
      return;
    }
    send({ type: "add_bot" });
  };
  const removeBot = (pid: string) => {
    if (readyState !== WebSocket.OPEN) {
      addToast("Not connected. Wait for the connection.", "error");
      return;
    }
    send({ type: "remove_bot", playerId: pid });
  };
  const setMyName = (name: string) =>
    send({ type: "set_name", playerName: name.trim().slice(0, 32) });

  const isHostPlayer = (p: Player) => p.id === lobbyId;

  if (readyState !== WebSocket.OPEN) {
    return <LobbyLoading />;
  }

  return (
    <FunPageLayout
      title="LOBBY&nbsp;🪱"
      tagline={code ? `CODE: ${code}` : undefined}
    >
      <FunPanel>
        <div className="space-y-5">
          <div>
            <p className="font-display text-sm text-emerald-600/90 tracking-wider mb-3">
              PLAYERS
            </p>
            <ul className="space-y-2">
              {players.map((p) => (
                <li
                  key={p.id}
                  className="flex items-center justify-between gap-2 rounded-xl border-2 border-stone-600 bg-stone-950/80 px-4 py-3"
                >
                  <div className="flex items-center gap-2 min-w-0 flex-1">
                    {p.id === playerId ? (
                      <Input
                        value={myEditName ?? p.name ?? ""}
                        placeholder="Your name"
                        onChange={(e) =>
                          setMyEditName(e.target.value.slice(0, 32))
                        }
                        onBlur={() => {
                          const raw = (myEditName ?? p.name ?? "").trim();
                          const v = raw || p.name || "";
                          if (v) setMyName(v);
                        }}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") {
                            e.preventDefault();
                            const raw = (myEditName ?? p.name ?? "").trim();
                            const v = raw || p.name || "";
                            if (v) setMyName(v);
                            e.currentTarget.blur();
                          }
                        }}
                        className={`${funInputClass} py-2 h-auto text-stone-100 font-medium max-w-[180px]`}
                      />
                    ) : (
                      <span className="text-stone-100 font-medium truncate">
                        {p.name}
                      </span>
                    )}
                    {isHostPlayer(p) && (
                      <span className="flex-shrink-0 text-xs text-emerald-400/90">
                        (host)
                      </span>
                    )}
                    {p.isBot && (
                      <span className="flex-shrink-0 text-xs text-amber-400/90">
                        (bot)
                      </span>
                    )}
                  </div>
                  {isHost && p.isBot && (
                    <Button
                      variant="ghost"
                      onClick={() => removeBot(p.id)}
                      className="text-amber-400 hover:text-amber-300 hover:bg-amber-950/50 rounded-lg px-2 py-1 text-sm flex-shrink-0"
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
              <Button
                variant="outline"
                className={funButtonSecondaryClass}
                onClick={addBot}
              >
                ADD BOT
              </Button>
              <Button
                variant="secondary"
                className={funButtonSecondaryClass}
                onClick={copyLink}
                disabled={!code}
              >
                {copied ? "COPIED!" : "COPY INVITE LINK"}
              </Button>
              <Button
                className={funButtonPrimaryClass}
                onClick={startGame}
                disabled={players.length < 2 || isStarting}
              >
                {isStarting ? "Starting…" : "START GAME"}
              </Button>
            </>
          )}
        </div>
      </FunPanel>
      <p className="mt-8 text-sm text-stone-500">
        <Link
          href="/"
          className="text-emerald-500 hover:text-emerald-400 underline"
        >
          Back home
        </Link>
      </p>
    </FunPageLayout>
  );
}
