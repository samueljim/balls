# Balls (web)

Web-based worms-style game: invite friends with a link, join with your name, turn-based artillery with destructible terrain. Built with Next.js, Cloudflare Workers + Durable Objects, and a Rust WASM game engine.

## Structure

- **apps/web** – Next.js app (lobby UI, game canvas)
- **apps/worker** – Cloudflare Worker + Durable Objects (Lobby, Game, Registry)
- **packages/game-core** – Rust game engine (terrain, physics, weapons), compiles to WASM

## Prerequisites

- Node 18+ and pnpm
- Rust + wasm-pack (for game engine): <https://rustup.rs/>, then `cargo install wasm-pack`
- Wrangler (for Worker): `pnpm install` in `apps/worker`

## Quick start

**Easiest:** [Deploy the worker to Cloudflare](#deploy-to-cloudflare-recommended), set `NEXT_PUBLIC_API_BASE` in `apps/web/.env.local`, then run the web app. Lobbies and WebSockets run on CF.

**Local only:** Build WASM, run the worker from `apps/worker`, then run the web app (see below). Local DO bindings can be unreliable; deploy is recommended.

### Local frontend development (hosted backend)

To work on the Next.js UI only, run the web app locally and point it at the deployed API (default):

```bash
pnpm install
pnpm dev:web
```

Then open **http://localhost:3000**. The app uses **https://api.balls.bne.sh** for API and WebSockets by default, so you don’t need a `.env.local` file. Create a game, share the link, join in another tab, etc.

**Game screen (WASM):** The in-browser game canvas needs the WASM build. If you don’t have Rust installed, the lobby and join flows work; the game view will show a load error until WASM is built. From repo root, run `pnpm run build:game` once (install [Rust](https://rustup.rs) and `cargo install wasm-pack` first). When you run `pnpm dev:web`, a watcher copies `packages/game-core/pkg` into `apps/web/public/wasm/` and keeps it in sync — after changing Rust, run `pnpm run build:game` again and refresh the browser to hot-reload WASM.

### 1. Build the game engine (WASM)

```bash
cd packages/game-core && wasm-pack build --target web --out-dir pkg
cp -r pkg/* ../../apps/web/public/wasm/
```

### 2. Run the Worker (lobby + game backend)

**Use a dedicated terminal** — start the worker from `apps/worker` only (don’t rely on root `pnpm dev` for the worker, or Durable Object bindings may not load):

```bash
cd apps/worker && pnpm install && pnpm dev
```

Worker runs at `http://localhost:8787` by default. If Wrangler shows a different port, set `NEXT_PUBLIC_API_BASE` and `NEXT_PUBLIC_WS_BASE` in the web app to that host (e.g. `http://localhost:8788`, `ws://localhost:8788`).

### 3. Run the Next.js app

```bash
cd apps/web && pnpm install && pnpm dev
```

Set env so the app talks to the Worker:

- `NEXT_PUBLIC_API_BASE=http://localhost:8787`
- `NEXT_PUBLIC_WS_BASE=ws://localhost:8787`

Then open `http://localhost:3000`: create a game, copy the invite link, open it in another tab and join with a name. Add bots in the lobby if you like, then start the game.

## Deploy to Cloudflare (recommended)

Deploying the worker to Cloudflare fixes "Durable Objects not loaded" and gives you a stable URL for API + WebSockets. You can run the web app locally or deploy it separately.

### 1. Deploy the worker

```bash
# Log in to Cloudflare (one-time)
cd apps/worker && npx wrangler login

# Deploy (from repo root or apps/worker)
pnpm deploy:worker
# or: cd apps/worker && pnpm deploy
```

Wrangler will print your worker URL, e.g. `https://balls-worker.<your-subdomain>.workers.dev`. **Use this URL for the web app.**

### 2. Point the web app at the worker

Create `apps/web/.env.local` (or set env in your host):

```bash
NEXT_PUBLIC_API_BASE=https://balls-worker.<your-subdomain>.workers.dev
```

WebSockets use the same host (wss://...). The app derives the WebSocket URL from `NEXT_PUBLIC_API_BASE`, so you don’t need to set `NEXT_PUBLIC_WS_BASE` unless you use a different host for WS.

### 3. Run the web app

```bash
cd apps/web && pnpm dev
```

Open http://localhost:3000 and create a game. The app will call your deployed worker; lobbies and WebSockets run on Cloudflare.

### Deploy the frontend (optional)

- **Cloudflare Pages**: Connect the repo to Pages, set the build to `apps/web` (or your static/Next output), and add `NEXT_PUBLIC_API_BASE` (and `NEXT_PUBLIC_WS_BASE` if needed) in the Pages env.
- **Vercel / other**: Set `NEXT_PUBLIC_API_BASE` and `NEXT_PUBLIC_WS_BASE` to your worker URL.

## balls.bne.sh setup

The **worker** (API + WebSockets) is at **api.balls.bne.sh**. The **site** (Next.js app) is at **balls.bne.sh** and must be deployed separately to Cloudflare Pages.

**Why isn’t balls.bne.sh loading?** Only the worker is deployed by `pnpm deploy:worker`. The frontend is not deployed by that command. Deploy the web app to Cloudflare Pages and add the custom domain **balls.bne.sh** (steps below).

### 1. Deploy the worker (API at api.balls.bne.sh) — already done

```bash
cd apps/worker && npx wrangler login
pnpm deploy:worker   # from repo root
```

Cloudflare creates the custom domain **api.balls.bne.sh** and the DNS record (bne.sh must be your zone). The worker is also at `https://balls-worker.<subdomain>.workers.dev`.

### 2. Deploy the frontend (site at balls.bne.sh)

1. In [Cloudflare Dashboard](https://dash.cloudflare.com) go to **Workers & Pages** → **Create** → **Pages** → **Connect to Git**.
2. Select this repo and branch.
3. **Build configuration** (choose one):

   **Option A — With WASM (game canvas works):** Build from repo root so Rust can run. The script installs Rust in the cloud if needed.
   - **Root directory**: leave blank (repo root)
   - **Build command**: `pnpm install && pnpm run build:web`
   - **Build output directory**: `apps/web/out`
   - **Environment variables** (Production): **NEXT_PUBLIC_API_BASE** = `https://api.balls.bne.sh`

   **Option B — Without WASM (lobby/join only):** Build from `apps/web` only (no Rust in cloud).
   - **Root directory**: `apps/web`
   - **Build command**: `pnpm install && pnpm run build`
   - **Build output directory**: `out`

4. Save and deploy. After the first deploy, go to the project → **Custom domains** → **Set up a custom domain** → add **balls.bne.sh**.

**Alternative (CLI):** From repo root run `pnpm deploy:web`. This **requires Rust + wasm-pack** on your machine: it builds the game WASM, copies it into the web app, builds the static export, and deploys to Cloudflare Pages (project name `worms-web`). The app defaults to **api.balls.bne.sh** for API and WebSockets.

**Use balls.bne.sh instead of *.pages.dev:** In [Cloudflare Dashboard](https://dash.cloudflare.com) go to **Workers & Pages** → your Pages project (**worms-web**) → **Custom domains** → **Set up a custom domain** → enter **balls.bne.sh** and save. Cloudflare will add the DNS record (bne.sh must be your zone). After that, the site is available at **https://balls.bne.sh** as well as the *.pages.dev URL.

**Build output directory:** If your Next.js app is set to static export (`output: 'export'` in `next.config.js`), the output directory is `out`. If you use the default Next.js build (no static export), Cloudflare may expect `.next` and run a Node server; in that case pick the preset that matches (e.g. “Next.js” and the output directory your build produces). For a client-only app, static export is simplest: set `output: 'export'` in `apps/web/next.config.js`, then Build output directory = `out`.

### 3. Game engine (WASM) on the site

If you used **Option A** (build:web from repo root) or **`pnpm deploy:web`**, the WASM build is already part of the deploy and the in-browser game runs. If you used Option B (apps/web only), either switch to Option A for the next deploy or build locally: `pnpm run build:game`, copy `packages/game-core/pkg/*` to `apps/web/public/wasm/`, commit and push. Then open **https://balls.bne.sh**: create a game, share the link (e.g. https://balls.bne.sh/join/CODE), and play.

## Troubleshooting

**"Durable Objects not loaded" (503 when creating a game)**

The app defaults to **https://api.balls.bne.sh** (local and on Pages). So the worker must be deployed: run `pnpm deploy:worker` from repo root. Ensure **bne.sh** is your Cloudflare zone so the custom domain **api.balls.bne.sh** is created. To use a local worker instead, set **NEXT_PUBLIC_API_BASE** = `http://localhost:8787` in `apps/web/.env.local`.

**`GET /wasm/game_core.js` 404 (WASM load failed)**

The game canvas needs the WASM build in the deployed site. **Git-based Pages:** use **Option A** in “Deploy the frontend” (repo root, build command `pnpm run build:web`) so the cloud build installs Rust and builds WASM. **CLI deploy:** run **`pnpm deploy:web`** from your machine (requires Rust + wasm-pack; builds WASM and deploys). Optional: set **NEXT_PUBLIC_WASM_BASE** if you host WASM elsewhere.

**WebSocket connection to api.balls.bne.sh failed**

The game and lobby use WebSockets on the same host as the API. Ensure the **worker** is deployed and **api.balls.bne.sh** (or your worker URL) resolves to it: run `pnpm deploy:worker` and check Cloudflare Dashboard → Workers & Pages → your worker → Custom domains. If you use a different API URL, set **NEXT_PUBLIC_API_BASE** (and **NEXT_PUBLIC_WS_BASE** if the WebSocket host differs) in the web app’s build env (e.g. in Pages → Settings → Environment variables).

## Notes

- WASM must be built and present at `apps/web/public/wasm/` (e.g. `game_core.js`, `game_core_bg.wasm`) for the in-browser game to run. Without it, the canvas may not render correctly.
- Lobby codes are stored in the Registry DO; invite link is `https://yoursite.com/join/<CODE>`.
