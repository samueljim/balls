import { Suspense } from "react";
import LobbyContent from "./LobbyContent";
import { LobbyLoading } from "@/components/LobbyLoading";

export function generateStaticParams() {
  return [{ id: "default" }];
}

export default function LobbyPage() {
  return (
    <Suspense fallback={<LobbyLoading />}>
      <LobbyContent />
    </Suspense>
  );
}
