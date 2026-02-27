#!/usr/bin/env node
/**
 * Build the game-core WASM binary. Requires Rust (cargo) and wasm32 target.
 * Install: https://rustup.rs then: rustup target add wasm32-unknown-unknown
 * If cargo is not found, ensure your shell has run: source ~/.cargo/env (or restart the terminal).
 */
import { spawnSync } from "child_process";
import path from "path";
import fs from "fs";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const gameCoreDir = path.join(root, "packages", "game-core");
const outDir = path.join(root, "apps", "web", "public", "wasm");
const wasmPath = path.join(gameCoreDir, "target", "wasm32-unknown-unknown", "release", "game-core.wasm");

const cargoCheck = spawnSync("cargo", ["--version"], { encoding: "utf8" });
if (cargoCheck.status !== 0) {
  console.error("Rust (cargo) not found. The game engine is built with Rust.");
  console.error("Install: https://rustup.rs");
  console.error("Then run: rustup target add wasm32-unknown-unknown");
  console.error("If you just installed Rust, run: source ~/.cargo/env  (or restart your terminal).");
  process.exit(1);
}

console.log("Building game-core WASM...");
const result = spawnSync(
  "cargo",
  ["build", "--release", "--target", "wasm32-unknown-unknown"],
  { cwd: gameCoreDir, stdio: "inherit", shell: false }
);
if (result.status !== 0) process.exit(result.status ?? 1);

if (!fs.existsSync(wasmPath)) {
  console.error("Expected WASM at:", wasmPath);
  process.exit(1);
}
fs.mkdirSync(outDir, { recursive: true });
fs.copyFileSync(wasmPath, path.join(outDir, "game_core.wasm"));
console.log("Copied game_core.wasm to apps/web/public/wasm/");
process.exit(0);
