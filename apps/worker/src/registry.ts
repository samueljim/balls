/** Rate limit: max lookups per code per window. Prevents brute-force of lobby codes. */
const RATE_LIMIT_LOOKUPS_PER_CODE = 15;
const RATE_LIMIT_WINDOW_MS = 60_000;

/** Single DO that maps lobby code -> lobby id for join lookup. */
export class Registry implements DurableObject {
  private state: DurableObjectState;
  private map: Map<string, string> = new Map();
  /** Per-code lookup timestamps for rate limiting (code -> timestamps in last window) */
  private lookupTimestamps: Map<string, number[]> = new Map();

  constructor(state: DurableObjectState, _env: unknown) {
    this.state = state;
    this.state.blockConcurrencyWhile(async () => {
      const stored = await this.state.storage.get<[string, string][]>("codes");
      if (stored) this.map = new Map(stored);
    });
  }

  private isRateLimited(code: string): boolean {
    const now = Date.now();
    const cutoff = now - RATE_LIMIT_WINDOW_MS;
    let timestamps = this.lookupTimestamps.get(code) ?? [];
    timestamps = timestamps.filter((t) => t > cutoff);
    if (timestamps.length >= RATE_LIMIT_LOOKUPS_PER_CODE) return true;
    timestamps.push(now);
    this.lookupTimestamps.set(code, timestamps);
    return false;
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/put" && request.method === "POST") {
      const body = await request.json() as { code: string; lobbyId: string };
      this.map.set(body.code, body.lobbyId);
      await this.state.storage.put("codes", Array.from(this.map));
      return Response.json({ ok: true });
    }
    if (url.pathname === "/delete" && request.method === "POST") {
      const body = await request.json() as { code: string };
      if (body.code && this.map.has(body.code)) {
        this.map.delete(body.code);
        await this.state.storage.put("codes", Array.from(this.map));
      }
      return Response.json({ ok: true });
    }
    if (url.pathname === "/get" && request.method === "GET") {
      const code = url.searchParams.get("code");
      if (!code) return Response.json({ lobbyId: null });
      if (this.isRateLimited(code)) {
        return Response.json({ error: "Too many attempts", lobbyId: null }, { status: 429 });
      }
      const lobbyId = this.map.get(code) ?? null;
      return Response.json({ lobbyId });
    }
    return new Response("Not found", { status: 404 });
  }
}
