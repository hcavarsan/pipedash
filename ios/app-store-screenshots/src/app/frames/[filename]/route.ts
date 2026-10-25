import { promises as fs } from "node:fs";
import path from "node:path";
import { NextResponse } from "next/server";
import { FRAME_FILES } from "@/lib/frame-assets";

export const dynamic = "force-dynamic";

// Served at runtime: Next's production public-file index only covers files
// that existed at build time, and bezels may be swapped afterwards.
export async function GET(_req: Request, context: { params: Promise<{ filename: string }> }) {
  const { filename } = await context.params;
  const name = filename.replace(/\.png$/, "");
  if (!filename.endsWith(".png") || !(FRAME_FILES as readonly string[]).includes(name)) {
    return new NextResponse(null, { status: 404 });
  }
  try {
    const bytes = await fs.readFile(path.join(process.cwd(), "public", "frames", filename));
    return new NextResponse(new Uint8Array(bytes), { headers: {
      "Content-Type": "image/png",
      "Content-Length": String(bytes.length),
      // The URL carries ?v=<mtime>, so a replaced file gets a new URL.
      "Cache-Control": "public, max-age=31536000, immutable",
      "X-Content-Type-Options": "nosniff",
    } });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return new NextResponse(null, { status: 404 });
    return NextResponse.json({ ok: false, error: "Frame could not be read" }, { status: 500 });
  }
}
