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
  turnEndTime: number;
  phase: "aiming" | "projectile" | "retreat";
  rngSeed: number;
  terrainId: number;
}

export type GameMessage =
  | { type: "state"; state: Partial<GameState> }
  | { type: "input"; input: string; turnIndex: number }
  | { type: "aim"; aim: number; turnIndex: number }
  | { type: "turn_advanced"; turnIndex: number }
  | { type: "error"; message: string };

export type GameClientMessage =
  | { type: "input"; input: string }
  | { type: "aim"; aim: number }
  | { type: "end_turn" }
  | { type: "retreat_start" };
