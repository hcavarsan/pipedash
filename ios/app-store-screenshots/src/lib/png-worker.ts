// Export encoding worker: RGBA pixels in, 24-bit RGB PNG bytes out.
import { encodeRgbPixels } from "./png-rgb";

type Job = { id: number; pixels: Uint8ClampedArray; width: number; height: number };

self.onmessage = async ({ data }: MessageEvent<Job>) => {
  try {
    const png = await encodeRgbPixels(data.pixels, data.width, data.height);
    self.postMessage({ id: data.id, png }, { transfer: [png.buffer] });
  } catch (error) {
    self.postMessage({ id: data.id, error: error instanceof Error ? error.message : String(error) });
  }
};
