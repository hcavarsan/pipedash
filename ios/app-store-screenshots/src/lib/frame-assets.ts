// Device frame geometry from Apple's product bezels.
//
// Apple's bezels (developer.apple.com/design/resources, Product Bezels) ship in
// public/frames/; swapping a file changes the finish (see README). Apple
// licenses them for mock-ups of apps for Apple platforms. /api/frames measures
// each PNG's transparent screen cutout and the editor draws the screenshot under
// the real bezel. Without a file, iPhone Duo uses a drawn frame whose screen has
// the exact capture aspect, and every other device keeps its built-in frame.
// Android and CarPlay never use Apple bezels.
import { DUO_INNER_SCREEN, DUO_OUTER_SCREEN } from "./constants";
import type { Device } from "./types";

export type DuoDevice = Extract<Device, `duo-${string}`>;

/** File names (without .png) looked for in public/frames/. */
export const FRAME_FILES = [
  "iphone-portrait",
  "ipad-portrait",
  "watch",
  "tv",
  "mac",
  "duo-outer-portrait",
  "duo-outer-landscape",
  "duo-inner-portrait",
  "duo-inner-landscape",
] as const;
export type FrameFile = (typeof FRAME_FILES)[number];

const DUO_SOURCES: Record<DuoDevice, { file: FrameFile; rotateFrom?: FrameFile }> = {
  "duo-outer": { file: "duo-outer-portrait" },
  "duo-outer-landscape": { file: "duo-outer-landscape", rotateFrom: "duo-outer-portrait" },
  "duo-inner": { file: "duo-inner-portrait" },
  "duo-inner-landscape": { file: "duo-inner-landscape", rotateFrom: "duo-inner-portrait" },
};

/**
 * Which file draws each device, and which portrait file can be turned instead.
 * App Store creatives show an iPhone, so they use the iPhone bezel.
 */
const FRAME_SOURCES: Partial<Record<Device, { file: FrameFile; rotateFrom?: FrameFile }>> = {
  iphone: { file: "iphone-portrait" },
  ipad: { file: "ipad-portrait" },
  watchos: { file: "watch" },
  tvos: { file: "tv" },
  mac: { file: "mac" },
  "creative-universal": { file: "iphone-portrait" },
  "creative-header": { file: "iphone-portrait" },
  "creative-search": { file: "iphone-portrait" },
  ...DUO_SOURCES,
};

type PixelRect = { x: number; y: number; w: number; h: number };

/** A bezel PNG measured by /api/frames: its size and transparent screen cutout. */
export type MeasuredFrame = {
  src: string;
  width: number;
  height: number;
  screen: PixelRect;
  /** Largest corner radius of the cutout in image pixels. */
  radius: number;
  /** Each corner's radius in image pixels: top-left, top-right, bottom-right, bottom-left. */
  radii?: [number, number, number, number];
};

export type FrameGeometry = {
  /** Frame width / height. */
  aspect: number;
  /** Screen rect as percentages of the frame. */
  screen: { L: number; T: number; W: number; H: number };
  /** Screen corner radius as percentages of the screen's width and height. */
  radius: { x: number; y: number };
  /** CSS border-radius clipping the capture to a bezel cutout whose corners differ. */
  clip?: string;
  /** Screen width / height. */
  screenAspect: number;
  /** The real bezel, drawn over the screen. `rotate` turns a portrait file to landscape. */
  image?: { src: string; rotate: boolean };
  /**
   * Camera drawn on the stand-in frame. The outer display has a front camera in
   * its top trailing corner; the inner display's FaceTime camera sits under the
   * screen, so nothing is drawn there (nor a crease: captures show none).
   */
  camera: "corner" | "none";
};

let measured: Partial<Record<FrameFile, MeasuredFrame>> = {};

/** Install the frames measured by /api/frames. Call before rendering any deck. */
export function setFrameAssets(next: Partial<Record<FrameFile, MeasuredFrame>>) {
  measured = { ...next };
}

/** Bezel image URLs in use, so they can be preloaded for export. */
export function frameAssetPaths(device?: Device): string[] {
  // A Duo deck also draws its companion display in "Folded + open".
  const devices = (device
    ? isDuoDevice(device) ? [device, DUO_COMPANION[device]] : [device]
    : Object.keys(FRAME_SOURCES)) as Device[];
  return devices.flatMap((d) => {
    const image = bezelGeometry(d)?.image;
    return image ? [image.src] : [];
  });
}

export function isDuoDevice(device: Device): device is DuoDevice {
  return Object.hasOwn(DUO_SOURCES, device);
}

/**
 * Frame geometry for a device drawn under an Apple bezel: always for iPhone Duo
 * (measured or drawn), otherwise only when the user has added that bezel.
 */
export function bezelGeometry(device: Device): FrameGeometry | null {
  if (isDuoDevice(device)) return duoGeometry(device);
  const source = FRAME_SOURCES[device];
  const frame = source && measured[source.file];
  return frame ? fromMeasured(frame, false, "none") : null;
}

export function duoGeometry(device: DuoDevice): FrameGeometry {
  const { file, rotateFrom } = DUO_SOURCES[device];
  const inner = device.startsWith("duo-inner");
  const camera = inner ? "none" : "corner";
  const direct = measured[file];
  if (direct) return fromMeasured(direct, false, camera);
  const turned = rotateFrom ? measured[rotateFrom] : undefined;
  if (turned) return fromMeasured(turned, true, camera);
  const portrait = inner ? DUO_INNER_SCREEN : DUO_OUTER_SCREEN;
  return drawnGeometry(device.endsWith("-landscape") ? 1 / portrait : portrait, inner, camera);
}

function fromMeasured(frame: MeasuredFrame, rotate: boolean, camera: FrameGeometry["camera"]): FrameGeometry {
  // Turning counter-clockwise maps image point (x, y) to (y, width - x).
  const width = rotate ? frame.height : frame.width;
  const height = rotate ? frame.width : frame.height;
  const s = frame.screen;
  const screen = rotate ? { x: s.y, y: frame.width - (s.x + s.w), w: s.h, h: s.w } : s;
  return {
    aspect: width / height,
    screen: {
      L: (screen.x / width) * 100,
      T: (screen.y / height) * 100,
      W: (screen.w / width) * 100,
      H: (screen.h / height) * 100,
    },
    radius: { x: (frame.radius / screen.w) * 100, y: (frame.radius / screen.h) * 100 },
    clip: frame.radii && cornerClip(frame.radii, rotate, screen.w, screen.h),
    screenAspect: screen.w / screen.h,
    image: { src: frame.src, rotate },
    camera,
  };
}

// Clips the capture to the cutout's own corners, so a square corner shows the
// capture to the edge and a large one never pokes past the bezel's body.
function cornerClip(radii: [number, number, number, number], rotate: boolean, w: number, h: number) {
  // Turning counter-clockwise moves each corner one place round: the old
  // top-right becomes the top-left, and so on.
  const [tl, tr, br, bl] = rotate ? [radii[1], radii[2], radii[3], radii[0]] : radii;
  const xs = [tl, tr, br, bl].map((r) => `${(r / w) * 100}%`).join(" ");
  const ys = [tl, tr, br, bl].map((r) => `${(r / h) * 100}%`).join(" ");
  return `${xs} / ${ys}`;
}

// Drawn stand-in: an even bezel around a screen of exactly the capture aspect.
function drawnGeometry(screenAspect: number, inner: boolean, camera: FrameGeometry["camera"]): FrameGeometry {
  const sw = screenAspect;
  const sh = 1;
  const short = Math.min(sw, sh);
  const bezel = short * 0.028;
  const fw = sw + bezel * 2;
  const fh = sh + bezel * 2;
  const r = short * (inner ? 0.07 : 0.1);
  return {
    aspect: fw / fh,
    screen: { L: (bezel / fw) * 100, T: (bezel / fh) * 100, W: (sw / fw) * 100, H: (sh / fh) * 100 },
    radius: { x: (r / sw) * 100, y: (r / sh) * 100 },
    screenAspect,
    camera,
  };
}

/**
 * The same phone in its other state, for the "Folded + open" layout. Opening a
 * closed phone held upright gives the inner display in landscape (the fold runs
 * down the middle); opening it held sideways gives the inner display in portrait.
 */
export const DUO_COMPANION: Record<DuoDevice, DuoDevice> = {
  "duo-outer": "duo-inner-landscape",
  "duo-inner-landscape": "duo-outer",
  "duo-outer-landscape": "duo-inner",
  "duo-inner": "duo-outer-landscape",
};
