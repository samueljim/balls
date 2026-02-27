"use client";

import { usePathname } from "next/navigation";
import { Suspense, useState, useEffect } from "react";
import dynamic from "next/dynamic";
import { Fun40xPage } from "@/components/ErrorPages";

const LobbyContent = dynamic(
  () => import("@/app/lobby/[id]/LobbyContent").then((m) => m.default),
  { ssr: false }
);
const GameView = dynamic(
  () => import("@/app/game/[id]/GameClient").then((m) => m.default),
  { ssr: false }
);
const JoinContent = dynamic(
  () => import("@/app/join/[code]/JoinContent").then((m) => m.default),
  { ssr: false }
);

const loadingFallback = (
  <div className="min-h-screen flex items-center justify-center bg-[#0d1f0d] text-emerald-500 font-display">
    Loading…
  </div>
);

function NotFoundFallbackInner() {
  const [mounted, setMounted] = useState(false);
  const pathnameFromRouter = usePathname();

  useEffect(() => {
    setMounted(true);
  }, []);

  if (!mounted) {
    return loadingFallback;
  }

  const pathname = pathnameFromRouter ?? (typeof window !== "undefined" ? window.location.pathname : "");
  const search = typeof window !== "undefined" ? window.location.search : "";

  const lobbyMatch = pathname.match(/^\/lobby\/([^/]+)\/?$/);
  const gameMatch = pathname.match(/^\/game\/([^/]+)\/?$/);
  const joinMatch = pathname.match(/^\/join\/([^/]+)\/?$/);

  if (lobbyMatch) {
    return (
      <Suspense fallback={loadingFallback}>
        <LobbyContent overrideId={lobbyMatch[1]} overrideSearch={search} />
      </Suspense>
    );
  }
  if (gameMatch) {
    return (
      <Suspense fallback={loadingFallback}>
        <GameView overrideId={gameMatch[1]} />
      </Suspense>
    );
  }
  if (joinMatch) {
    return (
      <Suspense fallback={loadingFallback}>
        <JoinContent overrideCode={joinMatch[1]} overrideSearch={search} />
      </Suspense>
    );
  }

  return <Fun40xPage />;
}

export function NotFoundFallback() {
  return (
    <Suspense fallback={<div className="min-h-screen flex items-center justify-center bg-[#0d1f0d] text-emerald-500">Loading…</div>}>
      <NotFoundFallbackInner />
    </Suspense>
  );
}
