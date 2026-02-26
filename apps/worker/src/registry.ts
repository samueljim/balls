/** Single DO that maps lobby code -> lobby id for join lookup. */
export class Registry implements DurableObject {
  private state: DurableObjectState;
  private map: Map<string, string> = new Map();

  constructor(state: DurableObjectState, _env: unknown) {
    this.state = state;
    this.state.blockConcurrencyWhile(async () => {
      const stored = await this.state.storage.get<[string, string][]>("codes");
      if (stored) this.map = new Map(stored);
    });
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (url.pathname === "/put" && request.method === "POST") {
      const body = await request.json() as { code: string; lobbyId: string };
      this.map.set(body.code, body.lobbyId);
      await this.state.storage.put("codes", Array.from(this.map));
      return Response.json({ ok: true });
    }
    if (url.pathname === "/get" && request.method === "GET") {
      const code = url.searchParams.get("code");
      const lobbyId = code ? this.map.get(code) ?? null : null;
      return Response.json({ lobbyId });
    }
    return new Response("Not found", { status: 404 });
  }
}
