import type { Player } from "./types";

const TURN_TIME_MS = 45_000;

function generateCode(): string {
  const chars = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
  let code = "";
  for (let i = 0; i < 6; i++) {
    code += chars[Math.floor(Math.random() * chars.length)];
  }
  return code;
}

export class Lobby implements DurableObject {
  private state: DurableObjectState;
  private env: Record<string, unknown>;
  private lobbyCode: string = "";
  private hostId: string = "";
  private players: Player[] = [];
  private started: boolean = false;
  private gameId: string | null = null;
  private sockets: Map<string, WebSocket> = new Map();
  private playerIdToSocket: Map<string, string> = new Map();

  constructor(state: DurableObjectState, env: Record<string, unknown>) {
    this.state = state;
    this.env = env;
    this.state.blockConcurrencyWhile(async () => {
      const stored = await this.state.storage.get<{
        lobbyCode: string;
        hostId: string;
        players: Player[];
        started: boolean;
        gameId: string | null;
      }>("lobby");
      if (stored) {
        this.lobbyCode = stored.lobbyCode;
        this.hostId = stored.hostId;
        this.players = stored.players;
        this.started = stored.started;
        this.gameId = stored.gameId;
      }
    });
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (request.headers.get("Upgrade") === "websocket") {
      return this.handleWebSocket(request, url);
    }
    if (url.pathname === "/create" && request.method === "POST") {
      return this.handleCreate(request);
    }
    if (url.pathname === "/join" && request.method === "POST") {
      return this.handleJoin(request);
    }
    if (url.pathname === "/add" && request.method === "POST") {
      const body = await request.json().catch(() => ({})) as { playerName?: string };
      return this.handleAddPlayer(body);
    }
    return new Response("Not found", { status: 404 });
  }

  private async handleCreate(request: Request): Promise<Response> {
    if (this.lobbyCode) {
      return Response.json({ error: "Lobby already initialized" }, { status: 400 });
    }
    this.lobbyCode = generateCode();
    const id = this.state.id.toString();
    this.hostId = id;
    const hostPlayer: Player = {
      id,
      name: "Host",
      ready: false,
      isBot: false,
    };
    this.players = [hostPlayer];
    await this.persist();
    return Response.json({
      code: this.lobbyCode,
      lobbyId: this.state.id.toString(),
    });
  }

  private async handleJoin(request: Request): Promise<Response> {
    const body = await request.json().catch(() => ({})) as { code?: string; playerName?: string };
    const code = (body.code ?? "").toUpperCase().trim();
    const playerName = (body.playerName ?? "Player").trim().slice(0, 32);
    if (!code || !playerName) {
      return Response.json({ error: "code and playerName required" }, { status: 400 });
    }
    if (code !== this.lobbyCode) {
      return Response.json({ error: "Invalid or expired code" }, { status: 404 });
    }
    if (this.started) {
      return Response.json({ error: "Game already started" }, { status: 400 });
    }
    const playerId = crypto.randomUUID();
    const player: Player = { id: playerId, name: playerName, ready: false, isBot: false };
    this.players.push(player);
    await this.persist();
    const lobbyId = this.state.id.toString();
    return Response.json({
      lobbyId,
      playerId,
      playerName,
    });
  }

  /** Internal: add a player (called by Worker when join targets this lobby by code). */
  async handleAddPlayer(body: { playerName: string }): Promise<Response> {
    if (this.started) {
      return Response.json({ error: "Game already started" }, { status: 400 });
    }
    const playerName = (body.playerName ?? "Player").trim().slice(0, 32);
    const playerId = crypto.randomUUID();
    const player: Player = { id: playerId, name: playerName, ready: false, isBot: false };
    this.players.push(player);
    await this.persist();
    return Response.json({ playerId, playerName });
  }

  private async handleWebSocket(request: Request, url: URL): Promise<Response> {
    const playerId = url.searchParams.get("playerId");
    const playerName = url.searchParams.get("playerName");
    if (!playerId) {
      return new Response("playerId required", { status: 400 });
    }
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    this.state.acceptWebSocket(server);
    const existing = this.players.find((p) => p.id === playerId);
    if (existing) {
      existing.name = playerName ?? existing.name;
    } else {
      this.players.push({
        id: playerId,
        name: playerName ?? "Player",
        ready: false,
        isBot: false,
      });
    }
    this.sockets.set(playerId, server);
    this.playerIdToSocket.set(playerId, playerId);
    await this.persist();
    this.broadcast({ type: "player_list", players: this.players });
    return new Response(null, { status: 101, webSocket: client });
  }

  private async persist(): Promise<void> {
    await this.state.storage.put("lobby", {
      lobbyCode: this.lobbyCode,
      hostId: this.hostId,
      players: this.players,
      started: this.started,
      gameId: this.gameId,
    });
  }

  private broadcast(msg: { type: string; [k: string]: unknown }): void {
    const data = JSON.stringify(msg);
    for (const ws of this.sockets.values()) {
      try {
        ws.send(data);
      } catch (_) {}
    }
  }

  private sendTo(playerId: string, msg: { type: string; [k: string]: unknown }): void {
    const ws = this.sockets.get(playerId);
    if (ws) ws.send(JSON.stringify(msg));
  }

  async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
    const data = typeof message === "string" ? message : new TextDecoder().decode(message);
    let playerId: string | null = null;
    for (const [pid, s] of this.sockets) {
      if (s === ws) {
        playerId = pid;
        break;
      }
    }
    if (!playerId) return;
    try {
      const msg = JSON.parse(data) as { type: string; ready?: boolean; playerId?: string };
      if (msg.type === "set_ready") {
        const p = this.players.find((x) => x.id === playerId);
        if (p) {
          p.ready = msg.ready ?? false;
          await this.persist();
          this.broadcast({ type: "player_list", players: this.players });
        }
      } else if (msg.type === "start_game") {
        if (this.hostId !== playerId) {
          this.sendTo(playerId, { type: "error", message: "Only host can start" });
          return;
        }
        if (this.started) return;
        this.started = true;
        const gameId = this.state.id.toString() + "-game";
        this.gameId = gameId;
        await this.persist();
        const playerOrder = this.players.map((p) => ({
          playerId: p.id,
          isBot: p.isBot ?? false,
          name: p.name,
        }));
        this.broadcast({ type: "game_started", gameId, playerOrder });
      } else if (msg.type === "add_bot") {
        if (this.hostId !== playerId) return;
        const botNames = [
          "Captain Wiggles", "Sir Bouncesalot", "Private Noodle", "Sergeant Splodey",
          "Colonel Crater", "Major Mayhem", "Private Parts", "General Chaos", "Admiral Boom",
          "Lieutenant Left", "Corporal Crumble", "Sargeant Splodey", "Private Puff", "Captain Crater",
        ];
        const used = new Set(this.players.filter((p) => p.isBot).map((p) => p.name));
        const name = botNames.find((n) => !used.has(n)) ?? `Bot ${this.players.length + 1}`;
        const bot: Player = { id: `bot-${crypto.randomUUID()}`, name, ready: true, isBot: true };
        this.players.push(bot);
        await this.persist();
        this.broadcast({ type: "add_bot", player: bot });
      } else if (msg.type === "remove_bot" && msg.playerId) {
        if (this.hostId !== playerId) return;
        this.players = this.players.filter((p) => p.id !== msg.playerId);
        await this.persist();
        this.broadcast({ type: "remove_bot", playerId: msg.playerId });
        this.broadcast({ type: "player_list", players: this.players });
      }
    } catch (_) {}
  }

  async webSocketClose(ws: WebSocket): Promise<void> {
    let playerId: string | null = null;
    for (const [pid, s] of this.sockets) {
      if (s === ws) {
        playerId = pid;
        break;
      }
    }
    if (playerId) {
      this.sockets.delete(playerId);
      this.playerIdToSocket.delete(playerId);
      if (!this.started) {
        this.players = this.players.filter((p) => p.id !== playerId);
        await this.persist();
        this.broadcast({ type: "player_list", players: this.players });
      }
    }
  }
}
