import { NextResponse } from "next/server";

/** Bound the stream itself: Content-Length is optional and caller-controlled. */
export async function readJsonBody(req: Request, maxBytes: number): Promise<
  { value: unknown; response?: never } | { value?: never; response: NextResponse }
> {
  // The unread remainder is cancelled. Do not advertise a reusable HTTP/1
  // connection that could reset the client's next (otherwise valid) request.
  const tooLarge = () => NextResponse.json({ ok: false, error: "Request body too large" }, { status: 413, headers: { Connection: "close" } });
  if (Number(req.headers.get("content-length")) > maxBytes) return { response: tooLarge() };
  const reader = req.body?.getReader();
  if (!reader) return { response: NextResponse.json({ ok: false, error: "Invalid JSON" }, { status: 400 }) };
  try {
    const chunks: Uint8Array[] = [];
    let length = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > maxBytes) {
        void reader.cancel().catch(() => undefined);
        return { response: tooLarge() };
      }
      chunks.push(value);
    }
    return { value: JSON.parse(Buffer.concat(chunks).toString("utf8")) };
  } catch {
    return { response: NextResponse.json({ ok: false, error: "Invalid JSON" }, { status: 400 }) };
  } finally {
    reader.releaseLock();
  }
}

export function decodeBase64(value: string): Buffer | null {
  // Buffer.from silently ignores invalid characters and truncated quartets.
  if (!value || value.length % 4 !== 0 || !/^[A-Za-z0-9+/]*={0,2}$/.test(value)) return null;
  const bytes = Buffer.from(value, "base64");
  return bytes.toString("base64") === value ? bytes : null;
}
