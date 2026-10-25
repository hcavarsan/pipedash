import { promises as fs } from "node:fs";
import path from "node:path";
import { NextResponse } from "next/server";
import { FRAME_FILES, type FrameFile, type MeasuredFrame } from "@/lib/frame-assets";
import { measureFrame } from "@/lib/measure-frame";

export const dynamic = "force-dynamic";

const FRAMES_DIR = path.join("public", "frames");

// Measurements keyed by file and modification time: replacing a PNG re-measures it.
const cache = new Map<string, MeasuredFrame>();

/** Which Apple bezels the user has added to public/frames/, measured. */
export async function GET() {
  const frames: Partial<Record<FrameFile, MeasuredFrame>> = {};
  const errors: Partial<Record<FrameFile, string>> = {};
  await Promise.all(FRAME_FILES.map(async (name) => {
    const file = path.join(process.cwd(), FRAMES_DIR, `${name}.png`);
    try {
      const stat = await fs.stat(file);
      const version = Math.round(stat.mtimeMs).toString(36);
      const key = `${name}:${version}:${stat.size}`;
      let frame = cache.get(key);
      if (!frame) {
        frame = await measureFrame(await fs.readFile(file), `/frames/${name}.png?v=${version}`);
        cache.set(key, frame);
      }
      frames[name] = frame;
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return;
      errors[name] = `public/frames/${name}.png: ${error instanceof Error ? error.message : String(error)}`;
    }
  }));
  return NextResponse.json({ ok: true, frames, errors }, { headers: { "Cache-Control": "no-store" } });
}
