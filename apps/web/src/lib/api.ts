/**
 * Backend API base URL. Defaults to the deployed worker at api.worms.bne.sh.
 * Override with NEXT_PUBLIC_API_BASE for local worker (e.g. http://localhost:8787).
 */
const DEFAULT_API_BASE = "https://api.worms.bne.sh";
export const API_BASE =
  typeof window !== "undefined"
    ? (process.env.NEXT_PUBLIC_API_BASE ?? DEFAULT_API_BASE)
    : (process.env.NEXT_PUBLIC_API_BASE ?? DEFAULT_API_BASE);

export async function apiJson<T = unknown>(res: Response): Promise<T> {
  const contentType = res.headers.get("content-type") ?? "";
  if (!contentType.includes("application/json")) {
    const text = await res.text();
    if (text.trimStart().startsWith("<!")) {
      throw new Error(
        "Backend returned HTML instead of JSON. Set NEXT_PUBLIC_API_BASE to your Worker URL (e.g. http://localhost:8787)."
      );
    }
    throw new Error(text || res.statusText || "Request failed");
  }
  return res.json() as Promise<T>;
}
