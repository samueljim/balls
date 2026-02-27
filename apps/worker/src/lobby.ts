import type { Player } from "./types";

const TURN_TIME_MS = 45_000;

const FUNNY_NAMES = [
  "Captain Wiggles", "Sir Bouncesalot", "Private Noodle", "Sergeant Splodey",
  "Colonel Crater", "Major Mayhem", "Private Parts", "General Chaos", "Admiral Boom",
  "Lieutenant Left", "Corporal Crumble", "Private Puff", "Captain Crater",
];

function pickFunnyName(used: Set<string>): string {
  const name = FUNNY_NAMES.find((n) => !used.has(n)) ?? `Ball ${used.size + 1}`;
  return name;
}

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
  private gamePlayerOrder: { playerId: string; isBot: boolean; name: string }[] = [];
  private rngSeed: number = 0;
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
        gamePlayerOrder: { playerId: string; isBot: boolean; name: string }[];
        rngSeed?: number;
      }>("lobby");
      if (stored) {
        this.lobbyCode = stored.lobbyCode;
        this.hostId = stored.hostId;
        this.players = stored.players;
        this.started = stored.started;
        this.gameId = stored.gameId;
        this.gamePlayerOrder = stored.gamePlayerOrder ?? [];
        this.rngSeed = stored.rngSeed ?? 0;
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
      name: "", // set when host connects (client sends their chosen/random name)
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
    if (!code) {
      return Response.json({ error: "code required" }, { status: 400 });
    }
    if (code !== this.lobbyCode) {
      return Response.json({ error: "Invalid or expired code" }, { status: 404 });
    }
    if (this.started) {
      return Response.json({ error: "Game already started" }, { status: 400 });
    }
    const usedNames = new Set(this.players.map((p) => p.name));
    const rawName = (body.playerName ?? "").trim().slice(0, 32);
    const playerName = rawName || pickFunnyName(usedNames);
    const playerId = crypto.randomUUID();
    const player: Player = { id: playerId, name: playerName, ready: false, isBot: false };
    this.players.push(player);
    await this.persist();
    const lobbyId = this.state.id.toString();
    if (this.started && this.gameId && this.gamePlayerOrder.length > 0) {
      return Response.json({
        lobbyId,
        playerId,
        playerName,
        gameId: this.gameId,
        playerOrder: this.gamePlayerOrder,
        rngSeed: this.rngSeed,
      });
    }
    return Response.json({
      lobbyId,
      playerId,
      playerName,
    });
  }

  /** Internal: add a player (called by Worker when join targets this lobby by code). */
  async handleAddPlayer(body: { playerName?: string }): Promise<Response> {
    if (this.started) {
      return Response.json({ error: "Game already started" }, { status: 400 });
    }
    const usedNames = new Set(this.players.map((p) => p.name));
    const rawName = (body.playerName ?? "").trim().slice(0, 32);
    const playerName = rawName || pickFunnyName(usedNames);
    const playerId = crypto.randomUUID();
    const player: Player = { id: playerId, name: playerName, ready: false, isBot: false };
    this.players.push(player);
    await this.persist();
    if (this.started && this.gameId && this.gamePlayerOrder.length > 0) {
      return Response.json({
        playerId,
        playerName,
        gameId: this.gameId,
        playerOrder: this.gamePlayerOrder,
        rngSeed: this.rngSeed,
      });
    }
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
    const incomingName = (playerName ?? "").trim().slice(0, 32);
    const isPlaceholder = !incomingName || incomingName === "…" || incomingName === "...";
    const existing = this.players.find((p) => p.id === playerId);
    if (existing) {
      if (incomingName && !isPlaceholder) existing.name = incomingName;
    } else {
      this.players.push({
        id: playerId,
        name: incomingName && !isPlaceholder ? incomingName : pickFunnyName(new Set(this.players.map((p) => p.name))),
        ready: false,
        isBot: false,
      });
    }
    this.sockets.set(playerId, server);
    this.playerIdToSocket.set(playerId, playerId);
    await this.persist();
    this.broadcast({ type: "player_list", players: [...this.players] });
    return new Response(null, { status: 101, webSocket: client });
  }

  private async persist(): Promise<void> {
    await this.state.storage.put("lobby", {
      lobbyCode: this.lobbyCode,
      hostId: this.hostId,
      players: this.players,
      started: this.started,
      gameId: this.gameId,
      gamePlayerOrder: this.gamePlayerOrder,
      rngSeed: this.rngSeed,
    });
  }

  /** Send to all connected clients. Used for real-time lobby state (player_list on join/leave/rename/bots). */
  private broadcast(msg: { type: string; [k: string]: unknown }): void {
    const data = JSON.stringify(msg);
    for (const ws of this.sockets.values()) {
      try {
        ws.send(data);
      } catch (_) {}
    }
  }

  /** Send current player list to one client (e.g. after get_player_list or on connect). */
  private sendPlayerListTo(playerId: string): void {
    this.sendTo(playerId, { type: "player_list", players: [...this.players] });
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
      const msg = JSON.parse(data) as { type: string; ready?: boolean; playerId?: string; playerName?: string };
      if (msg.type === "get_player_list") {
        this.sendPlayerListTo(playerId);
        return;
      }
      if (msg.type === "set_name" && typeof msg.playerName === "string") {
        const p = this.players.find((x) => x.id === playerId);
        if (p) {
          p.name = msg.playerName.trim().slice(0, 32) || p.name;
          await this.persist();
          this.broadcast({ type: "player_list", players: [...this.players] });
        }
      } else if (msg.type === "set_ready") {
        const p = this.players.find((x) => x.id === playerId);
        if (p) {
          p.ready = msg.ready ?? false;
          await this.persist();
          this.broadcast({ type: "player_list", players: [...this.players] });
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
        this.gamePlayerOrder = this.players.map((p) => ({
          playerId: p.id,
          isBot: p.isBot ?? false,
          name: p.name,
        }));
        // Generate random seed for consistent terrain/randomness across all players
        this.rngSeed = Math.floor(Math.random() * 0xFFFFFFFF);
        await this.persist();
        this.broadcast({ type: "game_started", gameId, playerOrder: this.gamePlayerOrder, rngSeed: this.rngSeed });
      } else if (msg.type === "add_bot") {
        if (this.hostId !== playerId) {
          this.sendTo(playerId, { type: "error", message: "Only host can add bots" });
          return;
        }
        const used = new Set(this.players.filter((p) => p.isBot).map((p) => p.name));
        const name = FUNNY_NAMES.find((n) => !used.has(n)) ?? `Bot ${this.players.length + 1}`;
        const bot: Player = { id: `bot-${crypto.randomUUID()}`, name, ready: true, isBot: true };
        this.players.push(bot);
        await this.persist();
        this.broadcast({ type: "player_list", players: [...this.players] });
      } else if (msg.type === "remove_bot" && msg.playerId) {
        if (this.hostId !== playerId) {
          this.sendTo(playerId, { type: "error", message: "Only host can remove bots" });
          return;
        }
        this.players = this.players.filter((p) => p.id !== msg.playerId);
        await this.persist();
        this.broadcast({ type: "remove_bot", playerId: msg.playerId });
        this.broadcast({ type: "player_list", players: [...this.players] });
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
      // Only remove from lobby list and broadcast when game hasn't started (tab close / disconnect)
      if (!this.started) {
        this.players = this.players.filter((p) => p.id !== playerId);
        await this.persist();
        this.broadcast({ type: "player_list", players: [...this.players] });
      }
    }
  }
}
