#!/usr/bin/env node
/**
 * Run wrangler dev from apps/worker with explicit cwd so DO bindings load.
 * Usage: node scripts/run-worker-dev.mjs (from repo root)
 */
import { spawn } from "child_process";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(__dirname, "..");
const workerDir = path.join(root, "apps", "worker");

const child = spawn(
  "pnpm",
  ["exec", "wrangler", "dev", "--persist-to", ".wrangler/state"],
  {
    cwd: workerDir,
    stdio: "inherit",
    shell: false,
    env: { ...process.env, FORCE_COLOR: "1" },
  }
);

child.on("exit", (code, signal) => {
  process.exit(code ?? (signal ? 1 : 0));
});
