export interface Player {
  id: string;
  name: string;
  ready?: boolean;
  isBot?: boolean;
}

export interface LobbyState {
  lobbyCode: string;
  hostId: string;
  players: Player[];
  started: boolean;
  gameId: string | null;
}

export type LobbyMessage =
  | { type: "player_list"; players: Player[] }
  | { type: "player_joined"; player: Player }
  | { type: "player_left"; playerId: string }
  | { type: "set_ready"; playerId: string; ready: boolean }
  | { type: "add_bot"; player: Player }
  | { type: "remove_bot"; playerId: string }
  | { type: "game_started"; gameId: string; playerOrder?: { playerId: string; isBot: boolean; name: string }[]; rngSeed?: number }
  | { type: "error"; message: string };

export type LobbyClientMessage =
  | { type: "set_ready"; ready: boolean }
  | { type: "start_game" }
  | { type: "add_bot" }
  | { type: "remove_bot"; playerId: string };

export interface GameState {
  playerOrder: { playerId: string; isBot: boolean; name: string }[];
  inputLog: string[];
  currentTurnIndex: number;
  /** Index into the flat balls[] array of the currently active ball (authoritative) */
  currentBallIndex: number;
  turnEndTime: number;
  phase: string;
  rngSeed: number;
  terrainId: number;
  /** Monotonically increasing global turn counter. Drives deterministic worm selection:
   *  currentTurnIndex = globalTurnNumber % numPlayers
   *  worm_slot = floor(globalTurnNumber / numPlayers) % aliveBallsForTeam.length */
  globalTurnNumber: number;
}

export type GameMessage =
  | { type: "state"; state: Partial<GameState> }
  | { type: "input"; input: string; turnIndex: number; bi: number; bx?: number; by?: number }
  | { type: "aim"; aim: number; turnIndex: number }
  | { type: "turn_advanced"; turnIndex: number; turnNumber?: number; balls?: Array<{ x: number; y: number; vx: number; vy: number; hp: number; alive: boolean }>; wind?: number; terrain?: number[][] }
  | { type: "force_advance"; reason?: string }
  | { type: "game_over"; winner: string }
  | { type: "restart"; seed: number }
  | { type: "terrain_sync"; log: number[][] }
  | { type: "identity"; myPlayerIndex: number; playerId: string; rngSeed: number }
  | { type: "game_resync"; phase: string; currentTurnIndex: number; currentBallIndex: number; turnTimeRemainingMs: number; globalTurnNumber: number; balls?: Array<{ x: number; y: number; vx: number; vy: number; hp: number; alive: boolean }> }
  | { type: "current_turn"; currentTurnIndex: number; turnNumber: number; turnTimeRemainingMs: number; phase: string }
  | { type: "error"; message: string };

export type GameClientMessage =
  | { type: "input"; input: string }
  | { type: "aim"; aim: number }
  | { type: "end_turn" };
