import { NextResponse } from "next/server";
import { readFile } from "fs/promises";
import path from "path";

export const dynamic = "force-static";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

/** Serve public/js/gl.js so the game page always runs our miniquad loader, not a cached/bundled copy. */
export async function OPTIONS() {
  return new NextResponse(null, { status: 204, headers: CORS_HEADERS });
}

export async function GET() {
  const glPath = path.join(process.cwd(), "public", "js", "gl.js");
  try {
    const body = await readFile(glPath, "utf-8");
    return new NextResponse(body, {
      headers: {
        ...CORS_HEADERS,
        "Content-Type": "application/javascript",
        "Cache-Control": "no-store",
      },
    });
  } catch (e) {
    console.error("[api/gl-js]", e);
    return new NextResponse("Not found", { status: 404, headers: CORS_HEADERS });
  }
}
