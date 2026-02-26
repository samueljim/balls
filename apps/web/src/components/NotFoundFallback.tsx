"use client";

import { usePathname } from "next/navigation";
import { Suspense } from "react";
import dynamic from "next/dynamic";

const LobbyContent = dynamic(
  () => import("@/app/lobby/[id]/LobbyContent").then((m) => m.default),
  { ssr: false }
);
const GameContent = dynamic(
  () => import("@/app/game/[id]/GameContent").then((m) => m.default),
  { ssr: false }
);
const JoinContent = dynamic(
  () => import("@/app/join/[code]/JoinContent").then((m) => m.default),
  { ssr: false }
);

function NotFoundFallbackInner() {
  const pathname = usePathname() ?? (typeof window !== "undefined" ? window.location.pathname : "");
  const search = typeof window !== "undefined" ? window.location.search : "";

  const lobbyMatch = pathname.match(/^\/lobby\/([^/]+)\/?$/);
  const gameMatch = pathname.match(/^\/game\/([^/]+)\/?$/);
  const joinMatch = pathname.match(/^\/join\/([^/]+)\/?$/);

  if (lobbyMatch) {
    return (
      <Suspense fallback={<div className="min-h-screen flex items-center justify-center bg-[#0d1f0d] text-emerald-500">Loading…</div>}>
        <LobbyContent overrideId={lobbyMatch[1]} overrideSearch={search} />
      </Suspense>
    );
  }
  if (gameMatch) {
    return (
      <Suspense fallback={<div className="min-h-screen flex items-center justify-center bg-[#0d1f0d] text-emerald-500">Loading…</div>}>
        <GameContent overrideId={gameMatch[1]} overrideSearch={search} />
      </Suspense>
    );
  }
  if (joinMatch) {
    return (
      <Suspense fallback={<div className="min-h-screen flex items-center justify-center bg-[#0d1f0d] text-emerald-500">Loading…</div>}>
        <JoinContent overrideCode={joinMatch[1]} overrideSearch={search} />
      </Suspense>
    );
  }

  return (
    <main className="min-h-screen flex flex-col items-center justify-center bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
      <h1 className="font-display text-4xl text-emerald-500 mb-4">Page not found</h1>
      <a href="/" className="text-emerald-400 hover:text-emerald-300 underline">
        Back home
      </a>
    </main>
  );
}

export function NotFoundFallback() {
  return (
    <Suspense fallback={<div className="min-h-screen flex items-center justify-center bg-[#0d1f0d] text-emerald-500">Loading…</div>}>
      <NotFoundFallbackInner />
    </Suspense>
  );
}
