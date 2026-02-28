# Copilot Instructions — Balls (worms-style web game)

## Architecture Overview

Three-layer monorepo (pnpm workspaces):

| Layer | Path | Tech | Role |
|---|---|---|---|
| Game engine | `packages/game-core` | Rust → WASM (macroquad/miniquad) | Physics, terrain, weapons, rendering |
| Backend | `apps/worker` | Cloudflare Workers + Durable Objects | Lobby, game state, WebSockets |
| Frontend | `apps/web` | Next.js (App Router) | Lobby UI, game canvas host |

**Data flow:** Browser ↔ Worker DO (WebSocket) ↔ Game DO (turn/input log) ← WASM on client renders from shared input log.

The game is deterministic: every client runs the Rust WASM engine locally; the Worker only distributes inputs. `game.ts` stores an `inputLog` and `terrainDamageLog` so reconnecting clients can replay state.

## Critical Build Workflow

```bash
# Build Rust WASM → copies to apps/web/public/wasm/game_core.wasm
pnpm run build:game

# After build, deploy WASM to game page (dev only):
cp apps/web/public/wasm/game_core.wasm apps/web/public/wasm/game.wasm

# Full local dev (worker + wasm watcher + web):
pnpm dev              # runs all three concurrently

# Worker only (recommended standalone — DO bindings unreliable via root dev):
cd apps/worker && pnpm dev

# Web only (against deployed API, no local worker needed):
pnpm dev:web
```

**Never use `wasm-pack`** for day-to-day builds — the project uses `cargo build --target wasm32-unknown-unknown --release` directly. The `build:game` script handles this and copies the `.wasm` file.

## WASM Load Order (Critical)

The game page (`apps/web/public/game-standalone.html` / game route) must inject scripts in this exact order:
1. `gl.js` — miniquad's WASM loader, defines `load(url)` and `importObject.env`
2. `ws_plugin.js` — adds `js_send_ws` to `importObject.env`, opens WebSocket, calls `on_game_init`
3. `load(wasmUrl)` — fetches and instantiates the WASM

Canvas must have `id="glcanvas"` with a `tabindex` for keyboard focus.

## Durable Object Architecture

Three DOs in `apps/worker/src/`:
- **`Registry`** (singleton `"default"`) — maps short join codes → lobbyId
- **`Lobby`** — per-game lobby, manages player list, starts game, creates Game DO
- **`Game`** — per-game session, holds `inputLog`, `terrainDamageLog`, `ballSnapshots`; persists all state to DO storage for hibernation survival

All three must be re-exported from `index.ts` for Wrangler to register them.

## Game Engine Patterns

**Adding a weapon** requires changes in `packages/game-core/src/weapons.rs`:
- Add variant to `Weapon` enum
- Implement `name()`, `weapon_type()`, `explosion_radius()`, `base_damage()`, `speed_factor()`, `fuse_time()`, `max_bounces()`, `cluster_count()`, `category()`, `icon()`, `description()`
- Add to `Weapon::all()` and `Weapon::from_name()`
- Implement firing logic in `main.rs` (search for the weapon in the `fire_weapon` / input handling section)

**Game phases** (`packages/game-core/src/state.rs`): `Aiming → Charging → ProjectileFlying → Settling → Retreat → TurnEnd → GameOver`. Input is only accepted in `Aiming`/`Charging`.

## Environment Variables

```bash
# apps/web/.env.local (only needed for local worker)
NEXT_PUBLIC_API_BASE=http://localhost:8787
NEXT_PUBLIC_WS_BASE=ws://localhost:8787
# Defaults to https://api.balls.bne.sh if unset
```

Production worker: `https://api.balls.bne.sh` (Cloudflare custom domain in `wrangler.toml`).

## WebSocket Protocol

Lobby WS messages: `player_list`, `player_joined`, `game_started` (includes `playerOrder`, `rngSeed`).  
Game WS messages: `state` (partial `GameState`), `input` (forwarded to all clients), `turn_advanced`.  
Client → Game: `{ type: "input", input: "<json>" }` | `{ type: "aim", aim: <radians> }` | `{ type: "end_turn" }`.

`playerOrder` and `rngSeed` are stored in `sessionStorage` as `balls:<gameId>` by the lobby before redirecting to `/game/<id>`.

## Key Files — Game Engine (`packages/game-core/src/`)

| File | Role |
|---|---|
| [main.rs](packages/game-core/src/main.rs) | ~3195-line monolith: `Game` struct, entire loop, all rendering + input. `handle_input()` at line 368 is where weapon firing dispatches. |
| [weapons.rs](packages/game-core/src/weapons.rs) | All 27 weapons: enum variants, stats methods (`explosion_radius`, `base_damage`, `speed_factor`, `fuse_time`, `max_bounces`, `cluster_count`), UI methods (`icon`, `description`, `category`). |
| [terrain.rs](packages/game-core/src/terrain.rs) | 1400×800 pixel terrain, cell types `AIR/DIRT/GRASS/STONE/LAVA/WOOD`. `apply_damage(cx, cy, radius)` carves a circle and appends to `damage_log` for reconnect replay. `WATER_LEVEL = 740.0`. |
| [physics.rs](packages/game-core/src/physics.rs) | `Ball` struct. Key constants: `GRAVITY=480`, `WALK_SPEED=115`, `MOVEMENT_BUDGET=170`. Implements coyote time (0.15 s), jump buffer (0.12 s), fall damage, wall-impact damage. |
| [projectile.rs](packages/game-core/src/projectile.rs) | `Projectile`, `ClusterBomblet`, `ShotgunPellet`, `Explosion` structs. Each has a `tick()` that handles gravity, terrain collision, and Ball damage. |
| [special_weapons.rs](packages/game-core/src/special_weapons.rs) | `AirstrikeDroplet` (explosive + napalm variants), `FirePool`, `UziBullet`, `PlacedExplosive`. All follow the same `tick() → Option<Explosion>` pattern. |
| [network.rs](packages/game-core/src/network.rs) | `NetworkState`: wraps `js_send_ws` FFI for outbound WS messages and `js_game_event` FFI for UI toast events (hit/died/turn_start/game_over). `poll_messages()` drains the thread-local incoming queue. |
| [camera.rs](packages/game-core/src/camera.rs) | `GameCamera`: smooth `follow()`, inertial `pan_push()`, `apply_momentum()`. `BASE_SHORT_AXIS=350` controls zoom tightness. |
| [hud.rs](packages/game-core/src/hud.rs) | All HUD rendering + `WeaponMenuLayout` (mobile `<600 px CSS` vs desktop). Menu scroll, category headers, weapon item hit-testing. |
| [state.rs](packages/game-core/src/state.rs) | `Phase` enum: `Aiming → Charging → ProjectileFlying → Settling → Retreat → TurnEnd → GameOver`. `allows_input()` and `allows_movement()` gate which actions are legal. |

## Key Files — Worker (`apps/worker/src/`)

| File | Role |
|---|---|
| [index.ts](apps/worker/src/index.ts) | HTTP router + CORS. Routes `/lobby/create`, `/lobby/join/:code`, `/lobby/:id/*`, `/game/:id/*` to the appropriate DO stubs. All three DOs must be re-exported here for Wrangler to register them. |
| [lobby.ts](apps/worker/src/lobby.ts) | `Lobby` DO: player list, bot management, generates 6-char join code, creates `Game` DO on start, persists state to DO storage for hibernation. |
| [game.ts](apps/worker/src/game.ts) | `Game` DO: holds `inputLog`, `terrainDamageLog`, `ballSnapshots`. Watchdog timers: `TURN_TIME_MS=45000`, `WATCHDOG_GRACE_MS=5000`, `PROJECTILE_TIMEOUT_MS=20000`. Persists to DO storage. |
| [registry.ts](apps/worker/src/registry.ts) | `Registry` DO (singleton `"default"`): maps short join codes → lobbyId. |
| [types.ts](apps/worker/src/types.ts) | Shared TS interfaces: `Player`, `LobbyState`, `GameState`, `LobbyMessage`, `GameMessage`, `LobbyClientMessage`, `GameClientMessage`. |

## Key Files — Web (`apps/web/`)

| File | Role |
|---|---|
| [public/js/ws_plugin.js](apps/web/public/js/ws_plugin.js) | JS↔WASM bridge. Reads `gameId` from URL path, `playerId` from query, `playerOrder`+`rngSeed` from `sessionStorage` (`balls:<gameId>`). Registers `js_send_ws` in miniquad's `importObject.env`. |
| [public/js/gl.js](apps/web/public/js/gl.js) | miniquad's WASM loader. Defines `load(url)` and `importObject.env` (WebGL, console bindings). Must load before `ws_plugin.js`. |
| [src/app/game/[id]/GameClient.tsx](apps/web/src/app/game/%5Bid%5D/GameClient.tsx) | Injects scripts in order (`gl.js → ws_plugin.js → mobile_controls.js`), calls `load(wasmUrl)`, sets `window.__BALLS_WS_BASE`, focuses `#glcanvas`. On localhost serves WASM via `/api/wasm` route; in production uses `/wasm/game_core.wasm`. |
| [src/lib/ws.ts](apps/web/src/lib/ws.ts) | `useWebSocket` hook with exponential-backoff reconnect, message queue for sends-while-disconnected, and `messageVersion` counter to prevent missed updates. |
| [src/lib/api.ts](apps/web/src/lib/api.ts) | `API_BASE` constant (reads `NEXT_PUBLIC_API_BASE`, defaults to `https://api.balls.bne.sh`) and `apiJson` helper. |
| [docs/GAME_INTEGRATION.md](docs/GAME_INTEGRATION.md) | Detailed WASM load/init sequence, WS protocol, and integration checklist. |
