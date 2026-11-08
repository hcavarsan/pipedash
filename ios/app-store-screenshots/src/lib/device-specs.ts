import type { Device, Orientation, Platform } from './types'

// ---------- Canvas dimensions (design at largest required resolution) ----------
export const CANVAS: Record<Device, { w: number; h: number; wL?: number; hL?: number }> = {
  iphone: { w: 1320, h: 2868 },
  ipad: { w: 2064, h: 2752 },
  // iPhone Duo, exactly as App Store Connect lists them (checked 7 Oct 2026).
  // The displays are not scalings of each other, so each is its own canvas.
  'duo-outer': { w: 1398, h: 2034 },
  'duo-outer-landscape': { w: 2034, h: 1398 },
  'duo-inner': { w: 2007, h: 2853 },
  'duo-inner-landscape': { w: 2853, h: 2007 },
  // Apple TV is 16:9 landscape-only. Design at 4K; 1920x1080 is a clean 2x downscale.
  tvos: { w: 3840, h: 2160 },
  // Apple Watch: design at the largest slot Apple accepts (Ultra 422x514) so every
  // smaller size is a downscale rather than an upscale.
  watchos: { w: 422, h: 514 },
  // CarPlay has NO App Store screenshot slot of its own - see EXPORT_SIZES below.
  // It is submitted in an iPhone slot, which accepts landscape, so the canvas is
  // the 6.9" iPhone size turned sideways to fit a wide head unit.
  carplay: { w: 2868, h: 1320 },
  // Mac App Store is landscape-only 16:10. Design at 2880x1800; every other Mac
  // slot is an exact 16:10 downscale.
  mac: { w: 2880, h: 1800 },
  android: { w: 1080, h: 1920 },
  'android-7': { w: 1200, h: 1920, wL: 1920, hL: 1200 },
  'android-10': { w: 1600, h: 2560, wL: 2560, hL: 1600 },
  'feature-graphic': { w: 1024, h: 500 },
  // App Store creative assets, the size of Apple's own templates.
  'creative-universal': { w: 5244, h: 2950 },
  'creative-header': { w: 3840, h: 1646 },
  'creative-search': { w: 3840, h: 2560 },
}

// ---------- Export sizes per device ----------
export type ExportSize = { label: string; w: number; h: number }

export const EXPORT_SIZES: Record<Device, ExportSize[]> = {
  iphone: [
    { label: '6.9"', w: 1320, h: 2868 },
    { label: '6.5"', w: 1284, h: 2778 },
    { label: '6.3"', w: 1206, h: 2622 },
    { label: '6.1"', w: 1125, h: 2436 },
  ],
  ipad: [
    { label: '13" iPad', w: 2064, h: 2752 },
    { label: '12.9" iPad Pro', w: 2048, h: 2732 },
  ],
  // App Store Connect lists exactly one size per Duo display and orientation.
  'duo-outer': [{ label: 'Outer portrait', w: 1398, h: 2034 }],
  'duo-outer-landscape': [{ label: 'Outer landscape', w: 2034, h: 1398 }],
  'duo-inner': [{ label: 'Inner portrait', w: 2007, h: 2853 }],
  'duo-inner-landscape': [{ label: 'Inner landscape', w: 2853, h: 2007 }],
  // App Store Connect display type APP_APPLE_TV. Verified 18 Aug 2026 via
  // `asc screenshots sizes --all`; these are the only accepted dimensions.
  tvos: [
    { label: '4K (3840 x 2160)', w: 3840, h: 2160 },
    { label: 'HD (1920 x 1080)', w: 1920, h: 1080 },
  ],
  // Apple Watch display types, all verified the same way:
  //   APP_WATCH_ULTRA 410x502 + 422x514 | SERIES_10 416x496
  //   SERIES_7 396x484 | SERIES_4 368x448 | SERIES_3 312x390
  watchos: [
    { label: 'Ultra (422 x 514)', w: 422, h: 514 },
    { label: 'Ultra (410 x 502)', w: 410, h: 502 },
    { label: 'Series 10 (416x496)', w: 416, h: 496 },
    { label: 'Series 7 (396 x 484)', w: 396, h: 484 },
    { label: 'Series 4 (368 x 448)', w: 368, h: 448 },
    { label: 'Series 3 (312 x 390)', w: 312, h: 390 },
  ],
  // 🚨 CarPlay has NO display type in App Store Connect - confirmed against its own
  // metadata, not documentation: `asc screenshots sizes --all` lists APPLE_TV,
  // VISION_PRO, DESKTOP, IPAD*, IPHONE*, WATCH* and nothing for CarPlay. A CarPlay
  // app ships inside its iPhone app, so a CarPlay shot is submitted in an iPhone
  // slot. These are therefore the iPhone sizes on purpose, in landscape (every
  // iPhone slot accepts both orientations).
  carplay: [
    { label: '6.9" landscape', w: 2868, h: 1320 },
    { label: '6.5" landscape', w: 2778, h: 1284 },
    { label: '6.3" landscape', w: 2622, h: 1206 },
    { label: '6.1" landscape', w: 2436, h: 1125 },
  ],
  // App Store Connect display type APP_DESKTOP (Mac App Store). These four
  // 16:10 sizes are the only accepted dimensions.
  mac: [
    { label: '2880 x 1800', w: 2880, h: 1800 },
    { label: '2560 x 1600', w: 2560, h: 1600 },
    { label: '1440 x 900', w: 1440, h: 900 },
    { label: '1280 x 800', w: 1280, h: 800 },
  ],
  android: [{ label: 'Phone', w: 1080, h: 1920 }],
  'android-7': [{ label: '7" Portrait', w: 1200, h: 1920 }],
  'android-10': [{ label: '10" Portrait', w: 1600, h: 2560 }],
  'feature-graphic': [{ label: 'Feature Graphic', w: 1024, h: 500 }],
  // One universal asset covers both the product page header and search results.
  'creative-universal': [{ label: 'Universal', w: 5244, h: 2950 }],
  'creative-header': [{ label: 'Header', w: 3840, h: 1646 }],
  // Search results take any 3:2 size from 1920x1280 to 3840x2560.
  'creative-search': [
    { label: 'Search (3840 x 2560)', w: 3840, h: 2560 },
    { label: 'Search (1920 x 1280)', w: 1920, h: 1280 },
  ],
}

// Landscape sizes (tablets only)
export const EXPORT_SIZES_LANDSCAPE: Partial<Record<Device, ExportSize[]>> = {
  'android-7': [{ label: '7" Landscape', w: 1920, h: 1200 }],
  'android-10': [{ label: '10" Landscape', w: 2560, h: 1600 }],
}

// Targets whose two orientations are separate decks (and separate devices),
// so portrait and landscape keep independent compositions.
export const ORIENTATION_PAIRS: Partial<Record<Device, Record<Orientation, Device>>> = {
  'duo-outer': { portrait: 'duo-outer', landscape: 'duo-outer-landscape' },
  'duo-outer-landscape': { portrait: 'duo-outer', landscape: 'duo-outer-landscape' },
  'duo-inner': { portrait: 'duo-inner', landscape: 'duo-inner-landscape' },
  'duo-inner-landscape': { portrait: 'duo-inner', landscape: 'duo-inner-landscape' },
}

export function supportsLandscape(device: Device): boolean {
  return device in EXPORT_SIZES_LANDSCAPE || device in ORIENTATION_PAIRS
}

/** The device holding `device`'s deck in `orientation`; itself when orientation is just a canvas flip. */
export function deviceForOrientation(device: Device, orientation: Orientation): Device {
  return ORIENTATION_PAIRS[device]?.[orientation] ?? device
}

/** Orientation a paired device is fixed to; otherwise the project's orientation. */
export function orientationOf(device: Device, orientation: Orientation): Orientation {
  const pair = ORIENTATION_PAIRS[device]
  if (!pair) {
    return orientation
  }
  return pair.landscape === device ? 'landscape' : 'portrait'
}

export type CreativeDevice = 'creative-universal' | 'creative-header' | 'creative-search'

export function isCreative(device: Device): device is CreativeDevice {
  return device.startsWith('creative-')
}

type Box = { x: number; y: number; w: number; h: number }

// "Art Safe Area" layer of Apple's static creative asset templates, measured in
// canvas pixels from the Photoshop files linked on developer.apple.com/app-store/
// asset-best-practices (downloaded 7 Oct 2026). Keep focal art and copy inside.
// `crops` preview how the universal asset may be trimmed in each placement.
// Apple does not publish that crop, so these are centred cover crops at each
// placement's own aspect: guides only, never applied to the export.
export const CREATIVE_SPECS: Record<CreativeDevice, { safe: Box; crops: { label: string; aspect: number }[] }> = {
  'creative-universal': {
    safe: { x: 1921, y: 660, w: 1402, h: 962 },
    crops: [
      { label: 'Header crop', aspect: 3840 / 1646 },
      { label: 'Search crop', aspect: 3 / 2 },
    ],
  },
  'creative-header': { safe: { x: 1097, y: 493, w: 1646, h: 661 }, crops: [] },
  'creative-search': { safe: { x: 836, y: 765, w: 2168, h: 1030 }, crops: [] },
}

/** Centred cover crop of a `cW`×`cH` canvas at `aspect` (w/h). */
export function centredCrop(cW: number, cH: number, aspect: number): Box {
  const w = Math.min(cW, cH * aspect)
  const h = w / aspect
  return { x: (cW - w) / 2, y: (cH - h) / 2, w, h }
}

export function getExportSizes(device: Device, orientation: Orientation): ExportSize[] {
  if (orientation === 'landscape') {
    return EXPORT_SIZES_LANDSCAPE[device] || EXPORT_SIZES[device]
  }
  return EXPORT_SIZES[device]
}

// ---------- Frame aspect ratios ----------
export const MK_RATIO = 1022 / 2082 // iPhone PNG mockup
export const TAB_P_RATIO = 0.667 // tablet portrait
export const TAB_L_RATIO = 1.5 // tablet landscape
export const IPAD_RATIO = 0.77 // iPad
export const TV_RATIO = 16 / 9 // Apple TV - landscape only
export const WATCH_RATIO = 422 / 514 // Apple Watch Ultra, the largest accepted slot
// CarPlay head units vary by vehicle and Apple ships five presets in CarPlay
// Simulator.app/Contents/Resources/VehicleConfigs: Minimum 748x456, Standard 800x480,
// Widescreen 1920x720, Portrait 900x1200, Standard Video Playback 1920x1080.
// "Standard" is the default here; change this constant to target another.
export const CARPLAY_RATIO = 800 / 480
// Mac window: a 16:10 content area under a title bar MAC_TITLE_BAR x the content
// height tall, so a 16:10 capture (the Mac App Store's own aspect) fills the
// window without being cropped.
export const MAC_TITLE_BAR = 0.045
export const MAC_RATIO = 16 / (10 * (1 + MAC_TITLE_BAR))

// iPhone Duo capture aspects (screen w/h), from the exact sizes above.
export const DUO_OUTER_SCREEN = 1398 / 2034
export const DUO_INNER_SCREEN = 2007 / 2853

// iPhone mockup screen overlay (pre-measured)
export const PHONE_SCREEN = {
  L: (52 / 1022) * 100,
  T: (46 / 2082) * 100,
  W: (918 / 1022) * 100,
  H: (1990 / 2082) * 100,
  RX: (126 / 918) * 100,
  RY: (126 / 1990) * 100,
}

// ---------- Width formula helpers ----------
export function phoneW(cW: number, cH: number, clamp = 0.84): number {
  return Math.min(clamp, 0.72 * (cH / cW) * MK_RATIO)
}
export function phoneWSmall(cW: number, cH: number): number {
  return phoneW(cW, cH, 0.66)
}
export function tabletPW(cW: number, cH: number, clamp = 0.8): number {
  return Math.min(clamp, 0.72 * (cH / cW) * TAB_P_RATIO)
}
export function tabletLW(cW: number, cH: number, clamp = 0.62): number {
  return Math.min(clamp, 0.75 * (cH / cW) * TAB_L_RATIO)
}
export function ipadW(cW: number, cH: number, clamp = 0.75): number {
  return Math.min(clamp, 0.72 * (cH / cW) * IPAD_RATIO)
}
// Clamped low so a 16:9 device clears the 0.28-height caption block on a 16:9 canvas.
export function tvW(cW: number, cH: number, clamp = 0.58): number {
  return Math.min(clamp, 0.72 * (cH / cW) * TV_RATIO)
}
export function watchW(cW: number, cH: number, clamp = 0.52): number {
  return Math.min(clamp, 0.72 * (cH / cW) * WATCH_RATIO)
}
// Height-bound on the wide canvas: the head unit must clear the caption block.
export function carPlayW(cW: number, cH: number, clamp = 0.86): number {
  return Math.min(clamp, 0.58 * (cH / cW) * CARPLAY_RATIO)
}
// Width that makes a frame of `aspect` take `heightFrac` of the canvas height.
export function frameFitW(aspect: number, heightFrac: number, clamp: number): (cW: number, cH: number) => number {
  return (cW: number, cH: number) => Math.min(clamp, heightFrac * (cH / cW) * aspect)
}
// Contained like the TV: height-bound so the window clears the caption block
// above or below it on the 16:10 canvas.
export function macW(cW: number, cH: number, clamp = 0.86): number {
  return Math.min(clamp, 0.58 * (cH / cW) * MAC_RATIO)
}

// Toolbar platform tabs, in menu order. The platform is also the top-level
// export folder (ios/…, macos/…, android/…). Mac gets its own tab because App
// Store Connect lists macOS as a separate platform with its own screenshot set.
export const PLATFORM_DEVICES: Record<Platform, Device[]> = {
  ios: [
    'iphone',
    'ipad',
    'duo-outer',
    'duo-outer-landscape',
    'duo-inner',
    'duo-inner-landscape',
    'tvos',
    'watchos',
    'carplay',
    'creative-universal',
    'creative-header',
    'creative-search',
  ],
  macos: ['mac'],
  android: ['android', 'android-7', 'android-10', 'feature-graphic'],
}

export const DEVICE_LABEL: Record<Device, string> = {
  iphone: 'iPhone',
  ipad: 'iPad',
  'duo-outer': 'iPhone Duo outer',
  'duo-outer-landscape': 'iPhone Duo outer (landscape)',
  'duo-inner': 'iPhone Duo inner',
  'duo-inner-landscape': 'iPhone Duo inner (landscape)',
  tvos: 'Apple TV',
  watchos: 'Apple Watch',
  carplay: 'CarPlay (iPhone slot)',
  mac: 'Mac',
  android: 'Android Phone',
  'android-7': 'Android 7" Tablet',
  'android-10': 'Android 10" Tablet',
  'feature-graphic': 'Feature Graphic',
  'creative-universal': 'Creative: universal',
  'creative-header': 'Creative: header',
  'creative-search': 'Creative: search',
}
