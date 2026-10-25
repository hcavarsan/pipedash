// Encodes RGBA pixels as an opaque 24-bit RGB PNG. No DOM access, so it runs
// in the export worker (png-worker.ts) as well as on the main thread.
//
// canvas.toDataURL("image/png") always writes RGBA (colour type 6), even when
// every pixel is opaque. Google Play requires screenshots and the feature
// graphic to be "JPEG or 24-bit PNG (no alpha)", and RGB is also the safe
// choice for App Store Connect, so exports are re-encoded here without the
// alpha channel. Compression uses the browser's native zlib (CompressionStream
// "deflate"), with the standard per-row filter heuristic to keep files small.

const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(bytes: Uint8Array, start: number, end: number) {
  let c = 0xffffffff;
  for (let i = start; i < end; i++) c = CRC_TABLE[(c ^ bytes[i]) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type: string, data: Uint8Array) {
  const out = new Uint8Array(12 + data.length);
  const view = new DataView(out.buffer);
  view.setUint32(0, data.length);
  for (let i = 0; i < 4; i++) out[4 + i] = type.charCodeAt(i);
  out.set(data, 8);
  view.setUint32(8 + data.length, crc32(out, 4, 8 + data.length));
  return out;
}

// Writes one filtered scanline into `row` and returns its sum-of-absolute-
// values score, giving up once it reaches `limit`. `cur` and `prev` carry
// BPP leading zero bytes so the left neighbour never needs a bounds check.
// One loop per filter keeps the per-byte work branch-free; exports encode
// tens of millions of bytes.
const BPP = 3;

function applyFilter(type: number, cur: Uint8Array, prev: Uint8Array, row: Uint8Array, stride: number, limit: number) {
  let score = 0;
  let v = 0;
  switch (type) {
    case 0:
      for (let i = 0; i < stride && score < limit; i++) {
        v = cur[i + BPP];
        row[i] = v;
        score += v < 128 ? v : 256 - v;
      }
      break;
    case 1:
      for (let i = 0; i < stride && score < limit; i++) {
        v = (cur[i + BPP] - cur[i]) & 0xff;
        row[i] = v;
        score += v < 128 ? v : 256 - v;
      }
      break;
    case 2:
      for (let i = 0; i < stride && score < limit; i++) {
        v = (cur[i + BPP] - prev[i + BPP]) & 0xff;
        row[i] = v;
        score += v < 128 ? v : 256 - v;
      }
      break;
    case 3:
      for (let i = 0; i < stride && score < limit; i++) {
        v = (cur[i + BPP] - ((cur[i] + prev[i + BPP]) >> 1)) & 0xff;
        row[i] = v;
        score += v < 128 ? v : 256 - v;
      }
      break;
    default:
      for (let i = 0; i < stride && score < limit; i++) {
        const a = cur[i];
        const b = prev[i + BPP];
        const c = prev[i];
        const p = a + b - c;
        const pa = p > a ? p - a : a - p;
        const pb = p > b ? p - b : b - p;
        const pc = p > c ? p - c : c - p;
        v = (cur[i + BPP] - (pa <= pb && pa <= pc ? a : pb <= pc ? b : c)) & 0xff;
        row[i] = v;
        score += v < 128 ? v : 256 - v;
      }
  }
  return score;
}

// Filtered scanlines: each row is one filter-type byte plus the filtered RGB
// bytes, picking the filter with the smallest sum of absolute values.
function filterRows(rgba: Uint8ClampedArray, width: number, height: number): Uint8Array<ArrayBuffer> {
  const stride = width * BPP;
  const out = new Uint8Array((stride + 1) * height);
  let prev = new Uint8Array(stride + BPP);
  let cur = new Uint8Array(stride + BPP);
  const candidates = Array.from({ length: 5 }, () => new Uint8Array(stride));
  for (let y = 0; y < height; y++) {
    for (let x = 0, s = y * width * 4, d = BPP; x < width; x++, s += 4, d += BPP) {
      cur[d] = rgba[s];
      cur[d + 1] = rgba[s + 1];
      cur[d + 2] = rgba[s + 2];
    }
    let best = 0;
    let bestScore = Infinity;
    for (let type = 0; type < 5; type++) {
      const score = applyFilter(type, cur, prev, candidates[type], stride, bestScore);
      // A filter that hit the limit stopped early, but it lost, so the
      // winner's row is always complete.
      if (score < bestScore) {
        bestScore = score;
        best = type;
      }
    }
    const offset = y * (stride + 1);
    out[offset] = best;
    out.set(candidates[best], offset + 1);
    [prev, cur] = [cur, prev];
  }
  return out;
}

async function deflate(data: Uint8Array<ArrayBuffer>) {
  const stream = new Blob([data]).stream().pipeThrough(new CompressionStream("deflate"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

export async function encodeRgbPixels(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
): Promise<Uint8Array<ArrayBuffer>> {
  const header = new Uint8Array(13);
  const view = new DataView(header.buffer);
  view.setUint32(0, width);
  view.setUint32(4, height);
  header[8] = 8; // bit depth
  header[9] = 2; // colour type: truecolour, no alpha

  const parts = [
    new Uint8Array(SIGNATURE),
    chunk("IHDR", header),
    chunk("IDAT", await deflate(filterRows(pixels, width, height))),
    chunk("IEND", new Uint8Array()),
  ];
  const png = new Uint8Array(parts.reduce((n, part) => n + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    png.set(part, offset);
    offset += part.length;
  }
  return png;
}
