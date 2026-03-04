import { Lobby } from "./lobby";
import { Game } from "./game";
import { Registry } from "./registry";

// Re-export so Wrangler can register Durable Objects (do not remove)
export { Lobby, Game, Registry };

export interface Env {
  LOBBY: DurableObjectNamespace;
  GAME: DurableObjectNamespace;
  REGISTRY: DurableObjectNamespace;
  /** Comma-separated allowed origins (e.g. "https://worms.example.com,https://www.worms.example.com"). If unset, allows "*". */
  ALLOWED_ORIGINS?: string;
}

function getCorsHeaders(env: Env, request: Request): Record<string, string> {
  const base: Record<string, string> = {
    "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type",
    "Access-Control-Max-Age": "86400",
  };
  const origins = env.ALLOWED_ORIGINS?.trim();
  if (origins) {
    const origin = request.headers.get("Origin") ?? "";
    const allowed = origins.split(",").map((o) => o.trim()).filter(Boolean);
    base["Access-Control-Allow-Origin"] = allowed.includes(origin) ? origin : allowed[0] ?? "*";
  } else {
    base["Access-Control-Allow-Origin"] = "*";
  }
  return base;
}

function corsResponse(response: Response, request: Request, env: Env): Response {
  const next = new Response(response.body, { status: response.status, statusText: response.statusText, headers: response.headers });
  Object.entries(getCorsHeaders(env, request)).forEach(([k, v]) => next.headers.set(k, v));
  return next;
}

function corsJson(body: unknown, init?: ResponseInit, request?: Request, env?: Env): Response {
  const res = Response.json(body, init);
  const headers = env && request ? getCorsHeaders(env, request) : { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Methods": "GET, POST, OPTIONS", "Access-Control-Allow-Headers": "Content-Type", "Access-Control-Max-Age": "86400" };
  Object.entries(headers).forEach(([k, v]) => res.headers.set(k, v));
  return res;
}

// Keep DO classes in bundle so Wrangler can inject env.LOBBY etc.
const _doClasses = [Lobby, Game, Registry];

export default {
  async fetch(request: Request, env: Env, _ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);

    // CORS preflight
    if (request.method === "OPTIONS") {
      return new Response(null, { status: 204, headers: getCorsHeaders(env, request) });
    }

    // GET / -> health/status
    if (url.pathname === "/" && request.method === "GET") {
      return corsJson({ status: "ok" }, undefined, request, env);
    }
    // POST /lobby/create -> create new Lobby DO, return { code, lobbyId }
    if (url.pathname === "/lobby/create" && request.method === "POST") {
      try {
        const LOBBY = env.LOBBY;
        const REGISTRY = env.REGISTRY;
        if (!LOBBY || !REGISTRY) {
          return corsJson(
            { error: "Durable Objects not configured (LOBBY or REGISTRY missing)." },
            { status: 503 },
            request,
            env
          );
        }
        if (typeof LOBBY.newUniqueId !== "function") {
          return corsJson(
            { error: "LOBBY.newUniqueId is not available. Ensure the worker is deployed with Durable Object bindings (wrangler.toml)." },
            { status: 503 },
            request,
            env
          );
        }
        const id = LOBBY.newUniqueId();
        const stub = LOBBY.get(id);
        const createRes = await stub.fetch(new Request("https://x/create", { method: "POST" }));
        const data = await createRes.json() as { code?: string; lobbyId?: string; error?: string };
        if (data.error) {
          return corsJson({ error: data.error }, { status: 400 }, request, env);
        }
        if (data.code) {
          const registry = REGISTRY.get(REGISTRY.idFromName("default"));
          await registry.fetch(
            new Request("https://r/put", {
              method: "POST",
              body: JSON.stringify({ code: data.code, lobbyId: id.toString() }),
            })
          );
        }
        return corsJson({ code: data.code, lobbyId: id.toString() }, undefined, request, env);
      } catch (err) {
        return corsJson({ error: String(err) }, { status: 500 }, request, env);
      }
    }

// POST /lobby/join -> lookup code, add player to that Lobby, return { lobbyId, playerId, playerName }
    if (url.pathname === "/lobby/join" && request.method === "POST") {
      try {
        const body = await request.json().catch(() => ({})) as { code?: string; playerName?: string };
        const code = (body.code ?? "").toUpperCase().trim();
        if (!code) {
          return corsJson({ error: "code required" }, { status: 400 }, request, env);
        }
        const registry = env.REGISTRY.get(env.REGISTRY.idFromName("default"));
        const getRes = await registry.fetch(new Request(`https://r/get?code=${encodeURIComponent(code)}`));
        const getData = await getRes.json() as { lobbyId?: string | null; error?: string };
        if (getRes.status === 429) {
          return corsJson({ error: getData.error ?? "Too many join attempts. Try again later." }, { status: 429 }, request, env);
        }
        const lobbyId = getData.lobbyId ?? null;
        if (!lobbyId) {
          return corsJson({ error: "Invalid or expired code" }, { status: 404 }, request, env);
        }
        const lobby = env.LOBBY.get(env.LOBBY.idFromString(lobbyId));
        const addRes = await lobby.fetch(
          new Request("https://in/add", {
            method: "POST",
            body: JSON.stringify({ playerName: (body.playerName ?? "").trim() || undefined }),
          })
        );
        const addData = await addRes.json() as {
          playerId?: string;
          playerName?: string;
          error?: string;
          gameId?: string;
          playerOrder?: { playerId: string; isBot: boolean; name: string }[];
        };
        if (addData.error) {
          return corsJson({ error: addData.error }, { status: addRes.status }, request, env);
        }
        const payload: Record<string, unknown> = {
          lobbyId,
          playerId: addData.playerId,
          playerName: addData.playerName,
        };
        if (addData.gameId && addData.playerOrder) {
          payload.gameId = addData.gameId;
          payload.playerOrder = addData.playerOrder;
        }
        return corsJson(payload, undefined, request, env);
      } catch (err) {
        return corsJson({ error: String(err) }, { status: 500 }, request, env);
      }
    }

    // WebSocket /lobby/:lobbyId -> forward to Lobby DO (no CORS needed for WS)
    if (url.pathname.startsWith("/lobby/") && request.headers.get("Upgrade") === "websocket") {
      const lobbyId = url.pathname.slice("/lobby/".length).split("?")[0];
      if (!lobbyId) return corsResponse(new Response("lobbyId required", { status: 400 }), request, env);
      try {
        const stub = env.LOBBY.get(env.LOBBY.idFromString(lobbyId));
        return stub.fetch(request);
      } catch {
        return corsResponse(new Response("Invalid lobby", { status: 400 }), request, env);
      }
    }

    // WebSocket /game/:gameId -> forward to Game DO
    if (url.pathname.startsWith("/game/") && !url.pathname.includes("/init") && request.headers.get("Upgrade") === "websocket") {
      const gameId = url.pathname.slice("/game/".length).split("?")[0];
      if (!gameId) return corsResponse(new Response("gameId required", { status: 400 }), request, env);
      try {
        const stub = env.GAME.get(env.GAME.idFromName(gameId));
        return stub.fetch(request);
      } catch {
        return corsResponse(new Response("Invalid game", { status: 400 }), request, env);
      }
    }

    // POST /game/:gameId/init -> init a Game DO (called by client when game starts)
    const gameInitMatch = url.pathname.match(/^\/game\/([^/]+)\/init$/);
    if (gameInitMatch && request.method === "POST") {
      const gameId = gameInitMatch[1];
      try {
        const stub = env.GAME.get(env.GAME.idFromName(gameId));
        const res = await stub.fetch(new Request(request.url, { method: "POST", body: request.body }));
        return corsResponse(res, request, env);
      } catch {
        return corsJson({ error: "Invalid game" }, { status: 400 }, request, env);
      }
    }

    return corsResponse(new Response("Not found", { status: 404 }), request, env);
  },
};
