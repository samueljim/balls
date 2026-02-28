import type { GameState } from "./types";

const TURN_TIME_MS = 45_000;
/** Max retreat phase duration on server (client gets 5 s; +3 s covers network lag and settling animation) */
const RETREAT_TIME_MS = 8_000;

interface BallSnapshot {
  x: number; y: number; vx: number; vy: number; hp: number; alive: boolean;
}

interface PersistedGameData {
  gameState: GameState;
  terrainDamageLog: number[][];
  ballSnapshots: BallSnapshot[];
  playerIdToIndex: [string, number][];
  phaseStartTime: number;
}
/** Grace period after turnEndTime before the server forcibly advances the turn */
const WATCHDOG_GRACE_MS = 5_000;
/** Max time (ms) a "projectile" phase can last before the server force-advances */
const PROJECTILE_TIMEOUT_MS = 20_000;
/** How long (ms) to wait before force-advancing after the current-turn player disconnects */
const DISCONNECT_ADVANCE_MS = 12_000;

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
  /** Set of playerIds whose WebSocket is currently open */
  private connectedPlayerIds: Set<string> = new Set();
  /** Accumulated terrain damage events [[cx,cy,r], ...] for replay on reconnect */
  private terrainDamageLog: number[][] = [];
  /** Latest per-ball snapshot (positions + health) for reconnect sync */
  private ballSnapshots: BallSnapshot[] = [];
  /** Timestamp (ms) when the current phase last changed – used by watchdog */
  private phaseStartTime: number = 0;

  constructor(state: DurableObjectState, _env: unknown) {
    this.state = state;
    // Restore persisted state so the game survives DO hibernation / eviction.
    this.state.blockConcurrencyWhile(async () => {
      try {
        const saved = await this.state.storage.get<PersistedGameData>("gameData");
        if (saved) {
          this.gameState = saved.gameState;
          this.terrainDamageLog = saved.terrainDamageLog ?? [];
          this.ballSnapshots = saved.ballSnapshots ?? [];
          this.phaseStartTime = saved.phaseStartTime ?? 0;
          this.playerIdToIndex = new Map(saved.playerIdToIndex ?? []);
        }
      } catch (_) {}
    });
  }

  /** Persist critical game state to DO storage so it survives hibernation. */
  private persistState(): void {
    void this.state.storage.put<PersistedGameData>("gameData", {
      gameState: this.gameState,
      terrainDamageLog: this.terrainDamageLog,
      ballSnapshots: this.ballSnapshots,
      playerIdToIndex: [...this.playerIdToIndex.entries()],
      phaseStartTime: this.phaseStartTime,
    }).catch(() => {});
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
    // Initialise blank ball snapshots — will be filled once ball_state arrives
    const ballsPerTeam = 3;
    const totalBalls = (body.playerOrder ?? []).length * ballsPerTeam;
    this.ballSnapshots = Array.from({ length: totalBalls }, () => ({
      x: 0, y: 0, vx: 0, vy: 0, hp: 100, alive: true,
    }));
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
    this.phaseStartTime = Date.now();
    this.scheduleWatchdog();
    this.persistState();
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
    this.connectedPlayerIds.add(playerId);

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

      // On reconnect, always send terrain_sync first (even if empty) to guarantee
      // it arrives before game_resync on the client and ordering is deterministic.
      try {
        server.send(JSON.stringify({
          type: "terrain_sync",
          log: this.terrainDamageLog,
        }));
      } catch (_) {}
      // Send game_resync: full snapshot including phase and turn timer remaining.
      // Only include ball positions once the game has actually progressed and the
      // server has received real positions from the active player.  On a fresh game
      // start, ballSnapshots are all (0,0), and including them would overwrite the
      // deterministic spawn positions the client already computed from the seed.
      const turnTimeRemainingMs = Math.max(0, this.gameState.turnEndTime - Date.now());
      const gameHasProgressed = this.gameState.inputLog.length > 0 || this.gameState.currentTurnIndex > 0;
      try {
        server.send(JSON.stringify({
          type: "game_resync",
          phase: this.gameState.phase,
          currentTurnIndex: this.gameState.currentTurnIndex,
          turnTimeRemainingMs,
          // Only ship authoritative ball data once we have real positions from clients
          balls: gameHasProgressed ? this.ballSnapshots : undefined,
        }));
      } catch (_) {}

      // Notify all other connected players that this player is back online
      this.broadcastExcept(playerId, { type: "player_connected", playerIndex: myPlayerIndex });
    }
    
    // Send current game state to all (including the reconnecting client)
    this.broadcast({ type: "state", state: this.gameState });
    return new Response(null, { status: 101, webSocket: client });
  }

  private broadcast(msg: { type: string; [k: string]: unknown }): void {
    // Inject a relative turnTimeRemainingMs alongside any absolute turnEndTime so
    // WASM clients don't need wall-clock math to compute the remaining time.
    if (msg.type === "state") {
      const state = msg.state as Partial<GameState> | undefined;
      if (state && typeof state.turnEndTime === "number") {
        (msg as Record<string, unknown>).turnTimeRemainingMs = Math.max(0, state.turnEndTime - Date.now());
      }
    }
    const data = JSON.stringify(msg);
    for (const ws of this.sockets.values()) {
      try {
        ws.send(data);
      } catch (_) {}
    }
  }

  /** Send a message to all connected players except the one with the given playerId. */
  private broadcastExcept(excludePlayerId: string, msg: { type: string; [k: string]: unknown }): void {
    const data = JSON.stringify(msg);
    for (const [pid, ws] of this.sockets) {
      if (pid === excludePlayerId) continue;
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
    this.phaseStartTime = Date.now();
    // Include authoritative ball positions and terrain log at each turn boundary
    // so all live clients can reconcile any divergence before the next shot.
    this.broadcast({
      type: "turn_advanced",
      turnIndex: this.gameState.currentTurnIndex,
      balls: this.ballSnapshots,
      terrainLog: this.terrainDamageLog,
    });
    this.broadcast({ type: "state", state: this.gameState });
    this.scheduleWatchdog();
    this.persistState();
  }

  /** Schedule a Cloudflare DO alarm to fire when the current turn/phase should time out.
   *  Silently ignored in environments that don't support alarms (local dev). */
  private scheduleWatchdog(): void {
    const deadline =
      this.gameState.phase === "projectile"
        ? this.phaseStartTime + PROJECTILE_TIMEOUT_MS
        : this.gameState.phase === "retreat"
        ? this.phaseStartTime + RETREAT_TIME_MS
        : this.gameState.turnEndTime + WATCHDOG_GRACE_MS;
    try {
      this.state.storage.setAlarm(deadline);
    } catch (_) {
      // setAlarm may not be available in all environments — fail silently
    }
  }

  /** Cloudflare DO alarm handler — fires when a scheduled watchdog deadline hits.
   *  Forces the game forward if it has stalled (frozen client, disconnected player, etc.) */
  async alarm(): Promise<void> {
    const now = Date.now();

    if (this.gameState.playerOrder.length === 0) return; // Game not started

    if (this.gameState.phase === "projectile") {
      if (now >= this.phaseStartTime + PROJECTILE_TIMEOUT_MS) {
        // Projectile phase has been stuck too long — force-advance
        this.broadcast({ type: "force_advance", reason: "projectile_timeout" });
        this.advanceTurnAndMaybeBot();
      } else {
        // Not yet expired — re-arm
        this.scheduleWatchdog();
      }
      return;
    }

    if (this.gameState.phase === "retreat") {
      if (now >= this.phaseStartTime + RETREAT_TIME_MS) {
        // Retreat phase timed out — force-advance
        this.broadcast({ type: "force_advance", reason: "retreat_timeout" });
        this.advanceTurnAndMaybeBot();
      } else {
        this.scheduleWatchdog();
      }
      return;
    }

    // Aiming phase: check whether the active player is currently disconnected.
    // If so, skip their turn immediately rather than waiting for the full grace period.
    // activePid being undefined means currentTurnIndex is out of range (corrupted state)
    // — treat as disconnected so the game advances rather than stalling.
    const activePid = this.gameState.playerOrder[this.gameState.currentTurnIndex]?.playerId;
    const activeIsConnected = activePid !== undefined && this.connectedPlayerIds.has(activePid);
    if (!activeIsConnected) {
      this.broadcast({ type: "force_advance", reason: "player_disconnected" });
      this.advanceTurnAndMaybeBot();
      return;
    }

    if (now >= this.gameState.turnEndTime + WATCHDOG_GRACE_MS) {
      // Turn timer expired and no end_turn ever arrived — force-advance
      this.broadcast({ type: "force_advance", reason: "turn_timeout" });
      this.advanceTurnAndMaybeBot();
      return;
    }

    // Turn hasn't expired yet (e.g. alarm fired early) — re-arm for when it should
    this.scheduleWatchdog();
  }

  // ─── Bot AI helpers ──────────────────────────────────────────────────────────

  /** Simulate a bazooka projectile and return the y position when it crosses targetX.
   *  Returns null if the projectile never reaches targetX within the sim budget. */
  private simYAtX(
    sx: number, sy: number,
    angleDeg: number, power: number,
    targetX: number,
  ): number | null {
    const angle = (angleDeg * Math.PI) / 180;
    const speed = power * 12.0;
    const g = 480.0;
    const dt = 0.04;
    let vx = Math.cos(angle) * speed;
    let vy = Math.sin(angle) * speed;
    let x = sx;
    let y = sy;
    let prevX = x;

    for (let step = 0; step < 400; step++) {
      vx *= 0.99; // bazooka air resistance
      vy += g * dt;
      prevX = x;
      x += vx * dt;
      y += vy * dt;

      const crossed =
        (prevX <= targetX && x >= targetX) ||
        (prevX >= targetX && x <= targetX);
      if (crossed) {
        const frac = Math.abs(targetX - prevX) / Math.max(Math.abs(x - prevX), 0.001);
        return y - (1 - frac) * vy * dt;
      }
      if (y > 2500) break; // fell off map
    }
    return null;
  }

  /** LCG pseudo-random [0,1) seeded by current game state so bots are deterministic
   *  but vary shot to shot. */
  private botRand(): number {
    const s =
      (this.gameState.rngSeed ^
        (this.gameState.currentTurnIndex * 1664525 + 1013904223) ^
        (this.gameState.inputLog.length * 22695477 + 1)) >>>
      0;
    return ((s * 1664525 + 1013904223) >>> 0) / 0x100000000;
  }

  /** Return the best { angleDeg, power } to hit (tx, ty) from (sx, sy), or null if
   *  nothing viable was found (caller can fall back to random). */
  private aimAt(
    sx: number, sy: number,
    tx: number, ty: number,
  ): { angleDeg: number; power: number } | null {
    const dx = tx - sx;

    // Base direction angle (radians) pointing straight at the target
    const baseRad = Math.atan2(ty - sy, dx);

    let bestScore = Infinity;
    let bestAngleDeg = 0;
    let bestPower = 70;

    // Sweep angle offsets (degrees relative to base direction) from -55° to +5°
    // (negative = aiming higher than the direct line → compensates for gravity)
    const powers = [55, 65, 75, 85, 45, 35];
    for (const power of powers) {
      for (let delta = -55; delta <= 10; delta += 2) {
        const aRad = baseRad + (delta * Math.PI) / 180;
        const angleDeg = aRad * (180 / Math.PI);
        const yAtTarget = this.simYAtX(sx, sy, angleDeg, power, tx);
        if (yAtTarget === null) continue;
        const score = Math.abs(yAtTarget - ty);
        if (score < bestScore) {
          bestScore = score;
          bestAngleDeg = angleDeg;
          bestPower = power;
        }
      }
    }

    if (bestScore > 200) return null; // Can't get close enough — give up
    return { angleDeg: bestAngleDeg, power: bestPower };
  }

  /** Work out a complete bot action plan: optional walk steps + a fire input. */
  private getBotActions(): {
    walkDir: number;
    walkSteps: number;
    fireInput: string;
  } {
    const idx = this.gameState.currentTurnIndex;
    const numPlayers = this.gameState.playerOrder.length;
    const ballsPerTeam = 3;

    interface BallData { x: number; y: number; hp: number; alive: boolean }
    const balls: BallData[] = [];

    // Pull ball positions from persisted snapshots
    const snapshots = this.ballSnapshots;
    if (snapshots.length > 0) {
      for (const b of snapshots) {
        balls.push({ x: b.x, y: b.y, hp: b.hp, alive: b.alive });
      }
    }

    const fallback = (): { walkDir: number; walkSteps: number; fireInput: string } => {
      const r = this.botRand();
      const angle = (r * 120) - 60; // -60..60 deg
      const power = 45 + Math.floor(r * 45);
      return {
        walkDir: 0,
        walkSteps: 0,
        fireInput: JSON.stringify({ Fire: { weapon: "Bazooka", angle_deg: angle, power_percent: power } }),
      };
    };

    if (balls.length === 0) return fallback();

    // The bot's own ball indices follow the interleaved spawn pattern:
    // team t has balls at [t, t+numPlayers, t+numPlayers*2]
    const botBallSet = new Set<number>();
    for (let wi = 0; wi < ballsPerTeam; wi++) {
      const i = idx + wi * numPlayers;
      if (i < balls.length) botBallSet.add(i);
    }

    // Pick first alive bot ball as shooter
    let shooter: (BallData & { index: number }) | null = null;
    for (const i of botBallSet) {
      if (balls[i].alive) { shooter = { ...balls[i], index: i }; break; }
    }
    if (!shooter) return fallback();

    // Collect alive enemy balls
    const enemies = balls
      .map((b, i) => ({ ...b, index: i }))
      .filter(b => b.alive && !botBallSet.has(b.index));
    if (enemies.length === 0) return fallback();

    // Sort enemies: prioritise low-HP ones nearby, otherwise nearest
    const sx = shooter.x, sy = shooter.y;
    enemies.sort((a, b) => {
      const dA = Math.hypot(a.x - sx, a.y - sy);
      const dB = Math.hypot(b.x - sx, b.y - sy);
      if (a.hp <= 30 && dA < 500) return -1;
      if (b.hp <= 30 && dB < 500) return 1;
      return dA - dB;
    });

    const target = enemies[0];
    const dx = target.x - sx;

    // Decide whether to walk toward the target first (if they're very far away)
    const dist = Math.abs(dx);
    const walkDir = dx > 0 ? 1 : -1;
    // Walk 1–4 steps when target is far; 0 when close or almost in range already
    const walkSteps = dist > 500 ? 4 : dist > 300 ? 2 : dist > 150 ? 1 : 0;

    // Estimate shooter position after walking (rough: ~22 px per walk step)
    const approxShooterX = sx + walkDir * walkSteps * 22;
    const aim = this.aimAt(approxShooterX, sy, target.x, target.y);

    let angleDeg: number;
    let power: number;

    if (aim) {
      // Add some inaccuracy so the bot isn't always perfect
      const wobble = (this.botRand() - 0.5) * 14; // ±7 degrees
      angleDeg = aim.angleDeg + wobble;
      power = aim.power;
    } else {
      // Fall back: aim roughly in the direction of the target
      const rough = Math.atan2(target.y - sy - 100, dx) * (180 / Math.PI);
      angleDeg = rough + (this.botRand() - 0.5) * 20;
      power = 55 + Math.floor(this.botRand() * 30);
    }

    return {
      walkDir,
      walkSteps,
      fireInput: JSON.stringify({ Fire: { weapon: "Bazooka", angle_deg: angleDeg, power_percent: power } }),
    };
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

    // Accept terrain_damages only from the current-turn player — they are the only
    // source of authoritative terrain changes this turn.  Messages sent just before
    // end_turn are still valid because WebSocket ordering within one connection
    // guarantees terrain_damages arrives before end_turn on the server.
    try {
      const parsed = JSON.parse(data) as { type: string; [k: string]: unknown };
      if (parsed.type === "terrain_damages") {
        if (idx !== this.gameState.currentTurnIndex) return;
        const dmgMsg = parsed as { type: string; log?: number[][] };
        if (Array.isArray(dmgMsg.log) && dmgMsg.log.length >= this.terrainDamageLog.length) {
          this.terrainDamageLog = dmgMsg.log;
          this.persistState();
        }
        return;
      }
      // pos_update is a real-time position stream — relay from the owning player so
      // all clients can smoothly interpolate remote balls.
      // Ownership: in the interleaved spawn layout ball index `bi` belongs to
      // player `bi % numPlayers` — only accept updates the sender actually owns.
      if (parsed.type === "pos_update") {
        const pu = parsed as { bi?: number; x?: number; y?: number; vx?: number; vy?: number };
        const bi = pu.bi;
        const numPlayers = this.gameState.playerOrder.length;
        // Reject all pos_updates when playerOrder is not set (game not yet started)
        // or when the ball does not belong to the sending player.
        if (
          typeof bi === "number" &&
          bi >= 0 &&
          bi < this.ballSnapshots.length &&
          numPlayers > 0 &&
          bi % numPlayers === idx
        ) {
          const snap = this.ballSnapshots[bi];
          if (typeof pu.x === "number") snap.x = pu.x;
          if (typeof pu.y === "number") snap.y = pu.y;
          if (typeof pu.vx === "number") snap.vx = pu.vx;
          if (typeof pu.vy === "number") snap.vy = pu.vy;
          this.broadcast(parsed as { type: string; [k: string]: unknown });
        }
        return;
      }
    } catch (_) {}

    // Restart can be sent by any connected player (e.g., from the game-over screen).
    // Handle it before the current-turn guard so it is never silently dropped.
    try {
      const restartMsg = JSON.parse(data) as { type: string; seed?: number };
      if (restartMsg.type === "restart" && typeof restartMsg.seed === "number") {
        const seed = restartMsg.seed;
        this.gameState.rngSeed = seed;
        this.gameState.inputLog = [];
        this.gameState.currentTurnIndex = 0;
        this.gameState.phase = "aiming";
        this.gameState.turnEndTime = Date.now() + TURN_TIME_MS;
        this.phaseStartTime = Date.now();
        this.ballSnapshots = [];
        this.terrainDamageLog = [];
        this.broadcast({ type: "restart", seed });
        this.broadcast({ type: "state", state: this.gameState });
        this.scheduleWatchdog();
        this.persistState();
        return;
      }
    } catch (_) {}

    // All other message types require it to be the current turn player
    if (this.gameState.currentTurnIndex !== idx) return;

    const current = this.gameState.playerOrder[this.gameState.currentTurnIndex];
    if (current?.isBot) return;

    try {
      const msg = JSON.parse(data) as { type: string; input?: string; aim?: number };
      if (msg.type === "input" && typeof msg.input === "string") {
        // Check if this is a firing action (not movement)
        const isFiring = msg.input.includes('"Fire"');
        
        if (isFiring) {
          // Only log and change phase for firing actions
          this.gameState.inputLog.push(msg.input);
          this.gameState.phase = "projectile";
          this.phaseStartTime = Date.now();
          this.scheduleWatchdog();
          this.persistState();
        }
        
        // Always broadcast the input to all clients (Jump, Backflip, Fire)
        // Walk movement is no longer relayed via inputs — pos_update handles position sync.
        this.broadcast({ type: "input", input: msg.input, turnIndex: this.gameState.currentTurnIndex });
        
        if (isFiring) {
          this.broadcast({ type: "state", state: this.gameState });
        }
      } else if (msg.type === "aim" && typeof msg.aim === "number") {
        // Broadcast aim angle updates without changing game state
        this.broadcast({ type: "aim", aim: msg.aim, turnIndex: this.gameState.currentTurnIndex });
      } else if (msg.type === "ball_state") {
        // Update per-ball snapshots (health + alive + positions) from active player
        const bs = msg as { balls?: Array<{x?: number; y?: number; vx?: number; vy?: number; hp?: number; alive?: boolean}> };
        if (Array.isArray(bs.balls)) {
          bs.balls.forEach((b, i) => {
            if (i < this.ballSnapshots.length) {
              const s = this.ballSnapshots[i];
              if (typeof b.x === "number") s.x = b.x;
              if (typeof b.y === "number") s.y = b.y;
              if (typeof b.vx === "number") s.vx = b.vx;
              if (typeof b.vy === "number") s.vy = b.vy;
              if (typeof b.hp === "number") s.hp = b.hp;
              if (typeof b.alive === "boolean") s.alive = b.alive;
            }
          });
        }
        // Relay to other clients for position/health sync
        this.broadcast(msg as { type: string; [k: string]: unknown });
        this.persistState();
      } else if (msg.type === "retreat_start") {
        // Active player entered the post-fire retreat window.
        // Switch to retreat phase so the watchdog uses the retreat timeout
        // instead of the projectile timeout.  Only valid after a fire action
        // (server must already be in "projectile" phase).
        if (this.gameState.phase === "projectile") {
          this.gameState.phase = "retreat";
          this.phaseStartTime = Date.now();
          this.scheduleWatchdog();
          this.persistState();
        }
      } else if (msg.type === "end_turn") {
        this.advanceTurn();
        this.maybeBotTurn();
      }
    } catch (_) {}
  }

  private maybeBotTurn(): void {
    const current = this.gameState.playerOrder[this.gameState.currentTurnIndex];
    if (!current?.isBot) return;

    const turnIndex = this.gameState.currentTurnIndex;
    const { walkDir, walkSteps, fireInput } = this.getBotActions();

    // Send an aim-angle preview so other clients see the bot "aiming"
    // (angle extracted from the fire input so it matches what will be fired)
    let previewAngleRad = 0;
    try {
      const parsed = JSON.parse(fireInput) as { Fire?: { angle_deg?: number } };
      previewAngleRad = ((parsed.Fire?.angle_deg ?? 0) * Math.PI) / 180;
    } catch (_) {}
    this.broadcast({ type: "aim", aim: previewAngleRad, turnIndex });

    let stepIdx = 0;
    const doStep = (): void => {
      // Guard: ensure it's still this bot's turn
      if (this.gameState.currentTurnIndex !== turnIndex) return;

      if (stepIdx < walkSteps) {
        const walkInput = walkDir > 0
          ? '{"Walk":{"dir":1.0}}'
          : '{"Walk":{"dir":-1.0}}';
        this.broadcast({ type: "input", input: walkInput, turnIndex });
        stepIdx++;
        setTimeout(doStep, 180);
      } else {
        // Fire — guard against stale turns before committing the action
        if (this.gameState.currentTurnIndex !== turnIndex) return;
        this.gameState.inputLog.push(fireInput);
        this.broadcast({ type: "input", input: fireInput, turnIndex });
        this.gameState.phase = "projectile";
        this.phaseStartTime = Date.now();
        this.scheduleWatchdog();
        this.broadcast({ type: "state", state: this.gameState });
        setTimeout(() => {
          // This check is intentionally separate from the synchronous guard above:
          // the alarm watchdog (projectile_timeout) may fire within the 1500 ms window
          // and call advanceTurnAndMaybeBot() independently.  Without this guard the
          // bot's delayed callback would fire a second advance and skip an extra turn.
          if (this.gameState.currentTurnIndex === turnIndex) {
            this.advanceTurnAndMaybeBot();
          }
        }, 1500);
      }
    };

    // Small initial "thinking" delay before the bot starts moving
    setTimeout(doStep, 600);
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
    if (pid) {
      this.sockets.delete(pid);
      this.connectedPlayerIds.delete(pid);

      const playerIndex = this.playerIdToIndex.get(pid);
      if (playerIndex !== undefined) {
        // Notify remaining players that someone went offline
        this.broadcast({ type: "player_disconnected", playerIndex });

        // If the disconnected player is the active turn player during the aiming phase,
        // schedule a short watchdog so the game doesn't stall for the full turn timer.
        // We use DISCONNECT_ADVANCE_MS (12 s) giving them time to reconnect on a flaky
        // connection before force-advancing — the regular watchdog (50 s) is too long.
        const isActiveTurnPlayer =
          playerIndex === this.gameState.currentTurnIndex &&
          (this.gameState.phase === "aiming");
        if (isActiveTurnPlayer && this.gameState.playerOrder.length > 0) {
          try {
            this.state.storage.setAlarm(Date.now() + DISCONNECT_ADVANCE_MS);
          } catch (_) {}
        }
      }
    }
  }
}
