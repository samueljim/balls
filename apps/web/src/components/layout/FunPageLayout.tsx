"use client";

export function FunPageLayout({
  children,
  title,
  tagline,
}: {
  children: React.ReactNode;
  title: string;
  tagline?: string;
}) {
  return (
    <main className="min-h-screen relative flex flex-col items-center justify-center overflow-hidden bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
      <div className="absolute inset-0 overflow-hidden pointer-events-none">
        <div className="absolute -bottom-20 left-1/4 w-72 h-72 rounded-full bg-[#1a2e1a]/40 blur-3xl" />
        <div className="absolute bottom-0 right-1/3 w-96 h-48 rounded-full bg-[#2d1a0a]/30 blur-3xl" />
        <div className="absolute top-1/3 right-0 w-64 h-64 rounded-full bg-[#1a3d1a]/20 blur-3xl" />
        <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[600px] h-32 bg-gradient-to-b from-[#22c55e]/10 to-transparent blur-2xl" />
      </div>
      <div className="relative z-10 w-full max-w-md px-4 flex flex-col items-center">
        <h1
          className="font-display text-5xl sm:text-6xl text-center mb-1 select-none"
          style={{
            color: "#22c55e",
            textShadow: "3px 3px 0 #166534, 5px 5px 0 rgba(0,0,0,0.4)",
          }}
        >
          {title}
        </h1>
        {tagline && (
          <p className="font-display text-sm text-emerald-400/90 tracking-widest mb-8 -mt-1">
            {tagline}
          </p>
        )}
        {children}
      </div>
    </main>
  );
}

export function FunPanel({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return (
    <div
      className={`w-full rounded-2xl border-4 border-emerald-800/80 bg-gradient-to-b from-stone-900/95 to-stone-950/98 p-8 shadow-2xl ${className}`}
      style={{
        boxShadow: "inset 0 1px 0 rgba(255,255,255,0.06), 0 25px 50px -12px rgba(0,0,0,0.6)",
      }}
    >
      {children}
    </div>
  );
}

export const funInputClass =
  "h-12 rounded-xl border-2 border-stone-600 bg-stone-950/80 placeholder:text-stone-500 focus:border-emerald-500 focus:ring-emerald-500/30 text-stone-100 px-4";
export const funButtonPrimaryClass =
  "w-full h-12 font-display rounded-xl bg-gradient-to-b from-emerald-500 to-emerald-700 hover:from-emerald-400 hover:to-emerald-600 text-emerald-950 border-2 border-emerald-400/50 shadow-lg hover:scale-[1.02] active:scale-[0.98] transition-all duration-200";
export const funButtonSecondaryClass =
  "w-full h-12 font-display rounded-xl bg-stone-700 hover:bg-stone-600 border-2 border-stone-500 text-stone-100";
