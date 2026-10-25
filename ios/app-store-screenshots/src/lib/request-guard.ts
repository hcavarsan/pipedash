/** Disk writes require JSON and the editor's own origin. Headerless CLI
 * requests remain supported; these checks do not provide network authentication. */
export type GuardFailure = { error: string; status: number };

/**
 * Returns a failure to respond with, or null when the request is allowed.
 * Call at the top of every route handler that writes to disk.
 */
export function rejectCrossSiteWrite(req: Request): GuardFailure | null {
  // 1. Content-Type must be application/json. Not CORS-simple, so a
  //    cross-origin caller is forced into a preflight that fails.
  const contentType = req.headers.get("content-type") || "";
  if (contentType.toLowerCase().split(";")[0].trim() !== "application/json") {
    return { error: "Content-Type must be application/json", status: 415 };
  }

  // 2. Another local app (including a different port) is a different origin.
  const origin = req.headers.get("origin");
  // Next can canonicalize req.url to localhost even when the browser used
  // 127.0.0.1 or a LAN address. Host is the browser's actual request authority.
  const target = new URL(req.url);
  target.host = req.headers.get("host") || target.host;
  if (origin && origin !== target.origin) {
    return { error: "Cross-origin write rejected", status: 403 };
  }

  // 3. Sec-Fetch-Site, when present, must not be cross-site.
  const site = req.headers.get("sec-fetch-site");
  if (site && site !== "same-origin" && site !== "none") {
    return { error: "Cross-site write rejected", status: 403 };
  }

  return null;
}

/** Magic-byte sniff. Returns the real type, or null if it is neither. */
export function sniffImageType(bytes: Buffer): "image/png" | "image/jpeg" | null {
  if (
    bytes.length >= 8 &&
    bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47 &&
    bytes[4] === 0x0d && bytes[5] === 0x0a && bytes[6] === 0x1a && bytes[7] === 0x0a
  ) {
    return "image/png";
  }
  if (bytes.length >= 3 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return "image/jpeg";
  }
  return null;
}
