import sharp from "sharp";
import type { MeasuredFrame } from "./frame-assets";

// Pixels at or below this alpha count as the see-through screen.
const CLEAR_ALPHA = 24;

/**
 * Finds a bezel PNG's screen: the transparent region connected to the image
 * centre. Its bounding box is the screen rect; how far the cutout's corner sits
 * in along the diagonal gives the corner radius. Dynamic Island and camera
 * cut-ins are opaque islands inside that region, so they don't move the box.
 */
export async function measureFrame(bytes: Buffer, src: string): Promise<MeasuredFrame> {
  // Only the alpha channel matters: one byte per pixel instead of four.
  const { data: alpha, info } = await sharp(bytes).ensureAlpha().extractChannel(3).raw().toBuffer({ resolveWithObject: true });
  const { width, height } = info;
  const cx = Math.floor(width / 2);
  const cy = Math.floor(height / 2);
  if (alpha[cy * width + cx] > CLEAR_ALPHA) {
    throw new Error("the centre of the image is not transparent; export the bezel with an empty screen");
  }

  // Scanline flood fill from the centre: each popped seed fills its whole row
  // span, then seeds one pixel per unseen clear run in the rows above and below.
  // A 4K TV cutout is millions of pixels, so nothing is allocated per pixel.
  const seen = new Uint8Array(width * height);
  const open = (i: number) => !seen[i] && alpha[i] <= CLEAR_ALPHA;
  let stack = new Int32Array(1024);
  let top = 0;
  const push = (i: number) => {
    if (top === stack.length) {
      const grown = new Int32Array(stack.length * 2);
      grown.set(stack);
      stack = grown;
    }
    stack[top++] = i;
  };
  push(cy * width + cx);
  let minX = cx, maxX = cx, minY = cy, maxY = cy;
  while (top) {
    const seed = stack[--top];
    if (!open(seed)) continue;
    const y = (seed / width) | 0;
    const row = y * width;
    let left = seed - row;
    let right = left;
    while (left > 0 && open(row + left - 1)) left--;
    while (right < width - 1 && open(row + right + 1)) right++;
    seen.fill(1, row + left, row + right + 1);
    if (left < minX) minX = left;
    if (right > maxX) maxX = right;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
    for (const ny of [y - 1, y + 1]) {
      if (ny < 0 || ny >= height) continue;
      const nrow = ny * width;
      let inRun = false;
      for (let x = left; x <= right; x++) {
        if (open(nrow + x)) {
          if (!inRun) push(nrow + x);
          inRun = true;
        } else {
          inRun = false;
        }
      }
    }
  }
  if (minX === 0 || minY === 0 || maxX === width - 1 || maxY === height - 1) {
    throw new Error("the transparent screen reaches the image edge, so no bezel surrounds it");
  }
  const screen = { x: minX, y: minY, w: maxX - minX + 1, h: maxY - minY + 1 };
  if (screen.w < width * 0.5 || screen.h < height * 0.5) {
    throw new Error("the transparent area at the centre is too small to be the screen");
  }

  // A circular corner of radius r leaves r·(1 − 1/√2) of bezel along the
  // diagonal. Corners can differ (the iPhone Duo's outer display is square on
  // its hinge side), so each is measured on its own.
  const limit = Math.min(screen.w, screen.h) / 2;
  const cornerRadius = (x0: number, y0: number, dx: number, dy: number) => {
    let inset = 0;
    while (inset < limit && !seen[(y0 + dy * inset) * width + x0 + dx * inset]) inset++;
    return Math.round(inset / (1 - Math.SQRT1_2));
  };
  const radii: MeasuredFrame["radii"] = [
    cornerRadius(minX, minY, 1, 1),
    cornerRadius(maxX, minY, -1, 1),
    cornerRadius(maxX, maxY, -1, -1),
    cornerRadius(minX, maxY, 1, -1),
  ];

  return { src, width, height, screen, radius: Math.max(...radii), radii };
}
