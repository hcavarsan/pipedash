import { serveAsset } from "@/lib/serve-asset";

export const dynamic = "force-dynamic";

export async function GET(_req: Request, context: { params: Promise<{ filename: string }> }) {
  const { filename } = await context.params;
  return serveAsset("screenshots/uploaded", filename, { png: "image/png", jpg: "image/jpeg" });
}
