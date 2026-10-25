import { serveAsset } from "@/lib/serve-asset";

export const dynamic = "force-dynamic";

export async function GET(_req: Request, context: { params: Promise<{ filename: string }> }) {
  const { filename } = await context.params;
  return serveAsset("fonts/imported", filename, { woff2: "font/woff2", woff: "font/woff", ttf: "font/ttf", otf: "font/otf" });
}
