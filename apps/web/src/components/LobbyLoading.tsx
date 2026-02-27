"use client";

export function LobbyLoading() {
  return (
    <main className="min-h-screen relative flex flex-col items-center justify-center overflow-hidden bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -bottom-20 left-1/4 w-72 h-72 rounded-full bg-[#1a2e1a]/40 blur-3xl" />
        <div className="absolute bottom-0 right-1/3 w-96 h-48 rounded-full bg-[#2d1a0a]/30 blur-3xl" />
        <div className="absolute top-1/3 right-0 w-64 h-64 rounded-full bg-[#1a3d1a]/20 blur-3xl" />
      </div>
      <div className="relative z-10 flex flex-col items-center gap-8">
        <div className="flex items-center gap-2" aria-hidden>
          <span
            className="w-3 h-3 rounded-full bg-emerald-500 animate-[lobby-dot_1.2s_ease-in-out_infinite]"
            style={{ animationDelay: "0ms" }}
          />
          <span
            className="w-3 h-3 rounded-full bg-emerald-400 animate-[lobby-dot_1.2s_ease-in-out_infinite]"
            style={{ animationDelay: "150ms" }}
          />
          <span
            className="w-3 h-3 rounded-full bg-emerald-500 animate-[lobby-dot_1.2s_ease-in-out_infinite]"
            style={{ animationDelay: "300ms" }}
          />
          <span
            className="w-3 h-3 rounded-full bg-emerald-400 animate-[lobby-dot_1.2s_ease-in-out_infinite]"
            style={{ animationDelay: "450ms" }}
          />
        </div>
        <p className="font-display text-emerald-500/90 tracking-widest text-sm animate-[lobby-wiggle_2s_ease-in-out_infinite]">
          CONNECTING…
        </p>
      </div>
    </main>
  );
}
