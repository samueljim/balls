import type { GameState } from "./types";

const TURN_TIME_MS = 45_000;

interface BallSnapshot {
  x: number; y: number; vx: number; vy: number; hp: number; alive: boolean;
}

interface PersistedGameData {
  gameState: GameState;
  terrainDamageLog: number[][];
  ballSnapshots: BallSnapshot[];
  playerIdToIndex: [string, number][];
  phaseStartTime: number;
  /** Per-team last ball index used (round-robin), matches client last_ball_per_team */
  lastBallPerTeam?: (number | undefined)[];
  hostId?: string | null;
}
/** Grace period after turnEndTime before the server forcibly advances the turn */
const WATCHDOG_GRACE_MS = 1_000;
/** Max time (ms) a "projectile" phase can last before the server force-advances */
const PROJECTILE_TIMEOUT_MS = 12_000;
/** Max WebSocket message size (chars/bytes) to prevent memory exhaustion */
const MAX_MESSAGE_SIZE = 100_000;
/** Minimum ms between pos_update relays per player (20/sec cap) */
const POS_UPDATE_INTERVAL_MS = 50;

export class Game implements DurableObject {
  private state: DurableObjectState;
  private gameState: GameState = {
    playerOrder: [],
    inputLog: [],
    currentTurnIndex: 0,
    currentBallIndex: 0,
    turnEndTime: 0,
    phase: "aiming",
    rngSeed: 0, // Set by lobby via /init POST
    terrainId: 0,
    globalTurnNumber: 0,
  };
  private playerIdToIndex: Map<string, number> = new Map();
  /** Accumulated terrain damage events [[cx,cy,r], ...] for replay on reconnect */
  private terrainDamageLog: number[][] = [];
  /** Latest per-ball snapshot (positions + health) for reconnect sync */
  private ballSnapshots: BallSnapshot[] = [];
  /** Wind value from the active player at end-of-turn, forwarded in turn_advanced */
  private lastWindValue: number = 0;
  /** Timestamp (ms) when the current phase last changed – used by watchdog */
  private phaseStartTime: number = 0;
  /** Per-team last ball index used for round-robin (aligns with client last_ball_per_team) */
  private lastBallPerTeam: (number | undefined)[] = [];
  /** Host player id (only host can request restart) */
  private hostId: string | null = null;
  /** Per-player last ping timestamp (rate-limit: 1 force-advance per 2s per player) */
  private lastPingPerPlayer: Map<string, number> = new Map();
  /** Per-player last input timestamp (rate-limit: 25/sec) */
  private lastInputPerPlayer: Map<string, number> = new Map();
  /** Per-player last aim timestamp (rate-limit: 30/sec) */
  private lastAimPerPlayer: Map<string, number> = new Map();
  /** Per-player last pos_update timestamp (rate-limit: 20/sec) */
  private lastPosUpdatePerPlayer: Map<string, number> = new Map();
  /** When we last received input/ball_state/end_turn from the active player (for terrain fallback) */
  private lastActivePlayerActivity: number = 0;

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
          this.lastBallPerTeam = saved.lastBallPerTeam ?? [];
          this.hostId = saved.hostId ?? null;
        }
      } catch (e) {
        console.warn("[Game] Failed to restore persisted state:", e);
      }
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
      lastBallPerTeam: this.lastBallPerTeam,
      hostId: this.hostId,
    }).catch((e) => {
      console.warn("[Game] Failed to persist state:", e);
    });
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
    const body = await request.json().catch((e) => {
      console.warn("[Game] Invalid init JSON:", e);
      return {};
    }) as {
      playerOrder?: { playerId: string; isBot: boolean; name: string }[];
      rngSeed?: number;
      terrainId?: number;
      hostId?: string;
    };
    this.gameState.playerOrder = body.playerOrder ?? [];
    // Use seed from lobby (always provided via start_game)
    this.gameState.rngSeed = body.rngSeed ?? Math.floor(Math.random() * 0xFFFFFFFF);
    this.gameState.terrainId = body.terrainId ?? 0;
    this.gameState.inputLog = [];
    this.gameState.currentTurnIndex = 0;
    this.gameState.currentBallIndex = 0;
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
    this.lastBallPerTeam = Array.from({ length: this.gameState.playerOrder.length }, () => undefined);
    if (typeof body.hostId === "string") this.hostId = body.hostId;
    // Send identity to all already-connected sockets (they connected before /init was called)
    for (const ws of this.state.getWebSockets()) {
      const att = ws.deserializeAttachment() as { playerId: string } | null;
      const pid = att?.playerId;
      if (!pid) continue;
      const idx = this.playerIdToIndex.get(pid);
      if (idx !== undefined) {
        try {
          ws.send(JSON.stringify({
            type: "identity",
            myPlayerIndex: idx,
            playerId: pid,
            rngSeed: this.gameState.rngSeed,
          }));
        } catch (e) {
          console.warn("[Game] Failed to send identity to", pid, e);
        }
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

  /** Validate playerId format: reasonable length, no control chars (security) */
  private isValidPlayerId(id: string): boolean {
    if (typeof id !== "string" || id.length < 1 || id.length > 64) return false;
    for (let i = 0; i < id.length; i++) {
      const c = id.charCodeAt(i);
      if (c < 32 || c > 126) return false; // No control chars, only printable ASCII
    }
    return true;
  }

  private async handleWebSocket(request: Request, url: URL): Promise<Response> {
    const playerId = url.searchParams.get("playerId");
    if (!playerId) {
      return new Response("playerId required", { status: 400 });
    }
    if (!this.isValidPlayerId(playerId)) {
      return new Response("Invalid playerId format", { status: 400 });
    }
    if (this.gameState.playerOrder.length > 0) {
      const inGame = this.gameState.playerOrder.some((p) => p.playerId === playerId);
      if (!inGame) {
        return new Response("playerId not in game", { status: 400 });
      }
    }
    const pair = new WebSocketPair();
    const [client, server] = Object.values(pair);
    this.state.acceptWebSocket(server);
    server.serializeAttachment({ playerId });
    // Reconnecting with same playerId takes back that slot (we never remove from playerOrder on disconnect)
    
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
      } catch (e) {
        console.warn("[Game] Failed to send identity on connect:", e);
      }

      // On reconnect, send terrain damage log and a comprehensive resync message
      // so the client can fully restore game state without a reset.
      if (this.terrainDamageLog.length > 0) {
        try {
          server.send(JSON.stringify({
            type: "terrain_sync",
            log: this.terrainDamageLog,
          }));
        } catch (e) {
          console.warn("[Game] Failed to send terrain_sync on reconnect:", e);
        }
      }
      // Send game_resync: full snapshot including phase and turn timer remaining.
      // Only include ball positions once the game has actually progressed and the
      // server has received real positions from the active player.  On a fresh game
      // start, ballSnapshots are all (0,0), and including them would overwrite the
      // deterministic spawn positions the client already computed from the seed.
      const turnTimeRemainingMs = Math.max(0, this.gameState.turnEndTime - Date.now());
      const gameHasProgressed = this.gameState.inputLog.length > 0 || (this.gameState.globalTurnNumber ?? 0) > 0;
      try {
        server.send(JSON.stringify({
          type: "game_resync",
          phase: this.gameState.phase,
          currentTurnIndex: this.gameState.currentTurnIndex,
          currentBallIndex: this.gameState.currentBallIndex,
          turnTimeRemainingMs,
          globalTurnNumber: this.gameState.globalTurnNumber ?? 0,
          // Only ship authoritative ball data once we have real positions from clients
          balls: gameHasProgressed ? this.ballSnapshots : undefined,
        }));
      } catch (e) {
        console.warn("[Game] Failed to send game_resync on reconnect:", e);
      }
    }
    
    // Then send current game state
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
    for (const ws of this.state.getWebSockets()) {
      try {
        ws.send(data);
      } catch (e) {
        const att = ws.deserializeAttachment() as { playerId: string } | null;
        console.warn("[Game] Broadcast failed for", att?.playerId, e);
      }
    }
  }

  /** Check if the given team (player index) has any alive balls. */
  private teamHasAliveBalls(teamIndex: number): boolean {
    const numPlayers = this.gameState.playerOrder.length;
    const ballsPerTeam = 3;
    for (let wi = 0; wi < ballsPerTeam; wi++) {
      const bi = teamIndex + wi * numPlayers;
      if (bi < this.ballSnapshots.length && this.ballSnapshots[bi].alive) {
        return true;
      }
    }
    return false;
  }

  /** Return the team (player) index that owns the given flat ball index.
   *  Ball layout: bi = teamIndex + wormSlot * numPlayers, so teamIndex = bi % numPlayers. */
  private ballTeamIndex(bi: number): number {
    const numPlayers = this.gameState.playerOrder.length;
    return numPlayers > 0 ? bi % numPlayers : 0;
  }

  /** Count teams with at least one alive ball. Returns winner team index (0-based) or -1 if draw/none. */
  private getAliveTeams(): { count: number; winner: number } {
    const numPlayers = this.gameState.playerOrder.length;
    let count = 0;
    let lastAlive = -1;
    for (let t = 0; t < numPlayers; t++) {
      if (this.teamHasAliveBalls(t)) {
        count++;
        lastAlive = t;
      }
    }
    return { count, winner: lastAlive };
  }

  private advanceTurn(): void {
    const numPlayers = this.gameState.playerOrder.length;
    if (numPlayers === 0) return;

    // Check game over before advancing — if we're already down to 0–1 teams, broadcast and stop
    const { count: aliveCount, winner } = this.getAliveTeams();
    if (aliveCount <= 1) {
      const winnerName = winner >= 0
        ? this.gameState.playerOrder[winner]?.name ?? `Team ${winner + 1}`
        : "Nobody";
      this.broadcast({ type: "game_over", winner: winnerName });
      this.gameState.phase = "game_over";
      this.persistState();
      // Schedule storage cleanup 60s from now (clients have time to see the result)
      try { this.state.storage.setAlarm(Date.now() + 60_000); } catch (_) {}
      return;
    }

    // Increment global monotonic counter and skip dead teams
    const maxAttempts = numPlayers + 1; // avoid infinite loop
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
      this.gameState.globalTurnNumber = (this.gameState.globalTurnNumber ?? 0) + 1;
      this.gameState.currentTurnIndex =
        this.gameState.globalTurnNumber % numPlayers;

      if (this.teamHasAliveBalls(this.gameState.currentTurnIndex)) {
        break;
      }
    }

    // Safety: after skipping dead teams, re-check if the chosen team is actually alive.
    // This handles the case where ball-state updates caused more teams to die between
    // the pre-loop check and now (e.g. simultaneous kills in the same explosion).
    if (!this.teamHasAliveBalls(this.gameState.currentTurnIndex)) {
      const { winner: recheckWinner } = this.getAliveTeams();
      const winnerName = recheckWinner >= 0
        ? this.gameState.playerOrder[recheckWinner]?.name ?? `Team ${recheckWinner + 1}`
        : "Nobody";
      this.broadcast({ type: "game_over", winner: winnerName });
      this.gameState.phase = "game_over";
      this.persistState();
      // Schedule storage cleanup 60s from now (clients have time to see the result)
      try { this.state.storage.setAlarm(Date.now() + 60_000); } catch (_) {}
      return;
    }

    this.gameState.currentBallIndex = 0; // Will be set by first input of the new turn
    this.gameState.phase = "aiming";
    this.gameState.turnEndTime = Date.now() + TURN_TIME_MS;
    this.phaseStartTime = Date.now();
    this.lastActivePlayerActivity = Date.now(); // Reset for terrain fallback (allow non-active after 8s silence)

    // Include authoritative ball snapshots, wind, and terrain so all clients hard-sync
    // before starting the new turn, correcting any physics divergence from the previous turn.
    // Always include balls so all clients — including those whose turn was skipped — get the
    // latest alive/health state and can correctly detect end-of-game conditions.
    const includeBalls = this.ballSnapshots.length > 0;

    this.broadcast({
      type: "turn_advanced",
      turnIndex: this.gameState.currentTurnIndex,
      turnNumber: this.gameState.globalTurnNumber,
      balls: includeBalls ? this.ballSnapshots : undefined,
      wind: this.lastWindValue,
      terrain: this.terrainDamageLog.length > 0 ? this.terrainDamageLog : undefined,
    });
    this.broadcast({ type: "state", state: this.gameState });
    this.scheduleWatchdog();
    this.persistState();
  }

  /** Schedule a Cloudflare DO alarm to fire when the current turn/phase should time out,
   *  or in 1 second for the heartbeat tick — whichever is sooner.
   *  Silently ignored in environments that don't support alarms (local dev). */
  private scheduleWatchdog(): void {
    if (this.gameState.playerOrder.length === 0 || this.gameState.phase === "game_over") return;
    const deadline =
      this.gameState.phase === "projectile"
        ? this.phaseStartTime + PROJECTILE_TIMEOUT_MS
        : this.gameState.turnEndTime + WATCHDOG_GRACE_MS;
    // Fire at most every 5 seconds (heartbeat) or at the watchdog deadline, whichever is sooner.
    // This lets the DO hibernate between ticks instead of keeping a live setTimeout.
    const nextAlarm = Math.min(deadline, Date.now() + 5_000);
    try {
      this.state.storage.setAlarm(nextAlarm);
    } catch (_) {
      // setAlarm may not be available in all environments — fail silently
    }
  }

  /** Cloudflare DO alarm handler — fires when a scheduled watchdog deadline hits or
   *  every ~5 seconds as a turn-authority heartbeat (whichever is sooner).
   *  Forces the game forward if it has stalled (frozen client, disconnected player, etc.)
   *  Also handles post-game storage cleanup. */
  async alarm(): Promise<void> {
    const now = Date.now();

    if (this.gameState.playerOrder.length === 0) return; // Game not started

    // Post-game cleanup: purge all storage once clients have had time to see the result.
    // deleteAll() also cancels any pending alarms, so the DO will go fully idle.
    if (this.gameState.phase === "game_over") {
      await this.state.storage.deleteAll();
      return;
    }

    if (this.gameState.phase === "projectile") {
      if (now >= this.phaseStartTime + PROJECTILE_TIMEOUT_MS) {
        // Projectile phase has been stuck too long — force-advance
        this.broadcast({ type: "force_advance", reason: "projectile_timeout" });
        this.advanceTurnAndMaybeBot();
      } else {
        // Not yet expired — re-arm (also reschedules next heartbeat)
        this.scheduleWatchdog();
      }
      return;
    }

    if (now >= this.gameState.turnEndTime + WATCHDOG_GRACE_MS) {
      // Turn timer expired and no end_turn ever arrived — force-advance
      this.broadcast({ type: "force_advance", reason: "turn_timeout" });
      this.advanceTurnAndMaybeBot();
      return;
    }

    // Watchdog hasn't fired yet — send heartbeat and re-arm for next tick
    const turnTimeRemainingMs = Math.max(0, this.gameState.turnEndTime - now);
    this.broadcast({
      type: "current_turn",
      currentTurnIndex: this.gameState.currentTurnIndex,
      turnNumber: this.gameState.globalTurnNumber ?? 0,
      turnTimeRemainingMs,
      phase: this.gameState.phase,
    });
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

  /** Work out a complete bot action plan: optional walk steps + a fire input.
   *  Returns ballIndex (bi) so the server can broadcast it with inputs for client sync. */
  private getBotActions(): {
    ballIndex: number;
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

    const fallback = (ballIdx: number): { ballIndex: number; walkDir: number; walkSteps: number; fireInput: string } => {
      const r = this.botRand();
      const angle = (r * 120) - 60; // -60..60 deg
      const power = 45 + Math.floor(r * 45);
      return {
        ballIndex: ballIdx,
        walkDir: 0,
        walkSteps: 0,
        fireInput: JSON.stringify({ Fire: { weapon: "Bazooka", angle_deg: angle, power_percent: power } }),
      };
    };

    if (balls.length === 0) return fallback(0);

    // The bot's own ball indices follow the interleaved spawn pattern:
    // team t has balls at [t, t+numPlayers, t+numPlayers*2]
    const botBallSet = new Set<number>();
    for (let wi = 0; wi < ballsPerTeam; wi++) {
      const i = idx + wi * numPlayers;
      if (i < balls.length) botBallSet.add(i);
    }

    // Collect alive balls for this team (round-robin, same logic as client sync_to_player_turn)
    const teamBalls: number[] = [];
    for (const i of botBallSet) {
      if (balls[i].alive) teamBalls.push(i);
    }
    if (teamBalls.length === 0) return fallback([...botBallSet][0] ?? 0);

    // Pick next in rotation after last
    const last = this.lastBallPerTeam[idx];
    let chosen: number;
    if (last !== undefined) {
      const afterPrev = teamBalls.find((bi) => bi > last);
      chosen = afterPrev ?? teamBalls[0];
    } else {
      chosen = teamBalls[0];
    }
    const shooter: BallData & { index: number } = { ...balls[chosen], index: chosen };

    // Collect alive enemy balls
    const enemies = balls
      .map((b, i) => ({ ...b, index: i }))
      .filter(b => b.alive && !botBallSet.has(b.index));
    if (enemies.length === 0) return fallback(chosen);

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
      ballIndex: chosen,
      walkDir,
      walkSteps,
      fireInput: JSON.stringify({ Fire: { weapon: "Bazooka", angle_deg: angleDeg, power_percent: power } }),
    };
  }

  async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
    const size = typeof message === "string" ? message.length : message.byteLength;
    if (size > MAX_MESSAGE_SIZE) {
      ws.close(1009, "Message too large");
      return;
    }
    const data = typeof message === "string" ? message : new TextDecoder().decode(message);
    const att = ws.deserializeAttachment() as { playerId: string } | null;
    if (!att) return;
    const playerId = att.playerId;
    const idx = this.playerIdToIndex.get(playerId);
    if (idx === undefined) return;

    // Accept several message types from ANY connected player (not just active turn).
    try {
      const parsed = JSON.parse(data) as { type: string; [k: string]: unknown };

      // ping: any client can send this when they detect the active player's turn
      // has expired. Re-broadcast state so all clients stay informed, and if the
      // turn really has expired, force-advance immediately (handles dev environments
      // where DO alarms are unavailable and the normal watchdog never fires).
      if (parsed.type === "ping") {
        const now = Date.now();
        const lastPing = this.lastPingPerPlayer.get(playerId) ?? 0;
        const pingRateLimited = now - lastPing < 2000;
        if (this.gameState.playerOrder.length > 0 &&
            now >= this.gameState.turnEndTime + WATCHDOG_GRACE_MS &&
            !pingRateLimited) {
          this.lastPingPerPlayer.set(playerId, now);
          this.broadcast({ type: "force_advance", reason: "ping_watchdog" });
          this.advanceTurnAndMaybeBot();
        } else {
          this.broadcast({ type: "state", state: this.gameState });
          this.scheduleWatchdog();
        }
        return;
      }

      // terrain_damages: accept from active player during their turn; if active player has been
      // silent > 8s (e.g. disconnected), allow from any player to prevent permanent desync.
      if (parsed.type === "terrain_damages") {
        const dmgMsg = parsed as { type: string; log?: number[][] };
        const isActivePlayer = this.gameState.currentTurnIndex === idx;
        const phaseAllowsTerrain = ["aiming", "charging", "projectile", "settling", "retreat"].includes(this.gameState.phase);
        const activeSilentTooLong = Date.now() - this.lastActivePlayerActivity > 8000;
        const logValid = Array.isArray(dmgMsg.log) && dmgMsg.log.length >= this.terrainDamageLog.length
          && dmgMsg.log.every((e: unknown) => Array.isArray(e) && e.length >= 3 && (e as unknown[]).every((n) => typeof n === "number"));
        const allowTerrain = (isActivePlayer && phaseAllowsTerrain) || (phaseAllowsTerrain && activeSilentTooLong);
        if (allowTerrain && logValid) {
          this.terrainDamageLog = dmgMsg.log as number[][];
          this.persistState();
          // Forward to all OTHER clients as terrain_sync so they stay in sync
          const syncMsg = JSON.stringify({ type: "terrain_sync", log: dmgMsg.log });
          for (const sock of this.state.getWebSockets()) {
            const sockAtt = sock.deserializeAttachment() as { playerId: string } | null;
            if (sockAtt?.playerId !== playerId) {
              try {
                sock.send(syncMsg);
              } catch (e) {
                console.warn("[Game] terrain_sync send failed for", sockAtt?.playerId, e);
              }
            }
          }
        }
        return;
      }
      // pos_update is a real-time position stream — relay from ANY player so
      // all clients can smoothly interpolate remote balls.
      // Also update our persisted snapshots so reconnecting clients get
      // fresh positions, not stale end-of-turn data.
      if (parsed.type === "pos_update") {
        const posNow = Date.now();
        const lastPos = this.lastPosUpdatePerPlayer.get(playerId) ?? 0;
        if (posNow - lastPos < POS_UPDATE_INTERVAL_MS) return; // 20/sec max per player
        this.lastPosUpdatePerPlayer.set(playerId, posNow);
        const pu = parsed as { bi?: number; x?: number; y?: number; vx?: number; vy?: number };
        const bi = pu.bi;
        if (typeof bi === "number" && bi >= 0 && bi < this.ballSnapshots.length) {
          const snap = this.ballSnapshots[bi];
          // Clamp to world bounds (terrain is 1400x800) to prevent extreme values
          const WORLD_W = 1400, WORLD_H = 800, VEL_MAX = 800;
          if (typeof pu.x === "number") snap.x = Math.max(-50, Math.min(WORLD_W + 50, pu.x));
          if (typeof pu.y === "number") snap.y = Math.max(-100, Math.min(WORLD_H + 100, pu.y));
          if (typeof pu.vx === "number") snap.vx = Math.max(-VEL_MAX, Math.min(VEL_MAX, pu.vx));
          if (typeof pu.vy === "number") snap.vy = Math.max(-VEL_MAX, Math.min(VEL_MAX, pu.vy));
        }
        this.broadcast(parsed as { type: string; [k: string]: unknown });
        return;
      }

      // ball_state: accept from ANY player but limit non-active players to updating
      // only their own team's balls.  This lets non-active clients (including observers
      // during a bot turn) report deaths so the server can correctly detect game-over.
      if (parsed.type === "ball_state") {
        const bs = parsed as { balls?: Array<{ x?: number; y?: number; vx?: number; vy?: number; hp?: number; alive?: boolean }> };
        if (Array.isArray(bs.balls)) {
          const isActivePlayer = this.gameState.currentTurnIndex === idx;
          const WORLD_W = 1400, WORLD_H = 800, VEL_MAX = 800, HP_MAX = 100;
          bs.balls.forEach((b, i) => {
            if (i < this.ballSnapshots.length && b !== null && typeof b === "object") {
              // Non-active players may only update their own team's ball indices to prevent
              // them from tampering with other teams' health/alive status.
              if (!isActivePlayer && this.ballTeamIndex(i) !== idx) return;
              const s = this.ballSnapshots[i];
              if (typeof b.x === "number" && !Number.isNaN(b.x)) s.x = Math.max(-50, Math.min(WORLD_W + 50, b.x));
              if (typeof b.y === "number" && !Number.isNaN(b.y)) s.y = Math.max(-100, Math.min(WORLD_H + 100, b.y));
              if (typeof b.vx === "number" && !Number.isNaN(b.vx)) s.vx = Math.max(-VEL_MAX, Math.min(VEL_MAX, b.vx));
              if (typeof b.vy === "number" && !Number.isNaN(b.vy)) s.vy = Math.max(-VEL_MAX, Math.min(VEL_MAX, b.vy));
              if (typeof b.hp === "number" && !Number.isNaN(b.hp)) s.hp = Math.max(0, Math.min(HP_MAX, Math.round(b.hp)));
              if (typeof b.alive === "boolean") s.alive = b.alive;
            }
          });
          if (isActivePlayer) {
            this.lastActivePlayerActivity = Date.now();
          }
          // Relay to other clients for position/health sync
          this.broadcast(parsed as { type: string; [k: string]: unknown });
        }
        return;
      }
    } catch (e) {
      console.warn("[Game] Parse error in message from", playerId, e);
    }

    // All other message types require it to be the current turn player
    if (this.gameState.currentTurnIndex !== idx) return;

    const current = this.gameState.playerOrder[this.gameState.currentTurnIndex];
    if (current?.isBot) return;

    const now = Date.now();

    try {
      const msg = JSON.parse(data) as { type: string; input?: string; aim?: number };
        // Allow host to request a restart (broadcast to all clients)
        if (msg.type === "restart" && typeof (msg as any).seed === "number") {
          if (this.hostId != null && playerId !== this.hostId) return; // Only host can restart
          const seed = (msg as any).seed as number;
          if (!Number.isInteger(seed) || seed < 0 || seed > 0xFFFFFFFF) return; // Invalid 32-bit seed
          // Reset server-side minimal state for the new game
          const numPlayers = this.gameState.playerOrder.length;
          this.gameState.rngSeed = seed;
          this.gameState.inputLog = [];
          this.gameState.currentTurnIndex = 0;
          this.gameState.globalTurnNumber = 0;
          this.gameState.phase = "aiming";
          this.gameState.turnEndTime = Date.now() + TURN_TIME_MS;
          this.phaseStartTime = Date.now();
          this.ballSnapshots = Array.from({ length: numPlayers * 3 }, () => ({
            x: 0, y: 0, vx: 0, vy: 0, hp: 100, alive: true,
          }));
          this.terrainDamageLog = [];
          this.lastBallPerTeam = Array.from({ length: numPlayers }, () => undefined);
          this.lastActivePlayerActivity = Date.now();
          this.broadcast({ type: "restart", seed });
          this.broadcast({ type: "state", state: this.gameState });
          this.scheduleWatchdog();
          this.persistState();
          return;
        }
      if (msg.type === "input" && typeof msg.input === "string") {
        // Validate input: max 2000 chars, must look like JSON (Fire, Walk, Jump, Backflip)
        if (msg.input.length > 2000 || msg.input.length < 2) return;
        const trimmed = msg.input.trim();
        if (trimmed[0] !== "{" && trimmed[0] !== "[") return; // Must be JSON object/array
        const lastInput = this.lastInputPerPlayer.get(playerId) ?? 0;
        if (now - lastInput < 40) return; // 25/sec max
        this.lastInputPerPlayer.set(playerId, now);
        this.lastActivePlayerActivity = now;
        // Check if this is a firing action (not movement)
        const isFiring = msg.input.includes('"Fire"');
        // Track which ball is active (client sends bi in every input message)
        const incomingBi = typeof (msg as any).bi === "number" ? (msg as any).bi as number : undefined;
        const biValid = incomingBi !== undefined && incomingBi >= 0 && incomingBi < this.ballSnapshots.length;
        if (biValid) {
          this.gameState.currentBallIndex = incomingBi!;
          // Update round-robin for human players
          const ti = this.gameState.currentTurnIndex;
          while (this.lastBallPerTeam.length <= ti) this.lastBallPerTeam.push(undefined);
          this.lastBallPerTeam[ti] = incomingBi;
        }
        const incomingBx = typeof (msg as any).bx === "number" ? (msg as any).bx as number : undefined;
        const incomingBy = typeof (msg as any).by === "number" ? (msg as any).by as number : undefined;

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
        // Include bi/bx/by so receivers know exactly which ball to apply this to.
        this.broadcast({
          type: "input",
          input: msg.input,
          turnIndex: this.gameState.currentTurnIndex,
          bi: this.gameState.currentBallIndex,
          ...(incomingBx !== undefined ? { bx: incomingBx } : {}),
          ...(incomingBy !== undefined ? { by: incomingBy } : {}),
        });

        if (isFiring) {
          this.broadcast({ type: "state", state: this.gameState });
        }
      } else if (msg.type === "aim" && typeof msg.aim === "number") {
        if (!Number.isFinite(msg.aim) || msg.aim < -4 || msg.aim > 7) return; // Valid angle (radians, ~-230° to 400°)
        const lastAim = this.lastAimPerPlayer.get(playerId) ?? 0;
        if (now - lastAim < 34) return; // ~30/sec max
        this.lastAimPerPlayer.set(playerId, now);
        this.broadcast({ type: "aim", aim: msg.aim, turnIndex: this.gameState.currentTurnIndex });
      } else if (msg.type === "end_turn") {
        this.lastActivePlayerActivity = now;
        // If the active player embedded a ball snapshot in end_turn, store it
        // so we can forward it in turn_advanced for authoritative end-of-turn sync.
        const etMsg = msg as any;
        if (Array.isArray(etMsg.balls)) {
          const WORLD_W = 1400, WORLD_H = 800, VEL_MAX = 800, HP_MAX = 100;
          etMsg.balls.forEach((b: unknown, i: number) => {
            if (i < this.ballSnapshots.length && b !== null && typeof b === "object") {
              const obj = b as Record<string, unknown>;
              const s = this.ballSnapshots[i];
              const x = obj.x; if (typeof x === "number" && !Number.isNaN(x)) s.x = Math.max(-50, Math.min(WORLD_W + 50, x));
              const y = obj.y; if (typeof y === "number" && !Number.isNaN(y)) s.y = Math.max(-100, Math.min(WORLD_H + 100, y));
              const vx = obj.vx; if (typeof vx === "number" && !Number.isNaN(vx)) s.vx = Math.max(-VEL_MAX, Math.min(VEL_MAX, vx));
              const vy = obj.vy; if (typeof vy === "number" && !Number.isNaN(vy)) s.vy = Math.max(-VEL_MAX, Math.min(VEL_MAX, vy));
              const hp = obj.hp; if (typeof hp === "number" && !Number.isNaN(hp)) s.hp = Math.max(0, Math.min(HP_MAX, Math.round(hp)));
              if (typeof obj.alive === "boolean") s.alive = obj.alive;
            }
          });
        }
        // Capture the active player's wind value so all clients use the same wind
        // next turn, eliminating trajectory divergence from independent RNG drift.
        if (typeof etMsg.wind === "number") {
          this.lastWindValue = etMsg.wind;
        }
        // Update round-robin from end_turn (human's last ball used this turn)
        const ti = this.gameState.currentTurnIndex;
        if (typeof this.gameState.currentBallIndex === "number") {
          while (this.lastBallPerTeam.length <= ti) this.lastBallPerTeam.push(undefined);
          this.lastBallPerTeam[ti] = this.gameState.currentBallIndex;
        }
        this.advanceTurn();
        this.maybeBotTurn();
      }
    } catch (e) {
      console.warn("[Game] Parse error in turn message from", playerId, e);
    }
  }

  private maybeBotTurn(): void {
    const current = this.gameState.playerOrder[this.gameState.currentTurnIndex];
    if (!current?.isBot) return;

    const turnIndex = this.gameState.currentTurnIndex;
    const { ballIndex, walkDir, walkSteps, fireInput } = this.getBotActions();

    // Set authoritative ball index so clients apply bot inputs to the correct worm
    this.gameState.currentBallIndex = ballIndex;
    // Update round-robin so next bot turn picks the next ball
    while (this.lastBallPerTeam.length <= turnIndex) this.lastBallPerTeam.push(undefined);
    this.lastBallPerTeam[turnIndex] = ballIndex;

    // Send an aim-angle preview so other clients see the bot "aiming"
    // (angle extracted from the fire input so it matches what will be fired)
    let previewAngleRad = 0;
    try {
      const parsed = JSON.parse(fireInput) as { Fire?: { angle_deg?: number } };
      previewAngleRad = ((parsed.Fire?.angle_deg ?? 0) * Math.PI) / 180;
    } catch (e) {
      console.warn("[Game] Bot fire input parse failed:", e);
    }
    this.broadcast({ type: "aim", aim: previewAngleRad, turnIndex });

    let stepIdx = 0;
    const doStep = (): void => {
      // Guard: ensure it's still this bot's turn
      if (this.gameState.currentTurnIndex !== turnIndex) return;

      if (stepIdx < walkSteps) {
        const walkInput = walkDir > 0
          ? '{"Walk":{"dir":1.0}}'
          : '{"Walk":{"dir":-1.0}}';
        this.broadcast({ type: "input", input: walkInput, turnIndex, bi: ballIndex });
        stepIdx++;
        setTimeout(doStep, 180);
      } else {
        // Fire
        this.gameState.inputLog.push(fireInput);
        this.broadcast({ type: "input", input: fireInput, turnIndex, bi: ballIndex });
        this.gameState.phase = "projectile";
        this.phaseStartTime = Date.now();
        this.scheduleWatchdog();
        this.broadcast({ type: "state", state: this.gameState });
        setTimeout(() => this.advanceTurnAndMaybeBot(), 1500);
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
    // The closed socket is automatically removed from state.getWebSockets().
    // No cleanup needed since we no longer maintain an in-memory sockets Map.
  }
}
