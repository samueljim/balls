"use client";

import Link from "next/link";
import { FunPageLayout, FunPanel, funButtonPrimaryClass, funButtonSecondaryClass } from "@/components/layout/FunPageLayout";

const sharedBg = (
  <>
    <div className="absolute -bottom-20 left-1/4 w-72 h-72 rounded-full bg-[#1a2e1a]/40 blur-3xl" />
    <div className="absolute bottom-0 right-1/3 w-96 h-48 rounded-full bg-[#2d1a0a]/30 blur-3xl" />
    <div className="absolute top-1/3 right-0 w-64 h-64 rounded-full bg-[#1a3d1a]/20 blur-3xl" />
    <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[600px] h-32 bg-gradient-to-b from-[#22c55e]/10 to-transparent blur-2xl" />
  </>
);

/** Fun 40x (e.g. 404) page using app styling */
export function Fun40xPage({
  code = "404",
  title = "Page not found",
  tagline = "This page got blown up 💥",
  message = "Nothing wriggling here. Head back to base!",
  homeHref = "/",
  homeLabel = "Back to base",
}: {
  code?: string;
  title?: string;
  tagline?: string;
  message?: string;
  homeHref?: string;
  homeLabel?: string;
}) {
  return (
    <main className="min-h-screen relative flex flex-col items-center justify-center overflow-hidden bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
      <div className="absolute inset-0 overflow-hidden pointer-events-none">{sharedBg}</div>
      <div className="relative z-10 w-full max-w-md px-4 flex flex-col items-center">
        <p
          className="font-display text-7xl sm:text-8xl text-center mb-0 select-none animate-[float_3s_ease-in-out_infinite]"
          style={{
            color: "#22c55e",
            textShadow: "4px 4px 0 #166534, 6px 6px 0 rgba(0,0,0,0.4)",
          }}
        >
          {code}
        </p>
        <h1
          className="font-display text-3xl sm:text-4xl text-center mt-2 mb-1 select-none"
          style={{
            color: "#22c55e",
            textShadow: "2px 2px 0 #166534, 4px 4px 0 rgba(0,0,0,0.3)",
          }}
        >
          {title}
        </h1>
        <p className="font-display text-sm text-emerald-400/90 tracking-widest mb-6">{tagline}</p>
        <FunPanel className="text-center">
          <p className="text-stone-300 mb-6">{message}</p>
          <Link href={homeHref} className={`inline-block ${funButtonPrimaryClass} py-3 px-8`}>
            {homeLabel}
          </Link>
        </FunPanel>
      </div>
    </main>
  );
}

/** Fun 50x (server/runtime error) page with try-again + home */
export function Fun50xPage({
  title = "Something went wrong",
  tagline = "The server had a tummy ache 🪱",
  message = "We're on it. Try again or head back home.",
  onRetry,
  homeHref = "/",
  homeLabel = "Back to base",
}: {
  title?: string;
  tagline?: string;
  message?: string;
  onRetry?: () => void;
  homeHref?: string;
  homeLabel?: string;
}) {
  return (
    <main className="min-h-screen relative flex flex-col items-center justify-center overflow-hidden bg-gradient-to-b from-[#0d1f0d] via-[#0a0a0a] to-[#1a0f0a]">
      <div className="absolute inset-0 overflow-hidden pointer-events-none">{sharedBg}</div>
      <div className="relative z-10 w-full max-w-md px-4 flex flex-col items-center">
        <p
          className="font-display text-6xl sm:text-7xl text-center mb-0 select-none animate-[pulse-glow_2s_ease-in-out_infinite]"
          style={{
            color: "#22c55e",
            textShadow: "4px 4px 0 #166534, 6px 6px 0 rgba(0,0,0,0.4)",
          }}
        >
          500
        </p>
        <h1
          className="font-display text-3xl sm:text-4xl text-center mt-2 mb-1 select-none"
          style={{
            color: "#22c55e",
            textShadow: "2px 2px 0 #166534, 4px 4px 0 rgba(0,0,0,0.3)",
          }}
        >
          {title}
        </h1>
        <p className="font-display text-sm text-emerald-400/90 tracking-widest mb-6">{tagline}</p>
        <FunPanel className="text-center">
          <p className="text-stone-300 mb-6">{message}</p>
          <div className="flex flex-col sm:flex-row gap-3 justify-center">
            {onRetry && (
              <button type="button" onClick={onRetry} className={funButtonPrimaryClass}>
                Try again
              </button>
            )}
            <Link href={homeHref} className={onRetry ? funButtonSecondaryClass : funButtonPrimaryClass}>
              {homeLabel}
            </Link>
          </div>
        </FunPanel>
      </div>
    </main>
  );
}
