"use client";

import "./globals.css";
import { Fun50xPage } from "@/components/ErrorPages";

export default function GlobalError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  return (
    <html lang="en">
      <head>
        <link
          href="https://fonts.googleapis.com/css2?family=Luckiest+Guy&display=swap"
          rel="stylesheet"
        />
      </head>
      <body className="min-h-screen antialiased font-sans bg-[#0a0a0a] text-stone-100">
        <Fun50xPage
          onRetry={reset}
          message="A critical error occurred. Try again or head back home."
        />
      </body>
    </html>
  );
}
