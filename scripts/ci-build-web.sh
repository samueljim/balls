#!/usr/bin/env bash
# Build the web app with WASM for CI/Cloudflare Pages (Git deploy).
# Run from repo root. Installs Rust + wasm-pack if missing, then build:game and Next.
set -e

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

# Install Rust if cargo not in PATH (e.g. Cloudflare Pages)
if ! command -v cargo &>/dev/null; then
  echo "[ci-build-web] Installing Rust..."
  curl -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  export PATH="${HOME}/.cargo/bin:${PATH}"
  cargo install wasm-pack --locked
fi

pnpm install
pnpm run build:game

mkdir -p apps/web/public/wasm
if [ ! -d packages/game-core/pkg ]; then
  echo "[ci-build-web] ERROR: packages/game-core/pkg missing after build:game"
  exit 1
fi
cp packages/game-core/pkg/* apps/web/public/wasm/

cd apps/web && pnpm run build
echo "[ci-build-web] Done. Output in apps/web/out"
