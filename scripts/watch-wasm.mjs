#!/usr/bin/env node
/**
 * In dev: watch packages/game-core for changes; after build, copy game_core.wasm to apps/web/public/wasm.
 * Run from repo root. Run pnpm run build:game once to build WASM, then this script watches and copies
 * when you run build:game again (or we could trigger build on source change - for now just watch the target).
 */
import fs from "fs";
import path from "path";
import { spawnSync } from "child_process";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const gameCoreDir = path.join(root, "packages", "game-core");
const wasmPath = path.join(gameCoreDir, "target", "wasm32-unknown-unknown", "release", "game_core.wasm");
const outDir = path.join(root, "apps", "web", "public", "wasm");

function copy() {
  if (!fs.existsSync(wasmPath)) {
    fs.mkdirSync(outDir, { recursive: true });
    return;
  }
  fs.mkdirSync(outDir, { recursive: true });
  fs.copyFileSync(wasmPath, path.join(outDir, "game_core.wasm"));
  console.log("[watch-wasm] Copied game_core.wasm to public/wasm");
}

function buildAndCopy() {
  console.log("[watch-wasm] Building game-core...");
  const r = spawnSync("cargo", ["build", "--release", "--target", "wasm32-unknown-unknown"], {
    cwd: gameCoreDir,
    stdio: "inherit",
    shell: false,
  });
  if (r.status === 0) copy();
}

if (fs.existsSync(wasmPath)) {
  copy();
}

let debounce;
const watchDir = path.join(gameCoreDir, "src");
if (!fs.existsSync(watchDir)) {
  console.log("[watch-wasm] No packages/game-core/src — run pnpm run build:game to build WASM.");
} else {
  fs.watch(watchDir, { recursive: true }, () => {
    clearTimeout(debounce);
    debounce = setTimeout(buildAndCopy, 500);
  });
  console.log("[watch-wasm] Watching packages/game-core/src — saves will trigger build and copy. Or run pnpm run build:game then refresh.");
}
