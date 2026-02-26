"use client";

import { useEffect } from "react";
import { Fun50xPage } from "@/components/ErrorPages";

export default function Error({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    console.error(error);
  }, [error]);

  return (
    <Fun50xPage
      onRetry={reset}
      message="Something went wrong on our side. Try again or head back home."
    />
  );
}
