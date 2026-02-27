# Game integration (macroquad + WASM + Web)

Summary of what’s needed to run the Worms game in the browser, based on the [macroquad WASM article](https://macroquad.rs/articles/wasm/), Context7 docs, and the project plan.

## 1. Build and artifacts

- **Rust**: `cargo build --release --target wasm32-unknown-unknown` in `packages/game-core` produces **`game-core.wasm`** (hyphen). The build script must copy this file (not `game_core.wasm` from an old build).
- **Copy**: `game-core.wasm` → `apps/web/public/wasm/game_core.wasm` (served as `game_core.wasm` to the client).
- **Scripts**: `pnpm run build:game` runs the build and copy.

## 2. Load order (miniquad)

Per macroquad/miniquad:

1. **Canvas** must exist with `id="glcanvas"` and a `tabindex` so it can receive keyboard input.
2. **gl.js** (miniquad’s WASM loader) must load first. It defines `load(url)` and the `importObject.env` table (WebGL, console, etc.).
3. **ws_plugin.js** (our plugin) must load next. It calls `miniquad_add_plugin()` to add `js_send_ws` to `importObject.env` and runs `on_init` to open the WebSocket and call `on_game_init` / `on_ws_message`.
4. **Call** `load(wasmUrl)` so the loader fetches the WASM, instantiates it with the import table, and runs the game’s `main()`.

The game page does: inject gl.js → ws_plugin.js → `load(wasmUrl)`.

## 3. Game loop (macroquad)

From Context7 / macroquad docs:

- Entry: `#[macroquad::main(window_conf)] async fn main() { ... }`.
- Each frame: update state, draw, then `next_frame().await`.
- Input: `is_key_down(KeyCode::X)`, `is_key_pressed(KeyCode::X)`, `mouse_position()`, `get_frame_time()`, etc.
- No separate “update vs draw” callback; one loop that handles both.

Our `main.rs` follows this pattern.

## 4. WebSocket / multiplayer

- **Lobby** stores `playerOrder` in `sessionStorage` under `balls:${gameId}` and redirects to `/game/${gameId}?playerId=...`.
- **ws_plugin.js** reads `gameId` from the path, `playerId` from the query, and `playerOrder` from sessionStorage; connects to `wss://.../game/${gameId}?playerId=...`; on open, POSTs to `/game/${gameId}/init` and calls `wasm_exports.on_game_init(ptr, len)` with init JSON.
- **Worker** must accept `POST /game/:gameId/init` (path ends with `/init`) and forward to the Game DO so init runs.
- **Rust** uses `extern "C" fn js_send_ws(ptr, len)` to send; plugin provides it and sends over the WebSocket. Rust exports `alloc_buffer`, `on_ws_message`, `on_game_init` for the plugin.

## 5. API base

- Set `window.__BALLS_WS_BASE` before loading scripts so the plugin uses the correct API origin (e.g. local worker or production).

## 6. Checklist

- [x] Canvas `#glcanvas` with `tabIndex` for focus.
- [x] gl.js then ws_plugin.js then `load(wasmUrl)`.
- [x] Build copies `game-core.wasm` → `public/wasm/game_core.wasm`.
- [x] Game DO accepts init when path ends with `/init`.
- [x] Lobby sets sessionStorage and redirects to `/game/:id?playerId=...`.
- Optional: focus canvas after load so keys work without an extra click.
