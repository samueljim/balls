import type { GameState } from "./types";

const TURN_TIME_MS = 45_000;

export class Game implements DurableObject {
  private state: DurableObjectState;
  private gameState: GameState = {
    playerOrder: [],
    inputLog: [],
    currentTurnIndex: 0,
    turnEndTime: 0,
    phase: "aiming",
    rngSeed: 0, // Set by lobby via /init POST
    terrainId: 0,
  };
  private sockets: Map<string, WebSocket> = new Map();
  private playerIdToIndex: Map<string, number> = new Map();
  /** Accumulated terrain damage events [[cx,cy,r], ...] for replay on reconnect */
  private terrainDamageLog: number[][] = [];
  /** Latest worm state snapshot for replay on reconnect */
  private lastWormState: { type: string; [k: string]: unknown } | null = null;

  constructor(state: DurableObjectState, _env: unknown) {
    this.state = state;
  }

  async fetch(request: Request): Promise<Response> {
    const url = new URL(request.url);
    if (request.headers.get("Upgrade") === "websocket") {
      return this.handleWebSocket(request, url);
    }
    if (url.pathname.endsWith("/init") && request.method === "POST") {
      return this.handleInit(request);
    }
    return new Response("Not found", { status: 404 });
  }

  private async handleInit(request: Request): Promise<Response> {
    // Idempotent: if game already in progress, reconnecting clients must not reset state
    if (this.gameState.playerOrder.length > 0) {
      return Response.json({ ok: true, alreadyInitialized: true });
    }
    const body = await request.json().catch(() => ({})) as {
      playerOrder?: { playerId: string; isBot: boolean; name: string }[];
      rngSeed?: number;
      terrainId?: number;
    };
    this.gameState.playerOrder = body.playerOrder ?? [];
    // Use seed from lobby (always provided via start_game)
    this.gameState.rngSeed = body.rngSeed ?? Math.floor(Math.random() * 0xFFFFFFFF);
    this.gameState.terrainId = body.terrainId ?? 0;
    this.gameState.inputLog = [];
    this.gameState.currentTurnIndex = 0;
    this.gameState.phase = "aiming";
    this.gameState.turnEndTime = Date.now() + TURN_TIME_MS;
    this.playerIdToIndex.clear();
    this.gameState.playerOrder.forEach((p, i) => this.playerIdToIndex.set(p.playerId, i));
    // Send identity to all already-connected sockets (they connected before /init was called)
    for (const [pid, ws] of this.sockets) {
      const idx = this.playerIdToIndex.get(pid);
      if (idx !== undefined) {
        try {
          ws.send(JSON.stringify({
            type: "identity",
            myPlayerIndex: idx,
            playerId: pid,
            rngSeed: this.gameState.rngSeed,
          }));
        } catch (_) {}
      }
    }
    this.broadcast({ type: "state", state: this.gameState });

    if (this.gameState.playerOrder[0]?.isBot) {
      setTimeout(() => this.maybeBotTurn(), 500);
    }
    return Response.json({ ok: true });
  }

  private async handleWebSocket(request: Request, url: URL): Promise<Response> {
    const playerId = url.searchParams.get("playerId");
    if (!playerId) {
      return new Response("playerId required", { status: 400 });
    }
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    this.state.acceptWebSocket(server);
    // Reconnecting with same playerId takes back that slot (we never remove from playerOrder on disconnect)
    this.sockets.set(playerId, server);
    
    // Send authoritative player identity and game seed
    const myPlayerIndex = this.playerIdToIndex.get(playerId);
    if (myPlayerIndex !== undefined) {
      try {
        server.send(JSON.stringify({ 
          type: "identity", 
          myPlayerIndex,
          playerId,
          rngSeed: this.gameState.rngSeed
        }));
      } catch (_) {}

      // On reconnect, send stored terrain damage and worm state so client
      // can restore the game to its current state instead of resetting
      if (this.terrainDamageLog.length > 0) {
        try {
          server.send(JSON.stringify({
            type: "terrain_sync",
            log: this.terrainDamageLog,
          }));
        } catch (_) {}
      }
      if (this.lastWormState) {
        try {
          server.send(JSON.stringify(this.lastWormState));
        } catch (_) {}
      }
    }
    
    // Then send current game state
    this.broadcast({ type: "state", state: this.gameState });
    return new Response(null, { status: 101, webSocket: client });
  }

  private broadcast(msg: { type: string; [k: string]: unknown }): void {
    const data = JSON.stringify(msg);
    for (const ws of this.sockets.values()) {
      try {
        ws.send(data);
      } catch (_) {}
    }
  }

  private advanceTurn(): void {
    this.gameState.currentTurnIndex =
      (this.gameState.currentTurnIndex + 1) % this.gameState.playerOrder.length;
    this.gameState.phase = "aiming";
    this.gameState.turnEndTime = Date.now() + TURN_TIME_MS;
    this.broadcast({ type: "turn_advanced", turnIndex: this.gameState.currentTurnIndex });
    this.broadcast({ type: "state", state: this.gameState });
  }

  private getBotInput(): string {
    const idx = this.gameState.currentTurnIndex;
    const seed = this.gameState.rngSeed + idx * 1000 + this.gameState.inputLog.length;
    const angle = 30 + (seed % 120);
    const power = 50 + (seed % 41);
    return JSON.stringify({
      Fire: { weapon: "Bazooka", angle_deg: angle, power_percent: power },
    });
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
    const idx = this.playerIdToIndex.get(playerId);
    if (idx === undefined) return;

    // Accept terrain_damages from ANY connected player (not just current turn)
    // so the worker always has the latest cumulative damage log.
    try {
      const parsed = JSON.parse(data) as { type: string; [k: string]: unknown };
      if (parsed.type === "terrain_damages") {
        const dmgMsg = parsed as { type: string; log?: number[][] };
        if (Array.isArray(dmgMsg.log) && dmgMsg.log.length >= this.terrainDamageLog.length) {
          this.terrainDamageLog = dmgMsg.log;
        }
        return;
      }
    } catch (_) {}

    // All other message types require it to be the current turn player
    if (this.gameState.currentTurnIndex !== idx) return;

    const current = this.gameState.playerOrder[this.gameState.currentTurnIndex];
    if (current?.isBot) return;

    try {
      const msg = JSON.parse(data) as { type: string; input?: string; aim?: number };
        // Allow clients to request a restart which we broadcast to all clients
        if (msg.type === "restart" && typeof (msg as any).seed === "number") {
          const seed = (msg as any).seed as number;
          // Reset server-side minimal state for the new game
          this.gameState.rngSeed = seed;
          this.gameState.inputLog = [];
          this.gameState.currentTurnIndex = 0;
          this.gameState.phase = "aiming";
          this.gameState.turnEndTime = Date.now() + TURN_TIME_MS;
          this.broadcast({ type: "restart", seed });
          this.broadcast({ type: "state", state: this.gameState });
          return;
        }
      if (msg.type === "input" && typeof msg.input === "string") {
        // Check if this is a firing action (not movement)
        const isFiring = msg.input.includes('"Fire"');
        
        if (isFiring) {
          // Only log and change phase for firing actions
          this.gameState.inputLog.push(msg.input);
          this.gameState.phase = "projectile";
        }
        
        // Always broadcast the input to all clients (includes Walk, Jump, Fire)
        this.broadcast({ type: "input", input: msg.input, turnIndex: this.gameState.currentTurnIndex });
        
        if (isFiring) {
          this.broadcast({ type: "state", state: this.gameState });
        }
      } else if (msg.type === "aim" && typeof msg.aim === "number") {
        // Broadcast aim angle updates without changing game state
        this.broadcast({ type: "aim", aim: msg.aim, turnIndex: this.gameState.currentTurnIndex });
      } else if (msg.type === "worm_state") {
        // Store latest worm state so we can send it on reconnect
        this.lastWormState = msg as { type: string; [k: string]: unknown };
        // Relay worm state snapshot to all clients for position/health sync
        this.broadcast(msg as { type: string; [k: string]: unknown });
      } else if (msg.type === "end_turn") {
        this.advanceTurn();
        this.maybeBotTurn();
      }
    } catch (_) {}
  }

  private maybeBotTurn(): void {
    const current = this.gameState.playerOrder[this.gameState.currentTurnIndex];
    if (current?.isBot) {
      const input = this.getBotInput();
      this.gameState.inputLog.push(input);
      this.broadcast({ type: "input", input, turnIndex: this.gameState.currentTurnIndex });
      this.gameState.phase = "projectile";
      this.broadcast({ type: "state", state: this.gameState });
      setTimeout(() => this.advanceTurnAndMaybeBot(), 1500);
    }
  }

  private advanceTurnAndMaybeBot(): void {
    this.advanceTurn();
    this.maybeBotTurn();
  }

  async webSocketClose(_ws: WebSocket): Promise<void> {
    let pid: string | null = null;
    for (const [id, s] of this.sockets) {
      if (s === _ws) {
        pid = id;
        break;
      }
    }
    if (pid) this.sockets.delete(pid);
  }
}
