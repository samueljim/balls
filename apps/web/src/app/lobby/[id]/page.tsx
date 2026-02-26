import { Suspense } from "react";
import LobbyContent from "./LobbyContent";

export function generateStaticParams() {
  return [{ id: "default" }];
}

export default function LobbyPage() {
  return (
    <Suspense fallback={<div className="min-h-screen flex items-center justify-center bg-[#0d1f0d] text-emerald-500">Loading…</div>}>
      <LobbyContent />
    </Suspense>
  );
}
