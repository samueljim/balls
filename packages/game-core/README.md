# game-core

Deterministic Worms-like game engine in Rust, compiled to WebAssembly.

## Build (requires Rust + wasm-pack)

```bash
# Install Rust: https://rustup.rs/
# Then: cargo install wasm-pack
wasm-pack build --target web --out-dir pkg
```

From repo root, after `apps/web` exists, you can copy `pkg/` to `apps/web/public/wasm/` or reference it from the Next app.

## API (WASM)

- `new()` – create game instance
- `init_round(seed, terrainId, wormPositions)` – start round (wormPositions: [x1,y1, x2,y2, ...] in pixels)
- `apply_input(inputJson)` – apply one input (Fire/Move/EndTurn)
- `get_state_json()` – current state as JSON
- `get_terrain_buffer()` – flat u8 array, 0=air 1=solid
- `terrain_width()` / `terrain_height()`
- `tick()` – advance one sim tick (projectile or gravity)
- `is_waiting_for_input()` – true when in aiming/movement phase
