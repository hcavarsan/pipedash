import { cleanHexColor } from './clean-hex-color'
import { cleanImportedFont } from './clean-imported-font'
import { PROJECT_SCHEMA_VERSION } from './constants'
import { DEFAULT_PROJECT } from './defaults'
import { DEVICE_LABEL, orientationOf, supportsLandscape } from './device-specs'
import { DEFAULT_SCREENSHOT_FONT_ID, SCREENSHOT_FONTS } from './fonts'
import { LAYOUT_LABEL } from './layout-labels'
import { coerceLocalized } from './locale'
import { isRecord, projectValidationError } from './project-validation'
import { cleanCallout, cleanLook, cleanScene } from './scene'
import type {
  BuiltInElementId,
  Device,
  ElementTransform,
  ImageElement,
  Look,
  ProjectState,
  ScreenshotFontId,
  Slide,
  SlideLayout,
  SlideTypography,
  TextElement,
} from './types'
import { cleanTypography } from './typography'

const finiteNumber = (value: unknown): number | undefined =>
  typeof value === 'number' && Number.isFinite(value) ? value : undefined

const isDevice = (value: string): value is Device => Object.hasOwn(DEVICE_LABEL, value)
const isLayout = (value: unknown): value is SlideLayout =>
  typeof value === 'string' && Object.hasOwn(LAYOUT_LABEL, value)
const isFontId = (value: string): value is ScreenshotFontId => Object.hasOwn(SCREENSHOT_FONTS, value)
const BUILT_IN_IDS: readonly BuiltInElementId[] = ['caption', 'device', 'deviceSecondary', 'callout']
const isBuiltInId = (value: string): value is BuiltInElementId => BUILT_IN_IDS.some((id) => id === value)
const FADE_EDGES = ['top', 'bottom', 'left', 'right'] as const
const fadeEdge = (value: unknown): (typeof FADE_EDGES)[number] | undefined => FADE_EDGES.find((edge) => edge === value)

function cleanTransform(value: unknown): ElementTransform | undefined {
  if (!isRecord(value)) {
    return undefined
  }
  const x = finiteNumber(value.x)
  const y = finiteNumber(value.y)
  const width = finiteNumber(value.width)
  const height = finiteNumber(value.height)
  if (x === undefined || y === undefined || width === undefined || height === undefined) {
    return undefined
  }
  const rotation = finiteNumber(value.rotation)
  const zIndex = finiteNumber(value.zIndex)
  return {
    x,
    y,
    width: Math.max(1, width),
    height: Math.max(1, height),
    ...(rotation === undefined ? {} : { rotation }),
    ...(zIndex === undefined ? {} : { zIndex }),
  }
}

function cleanTextElement(value: unknown): TextElement | undefined {
  if (!isRecord(value) || typeof value.id !== 'string' || !value.id.trim()) {
    return undefined
  }
  const transform = cleanTransform(value.transform)
  if (!transform) {
    return undefined
  }
  const { color, align } = value
  const fontSize = finiteNumber(value.fontSize)
  const fontWeight = finiteNumber(value.fontWeight)
  return {
    id: value.id,
    text: coerceLocalized(value.text),
    transform,
    ...(fontSize === undefined ? {} : { fontSize }),
    ...(fontWeight === undefined ? {} : { fontWeight }),
    ...(typeof color === 'string' ? { color } : {}),
    ...(align === 'left' || align === 'center' || align === 'right' ? { align } : {}),
  }
}

function cleanFade(value: unknown): ImageElement['fade'] {
  if (!isRecord(value)) {
    return undefined
  }
  const edge = fadeEdge(value.edge)
  const amount = finiteNumber(value.amount)
  if (edge === undefined || amount === undefined) {
    return undefined
  }
  return { edge, amount: Math.max(0, Math.min(100, amount)) }
}

function cleanImageElement(value: unknown): ImageElement | undefined {
  if (!isRecord(value) || typeof value.id !== 'string' || !value.id.trim() || typeof value.src !== 'string') {
    return undefined
  }
  const transform = cleanTransform(value.transform)
  if (!transform) {
    return undefined
  }
  const fade = cleanFade(value.fade)
  return {
    id: value.id,
    src: value.src,
    transform,
    ...(value.fit === 'cover' || value.fit === 'contain' ? { fit: value.fit } : {}),
    ...(fade ? { fade } : {}),
  }
}

// Early builds had a "Classic Serif" that rendered exactly like Georgia.
const LEGACY_FONT_IDS: Record<string, ScreenshotFontId> = { 'classic-serif': 'template-serif' }

function cleanFontId(value: unknown, hasImportedFont: boolean): ScreenshotFontId {
  if (typeof value !== 'string') {
    return DEFAULT_SCREENSHOT_FONT_ID
  }
  const id = LEGACY_FONT_IDS[value] ?? value
  if (!isFontId(id)) {
    return DEFAULT_SCREENSHOT_FONT_ID
  }
  // "Imported font" without a file behind it would render the fallback.
  if (id === 'self-hosted' && !hasImportedFont) {
    return DEFAULT_SCREENSHOT_FONT_ID
  }
  return id
}

function cleanTransforms(value: unknown): Slide['transforms'] {
  if (!isRecord(value)) {
    return undefined
  }
  const transforms: NonNullable<Slide['transforms']> = {}
  for (const [id, raw] of Object.entries(value)) {
    const transform = cleanTransform(raw)
    if (transform && isBuiltInId(id)) {
      transforms[id] = transform
    }
  }
  return Object.keys(transforms).length > 0 ? transforms : undefined
}

function cleanList<T>(value: unknown, clean: (item: unknown) => T | undefined): T[] | undefined {
  if (!Array.isArray(value)) {
    return undefined
  }
  const items = value.map(clean).filter((item): item is T => item !== undefined)
  return items.length > 0 ? items : undefined
}

function typographyOf(value: unknown): SlideTypography | undefined {
  if (!isRecord(value)) {
    return undefined
  }
  const labelScale = finiteNumber(value.labelScale)
  const headlineScale = finiteNumber(value.headlineScale)
  const appNameScale = finiteNumber(value.appNameScale)
  return cleanTypography({
    ...(labelScale === undefined ? {} : { labelScale }),
    ...(headlineScale === undefined ? {} : { headlineScale }),
    ...(appNameScale === undefined ? {} : { appNameScale }),
  })
}

// Migrate older projects into the current schema while keeping legacy decks
// visually stable until they explicitly opt into connected canvas.
function migrateSlide(raw: unknown, device: string): Slide | undefined {
  if (!isRecord(raw) || typeof raw.id !== 'string' || !isLayout(raw.layout)) {
    return undefined
  }
  const isBanner = device === 'feature-graphic'
  const backgroundColor = cleanHexColor(raw.backgroundColor)
  const typography = typographyOf(raw.typography)
  const transforms = isBanner ? undefined : cleanTransforms(raw.transforms)
  const textElements = cleanList(raw.textElements, cleanTextElement)
  const imageElements = cleanList(raw.imageElements, cleanImageElement)
  const callout = cleanCallout(raw.callout)
  const secondary = typeof raw.screenshotSecondary === 'string' ? raw.screenshotSecondary : undefined
  return {
    id: raw.id,
    // The Play Store banner deck only has one layout. Normalising here (not
    // while editing) keeps it out of undo history.
    layout: isBanner ? 'feature-graphic' : raw.layout,
    label: coerceLocalized(raw.label),
    headline: coerceLocalized(raw.headline),
    screenshot: typeof raw.screenshot === 'string' ? raw.screenshot : '',
    ...(secondary === undefined || isBanner ? {} : { screenshotSecondary: secondary }),
    ...(typeof raw.inverted === 'boolean' ? { inverted: raw.inverted } : {}),
    ...(typography ? { typography } : {}),
    ...(backgroundColor ? { backgroundColor } : {}),
    ...(transforms ? { transforms } : {}),
    ...(textElements ? { textElements } : {}),
    ...(imageElements ? { imageElements } : {}),
    ...(callout ? { callout } : {}),
  }
}

function migrateDeviceSlides(device: string, slides: unknown): Slide[] {
  if (!Array.isArray(slides)) {
    return []
  }
  return slides.map((slide) => migrateSlide(slide, device)).filter((slide): slide is Slide => slide !== undefined)
}

function migrateDecks(value: unknown): ProjectState['slidesByDevice'] {
  const decks = { ...DEFAULT_PROJECT.slidesByDevice }
  if (!isRecord(value)) {
    return decks
  }
  for (const [device, slides] of Object.entries(value)) {
    if (isDevice(device)) {
      decks[device] = migrateDeviceSlides(device, slides)
    }
  }
  return decks
}

function cleanLocales(value: unknown): string[] {
  const locales = Array.isArray(value) ? value.filter((locale): locale is string => typeof locale === 'string') : []
  return locales.length > 0 ? locales : [...DEFAULT_PROJECT.locales]
}

/** Validate a raw project (file, cache or script) and fill it into the current schema. Throws on malformed input. */
export function parseProject(parsed: unknown): ProjectState {
  const validationError = projectValidationError(parsed)
  if (validationError || !isRecord(parsed)) {
    throw new Error(validationError ?? 'Project must be an object')
  }
  const importedFont = cleanImportedFont(parsed.importedFont)
  const scene = cleanScene(parsed.scene)
  const savedLooks = Array.isArray(parsed.savedLooks)
    ? parsed.savedLooks.map(cleanLook).filter((look): look is Look => !!look)
    : []
  const locales = cleanLocales(parsed.locales)
  const locale = typeof parsed.locale === 'string' && locales.includes(parsed.locale) ? parsed.locale : locales[0]
  const device = typeof parsed.device === 'string' && isDevice(parsed.device) ? parsed.device : DEFAULT_PROJECT.device
  const orientation = parsed.orientation === 'landscape' ? 'landscape' : DEFAULT_PROJECT.orientation
  const themeId = typeof parsed.themeId === 'string' && parsed.themeId.trim() ? parsed.themeId : DEFAULT_PROJECT.themeId
  return {
    schemaVersion: PROJECT_SCHEMA_VERSION,
    appName: typeof parsed.appName === 'string' ? parsed.appName : DEFAULT_PROJECT.appName,
    themeId,
    fontId: cleanFontId(parsed.fontId, !!importedFont),
    ...(importedFont ? { importedFont } : {}),
    connectedCanvas: typeof parsed.connectedCanvas === 'boolean' ? parsed.connectedCanvas : false,
    locales,
    locale: locale ?? DEFAULT_PROJECT.locale,
    device,
    // Duo decks are fixed to one orientation each; keep the project's in step.
    orientation: orientationOf(device, orientation),
    slidesByDevice: migrateDecks(parsed.slidesByDevice),
    appIcon: typeof parsed.appIcon === 'string' ? parsed.appIcon : '',
    ...(scene ? { scene } : {}),
    ...(savedLooks.length > 0 ? { savedLooks } : {}),
  }
}

/** Orientation is portrait on devices without a landscape target; returns the same object when nothing changes. */
export function normalizeOrientation(state: ProjectState): ProjectState {
  const orientation = supportsLandscape(state.device) ? orientationOf(state.device, state.orientation) : 'portrait'
  return orientation === state.orientation ? state : { ...state, orientation }
}
