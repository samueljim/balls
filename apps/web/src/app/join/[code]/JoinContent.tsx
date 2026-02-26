"use client";

import { useParams, useRouter } from "next/navigation";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { FunPageLayout, FunPanel, funInputClass, funButtonPrimaryClass } from "@/components/layout/FunPageLayout";
import { API_BASE, apiJson } from "@/lib/api";

export default function JoinContent() {
  const params = useParams();
  const router = useRouter();
  const code = (params.code as string)?.toUpperCase() ?? "";
  const [name, setName] = useState("");
  const [joining, setJoining] = useState(false);
  const [error, setError] = useState("");

  async function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!name.trim()) return;
    setJoining(true);
    setError("");
    try {
      const res = await fetch(`${API_BASE}/lobby/join`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code, playerName: name.trim() }),
      });
      const data = await apiJson<{ error?: string; lobbyId?: string; playerId?: string; playerName?: string }>(res);
      if (data.error) throw new Error(data.error);
      router.push(`/lobby/${data.lobbyId}?playerId=${data.playerId}&playerName=${encodeURIComponent(data.playerName ?? "")}`);
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
              YOUR NAME
            </label>
            <Input
              placeholder="Enter your name"
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
            {joining ? "JOINING…" : "JOIN"}
          </Button>
        </form>
      </FunPanel>
      <p className="mt-8 text-sm text-stone-500">No sign-up. Just your name. Then play.</p>
    </FunPageLayout>
  );
}
