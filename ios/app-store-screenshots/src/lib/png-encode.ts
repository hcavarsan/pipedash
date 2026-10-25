"use client";
// Canvas → 24-bit RGB PNG bytes (see png-rgb.ts for why not toDataURL).
// Encoding a full-size export is ~100ms of filtering and deflate, so it runs
// on a small worker pool: exports keep rendering the next screen meanwhile.
// Without workers, or if one fails, the same encoder runs inline.
import { encodeRgbPixels } from "./png-rgb";

type Job = {
  worker: Worker;
  pixels: Uint8ClampedArray;
  width: number;
  height: number;
  resolve: (png: Uint8Array) => void;
  reject: (error: unknown) => void;
  timeout: ReturnType<typeof setTimeout>;
};
type Result = { id: number; png?: Uint8Array; error?: string };

let pool: Worker[] | null = null;
let nextWorker = 0;
let nextId = 0;
const jobs = new Map<number, Job>();

function encodeInline(job: Job) {
  clearTimeout(job.timeout);
  encodeRgbPixels(job.pixels, job.width, job.height).then(job.resolve, job.reject);
}

function retireWorker(worker: Worker) {
  worker.terminate();
  pool = (pool || []).filter((w) => w !== worker);
  for (const [id, job] of jobs) {
    if (job.worker !== worker) continue;
    jobs.delete(id);
    encodeInline(job);
  }
}

function workers(): Worker[] {
  if (pool) return pool;
  pool = [];
  try {
    const size = Math.max(1, Math.min(4, (navigator.hardwareConcurrency || 2) - 1));
    for (let i = 0; i < size; i++) {
      const worker = new Worker(new URL("./png-worker.ts", import.meta.url));
      worker.onmessage = ({ data }: MessageEvent<Result>) => {
        const job = jobs.get(data.id);
        if (!job) return;
        jobs.delete(data.id);
        clearTimeout(job.timeout);
        if (data.png) job.resolve(data.png);
        else encodeInline(job);
      };
      // A worker that fails to load or crashes leaves the pool, and its jobs
      // finish inline instead of leaving the export waiting forever.
      worker.onerror = (event) => {
        event.preventDefault();
        retireWorker(worker);
      };
      worker.onmessageerror = () => retireWorker(worker);
      pool.push(worker);
    }
  } catch {
    // Keep any workers that did start; an empty pool means inline encoding.
  }
  return pool;
}

export function encodeCanvasPng(canvas: HTMLCanvasElement): Promise<Uint8Array> {
  const ctx = canvas.getContext("2d");
  if (!ctx) return Promise.reject(new Error("Canvas 2D context unavailable"));
  const { width, height } = canvas;
  const pixels = ctx.getImageData(0, 0, width, height).data;
  const available = workers();
  if (!available.length) return encodeRgbPixels(pixels, width, height);
  const id = nextId++;
  const worker = available[nextWorker++ % available.length];
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => retireWorker(worker), 15000);
    jobs.set(id, { worker, pixels, width, height, resolve, reject, timeout });
    // Copied, not transferred: the pixels stay here for the inline fallback.
    try { worker.postMessage({ id, pixels, width, height }); }
    catch { retireWorker(worker); }
  });
}
