# Worms (web)

Web-based Worms-style game: invite friends with a link, join with your name, turn-based artillery with destructible terrain. Built with Next.js, Tailwind, Cloudflare Workers + Durable Objects, and a Rust WASM game engine.

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

Wrangler will print your worker URL, e.g. `https://worms-worker.<your-subdomain>.workers.dev`. **Use this URL for the web app.**

### 2. Point the web app at the worker

Create `apps/web/.env.local` (or set env in your host):

```bash
NEXT_PUBLIC_API_BASE=https://worms-worker.<your-subdomain>.workers.dev
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

## worms.bne.sh setup

The **worker** (API + WebSockets) is at **api.worms.bne.sh**. The **site** (Next.js app) is at **worms.bne.sh** and must be deployed separately to Cloudflare Pages.

**Why isn’t worms.bne.sh loading?** Only the worker is deployed by `pnpm deploy:worker`. The frontend is not deployed by that command. Deploy the web app to Cloudflare Pages and add the custom domain **worms.bne.sh** (steps below).

### 1. Deploy the worker (API at api.worms.bne.sh) — already done

```bash
cd apps/worker && npx wrangler login
pnpm deploy:worker   # from repo root
```

Cloudflare creates the custom domain **api.worms.bne.sh** and the DNS record (bne.sh must be your zone). The worker is also at `https://worms-worker.<subdomain>.workers.dev`.

### 2. Deploy the frontend (site at worms.bne.sh)

1. In [Cloudflare Dashboard](https://dash.cloudflare.com) go to **Workers & Pages** → **Create** → **Pages** → **Connect to Git**.
2. Select this repo and branch.
3. **Build configuration**:
   - **Framework preset**: Next.js (Static HTML Export)
   - **Root directory**: `apps/web`
   - **Build command**: `pnpm install && pnpm run build` (or `npm run build` if you use npm in the build)
   - **Build output directory**: `out` (see note below)
4. **Environment variables** (Production): add **NEXT_PUBLIC_API_BASE** = `https://api.worms.bne.sh`
5. Save and deploy. After the first deploy, go to the project → **Custom domains** → **Set up a custom domain** → add **worms.bne.sh**.

**Alternative (CLI):** From repo root run `pnpm deploy:web` to build and deploy the static export to Cloudflare Pages (project name `worms-web`). The app defaults to **api.worms.bne.sh** for API and WebSockets, so you don’t need to set env vars for that.

**Use worms.bne.sh instead of *.pages.dev:** In [Cloudflare Dashboard](https://dash.cloudflare.com) go to **Workers & Pages** → your Pages project (**worms-web**) → **Custom domains** → **Set up a custom domain** → enter **worms.bne.sh** and save. Cloudflare will add the DNS record (bne.sh must be your zone). After that, the site is available at **https://worms.bne.sh** as well as the *.pages.dev URL.

**Build output directory:** If your Next.js app is set to static export (`output: 'export'` in `next.config.js`), the output directory is `out`. If you use the default Next.js build (no static export), Cloudflare may expect `.next` and run a Node server; in that case pick the preset that matches (e.g. “Next.js” and the output directory your build produces). For a client-only app, static export is simplest: set `output: 'export'` in `apps/web/next.config.js`, then Build output directory = `out`.

### 3. Build the game engine (WASM)

So the in-browser game runs on worms.bne.sh:

```bash
cd packages/game-core && wasm-pack build --target web --out-dir pkg
cp -r pkg/* ../../apps/web/public/wasm/
```

Commit and push (if using Git deploy), or include `public/wasm/` in your upload. Then open **https://worms.bne.sh**: create a game, share the link (e.g. https://worms.bne.sh/join/CODE), and play.

## Troubleshooting

**"Durable Objects not loaded" (503 when creating a game)**

The app defaults to **https://api.worms.bne.sh** (local and on Pages). So the worker must be deployed: run `pnpm deploy:worker` from repo root. Ensure **bne.sh** is your Cloudflare zone so the custom domain **api.worms.bne.sh** is created. To use a local worker instead, set **NEXT_PUBLIC_API_BASE** = `http://localhost:8787` in `apps/web/.env.local`.

## Notes

- WASM must be built and present at `apps/web/public/wasm/` (e.g. `game_core.js`, `game_core_bg.wasm`) for the in-browser game to run. Without it, the canvas may not render correctly.
- Lobby codes are stored in the Registry DO; invite link is `https://yoursite.com/join/<CODE>`.
