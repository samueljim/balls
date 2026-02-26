declare module "/wasm/game_core.js" {
  const init: () => Promise<void>;
  const Game: new () => {
    apply_input: (s: string) => void;
    get_state_json: () => string;
    get_terrain_buffer: () => Uint8Array;
    terrain_width: () => number;
    terrain_height: () => number;
    tick: () => void;
    init_round: (seed: number, tid: number, positions: number[]) => void;
  };
  export default init;
  export { Game };
}
