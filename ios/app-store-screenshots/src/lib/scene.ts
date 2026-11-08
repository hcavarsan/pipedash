import { SCREENSHOT_FONTS } from './fonts'
import { THEMES } from './themes'
import type { Callout, Look, Scene, SceneBackdrop, SceneDecoration, ScreenshotFontId, SlideLayout } from './types'

// The classic look every deck had before scenes existed. A project without a
// `scene` renders exactly like this, so older decks don't shift.
export const DEFAULT_SCENE: Scene = {
  backdrop: 'gradient',
  span: false,
  decoration: 'blobs',
  shadow: 0,
  glow: 0,
  tilt: 0,
  headlineWeight: 700,
  headlineScale: 1,
  headlineCase: 'as-typed',
  captionAlign: 'auto',
}

export const BACKDROPS: { id: SceneBackdrop; name: string }[] = [
  { id: 'gradient', name: 'Gradient' },
  { id: 'solid', name: 'Solid' },
  { id: 'aurora', name: 'Aurora' },
  { id: 'spotlight', name: 'Spotlight' },
  { id: 'grid', name: 'Grid' },
  { id: 'dots', name: 'Dots' },
  { id: 'lines', name: 'Ruled' },
]

export const DECORATIONS: { id: SceneDecoration; name: string }[] = [
  { id: 'blobs', name: 'Blobs' },
  { id: 'rings', name: 'Rings' },
  { id: 'sparkles', name: 'Sparkles' },
  { id: 'none', name: 'None' },
]

export const TILT_LIMIT = 30
export const CALLOUT_ZOOM_MIN = 1.5
export const CALLOUT_ZOOM_MAX = 5
export const DEFAULT_CALLOUT: Callout = { focusX: 0.5, focusY: 0.32, zoom: 2, shape: 'circle' }

const LAYOUTS: ReadonlySet<SlideLayout> = new Set([
  'hero',
  'device-bottom',
  'device-top',
  'two-devices',
  'no-device',
  'split-landscape',
  'feature-graphic',
])

const record = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === 'object' && !Array.isArray(value)

function clampNumber(value: unknown, min: number, max: number, fallback: number) {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    return fallback
  }
  return Math.min(max, Math.max(min, value))
}

const isFontId = (value: unknown): value is ScreenshotFontId =>
  typeof value === 'string' && Object.hasOwn(SCREENSHOT_FONTS, value)

const isLayout = (value: unknown): value is SlideLayout => [...LAYOUTS].some((layout) => layout === value)

export function sceneOf(scene: Scene | undefined): Scene {
  return scene ?? DEFAULT_SCENE
}

/** Normalise a scene from disk or a script; unknown values fall back to the classic look. */
export function cleanScene(value: unknown): Scene | undefined {
  if (!record(value)) {
    return undefined
  }
  return {
    backdrop: BACKDROPS.find((b) => b.id === value.backdrop)?.id ?? DEFAULT_SCENE.backdrop,
    span: value.span === true,
    decoration: DECORATIONS.find((d) => d.id === value.decoration)?.id ?? DEFAULT_SCENE.decoration,
    shadow: Math.round(clampNumber(value.shadow, 0, 100, DEFAULT_SCENE.shadow)),
    glow: Math.round(clampNumber(value.glow, 0, 100, DEFAULT_SCENE.glow)),
    tilt: Math.round(clampNumber(value.tilt, -TILT_LIMIT, TILT_LIMIT, DEFAULT_SCENE.tilt)),
    headlineWeight: Math.round(clampNumber(value.headlineWeight, 300, 900, DEFAULT_SCENE.headlineWeight) / 100) * 100,
    headlineScale: Math.round(clampNumber(value.headlineScale, 0.5, 2, DEFAULT_SCENE.headlineScale) * 20) / 20,
    headlineCase: value.headlineCase === 'upper' ? 'upper' : 'as-typed',
    captionAlign: value.captionAlign === 'left' || value.captionAlign === 'center' ? value.captionAlign : 'auto',
  }
}

export function cleanCallout(value: unknown): Callout | undefined {
  if (!record(value)) {
    return undefined
  }
  return {
    focusX: clampNumber(value.focusX, 0, 1, DEFAULT_CALLOUT.focusX),
    focusY: clampNumber(value.focusY, 0, 1, DEFAULT_CALLOUT.focusY),
    zoom: clampNumber(value.zoom, CALLOUT_ZOOM_MIN, CALLOUT_ZOOM_MAX, DEFAULT_CALLOUT.zoom),
    shape: value.shape === 'rounded' ? 'rounded' : 'circle',
  }
}

export function cleanLook(value: unknown): Look | undefined {
  if (!record(value) || typeof value.id !== 'string' || !value.id.trim()) {
    return undefined
  }
  const scene = cleanScene(value.scene)
  if (!scene) {
    return undefined
  }
  const layouts = Array.isArray(value.layouts) ? value.layouts.filter(isLayout) : []
  return {
    id: value.id,
    name: typeof value.name === 'string' && value.name.trim() ? value.name : 'Saved look',
    direction: typeof value.direction === 'string' ? value.direction : '',
    themeId: typeof value.themeId === 'string' && THEMES[value.themeId] ? value.themeId : 'clean-light',
    fontId: isFontId(value.fontId) ? value.fontId : 'template-default',
    scene,
    layouts: layouts.length > 0 ? layouts : ['hero'],
    inverted:
      Array.isArray(value.inverted) && value.inverted.length > 0
        ? value.inverted.map((item) => item === true)
        : [false],
  }
}

export function sameScene(a: Scene, b: Scene): boolean {
  return Object.keys(DEFAULT_SCENE).every((key) => Reflect.get(a, key) === Reflect.get(b, key))
}
