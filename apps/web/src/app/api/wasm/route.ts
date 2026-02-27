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
 * Serve the macroquad game WASM so the correct file is always returned
 * (avoids Turbopack/static server serving wrong or cached wasm-bindgen build).
 * In production (static export) the client uses /wasm/game_core.wasm from public/.
 */
export async function OPTIONS() {
  return new NextResponse(null, { status: 204, headers: CORS_HEADERS });
}

export async function GET() {
  const wasmPath = path.join(process.cwd(), "public", "wasm", "game_core.wasm");
  try {
    const buffer = await readFile(wasmPath);
    return new NextResponse(buffer, {
      headers: {
        ...CORS_HEADERS,
        "Content-Type": "application/wasm",
        "Cache-Control": "no-store",
      },
    });
  } catch (e) {
    console.error("[api/wasm]", e);
    return new NextResponse("WASM not found", { status: 404, headers: CORS_HEADERS });
  }
}
