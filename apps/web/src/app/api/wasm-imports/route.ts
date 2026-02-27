import { NextResponse } from "next/server";
import { readFile } from "fs/promises";
import path from "path";

export const dynamic = "force-static";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

/**
 * Debug: list first 25 imports of the WASM file on disk.
 * If you see __wbindgen_placeholder__, the file is a wasm-bindgen build (wrong).
 * Macroquad/miniquad expects env.* (e.g. env.console_log, env.glClear).
 */
export async function OPTIONS() {
  return new NextResponse(null, { status: 204, headers: CORS_HEADERS });
}

export async function GET() {
  const wasmPath = path.join(process.cwd(), "public", "wasm", "game_core.wasm");
  try {
    const buffer = await readFile(wasmPath);
    const mod = new WebAssembly.Module(buffer);
    const imports = WebAssembly.Module.imports(mod);
    const preview = imports.slice(0, 25).map((i) => `${i.module}.${i.name}`);
    return NextResponse.json(
      {
        size: buffer.length,
        totalImports: imports.length,
        firstImports: preview,
      },
      { headers: CORS_HEADERS }
    );
  } catch (e) {
    console.error("[api/wasm-imports]", e);
    return NextResponse.json({ error: String(e) }, { status: 500, headers: CORS_HEADERS });
  }
}
