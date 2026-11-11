// Style Lab: generates complete looks for a deck (palette, type, layout rhythm
// and scene), keeps locked parts from the current deck, and applies a look as
// one undoable edit. Generation is seeded, so the same seed always produces
// the same four looks.

import { supportsLandscape } from './device-specs'
import { SCREENSHOT_FONTS } from './fonts'
import { DEFAULT_SCENE, sameScene, sceneOf } from './scene'
import { DIRECTIONS, pick, rng } from './style-directions'
import { THEMES } from './themes'
import type { Device, Look, ProjectState, Scene, Slide } from './types'

export type LockKey = 'palette' | 'type' | 'layout' | 'scene'
export type Locks = Record<LockKey, boolean>
export const NO_LOCKS: Locks = { palette: false, type: false, layout: false, scene: false }

export const LOCK_LABEL: Record<LockKey, { name: string; hint: string }> = {
  palette: { name: 'Colors', hint: 'Theme and which screens are inverted' },
  type: { name: 'Type', hint: 'Font, headline weight, case, alignment and size' },
  layout: { name: 'Layout', hint: 'Device placement on each screen' },
  scene: { name: 'Scene', hint: 'Backdrop, decoration, shadow, glow and tilt' },
}

export const LOCK_KEYS: LockKey[] = ['palette', 'type', 'layout', 'scene']

/** The four directions a fresh Style Lab opens with: clearly different from each other. */
export const STARTER_DIRECTIONS = ['editorial', 'playful', 'cinematic', 'minimal']

export function directionName(id: string): string {
  return DIRECTIONS.find((d) => d.id === id)?.name ?? 'Custom'
}

/** One look from a direction, fully determined by the seed. */
export function generateLook(directionId: string, seed: number): Look {
  const direction = DIRECTIONS.find((d) => d.id === directionId) ?? DIRECTIONS[0]
  if (!direction) {
    throw new Error('no style directions defined')
  }
  const r = rng(seed * 7919 + direction.id.length * 104729)
  const themeId = pick(
    r,
    direction.themes.filter((id) => !!THEMES[id])
  )
  const [lo, hi] = direction.headlineScale
  return {
    id: `${direction.id}-${seed}`,
    name: `${direction.name} · ${THEMES[themeId]?.name ?? themeId}`,
    direction: direction.id,
    themeId,
    fontId: pick(
      r,
      direction.fonts.filter((id) => Object.hasOwn(SCREENSHOT_FONTS, id))
    ),
    scene: { ...direction.scene(r), headlineScale: Math.round((lo + r() * (hi - lo)) * 20) / 20 },
    layouts: pick(r, direction.layouts),
    inverted: pick(r, direction.inverted),
  }
}

/** `count` different directions for one shuffle, starting with the starter set. */
export function pickDirections(seed: number, count = 4): string[] {
  if (seed === 0) {
    return STARTER_DIRECTIONS.slice(0, count)
  }
  const r = rng(seed * 31 + 7)
  const pool = DIRECTIONS.map((d) => d.id)
  for (let i = pool.length - 1; i > 0; i--) {
    const j = Math.floor(r() * (i + 1))
    const a = pool[i]
    const b = pool[j]
    if (a !== undefined && b !== undefined) {
      pool[i] = b
      pool[j] = a
    }
  }
  return pool.slice(0, count)
}

/** The deck as it is now, expressed as a look. */
export function currentLook(state: ProjectState): Look {
  const slides = state.slidesByDevice[state.device] || []
  return {
    id: 'current',
    name: 'Current',
    direction: 'current',
    themeId: state.themeId,
    fontId: state.fontId ?? 'template-default',
    scene: sceneOf(state.scene),
    layouts: slides.length ? slides.map((s) => s.layout) : ['hero'],
    inverted: slides.length ? slides.map((s) => !!s.inverted) : [false],
  }
}

const TYPE_SCENE_KEYS = ['headlineWeight', 'headlineScale', 'headlineCase', 'captionAlign'] as const

const isSceneKey = (key: string): key is keyof Scene => Object.hasOwn(DEFAULT_SCENE, key)

function copySceneKey<K extends keyof Scene>(target: Scene, from: Scene, key: K): void {
  target[key] = from[key]
}

/** Replace every locked part of `look` with the same part of `base`. */
export function withLocks(look: Look, base: Look, locks: Locks): Look {
  const scene: Scene = { ...look.scene }
  if (locks.scene) {
    for (const key of Object.keys(DEFAULT_SCENE).filter(isSceneKey)) {
      if (!TYPE_SCENE_KEYS.some((typeKey) => typeKey === key)) {
        copySceneKey(scene, base.scene, key)
      }
    }
  }
  if (locks.type) {
    for (const key of TYPE_SCENE_KEYS) {
      copySceneKey(scene, base.scene, key)
    }
  }
  return {
    ...look,
    themeId: locks.palette ? base.themeId : look.themeId,
    inverted: locks.palette ? base.inverted : look.inverted,
    fontId: locks.type ? base.fontId : look.fontId,
    layouts: locks.layout ? base.layouts : look.layouts,
    scene,
  }
}

// Phones and portrait tablets have the full set of portrait layouts. Landscape
// canvases, watches and the Play banner keep their own layouts.
export function canRearrange(device: Device, orientation: ProjectState['orientation']): boolean {
  if (device === 'iphone' || device === 'android' || device === 'ipad') {
    return true
  }
  // Both Duo displays in portrait take every portrait layout.
  if (device === 'duo-outer' || device === 'duo-inner') {
    return true
  }
  if (device === 'android-7' || device === 'android-10') {
    return orientation === 'portrait' || !supportsLandscape(device)
  }
  return false
}

function restyleSlide(slide: Slide, index: number, look: Look, locks: Locks, rearrange: boolean): Slide {
  let next: Slide = { ...slide }
  if (!locks.palette) {
    const { inverted: _inverted, backgroundColor: _background, ...rest } = next
    next = look.inverted[index % look.inverted.length] ? { ...rest, inverted: true } : rest
  }
  if (!locks.layout && rearrange && slide.layout !== 'no-device' && slide.layout !== 'feature-graphic') {
    let layout = look.layouts[index % look.layouts.length] ?? slide.layout
    // A back device without its own screenshot would just repeat the front one.
    if (layout === 'two-devices' && !slide.screenshotSecondary) {
      layout = 'hero'
    }
    if (layout === 'no-device' || layout === 'feature-graphic' || layout === 'split-landscape') {
      layout = slide.layout
    }
    // Fresh layout = fresh default placement, so the look reads as designed.
    const { transforms, ...withoutTransforms } = next
    const { caption: _c, device: _d, deviceSecondary: _s, callout: _m, ...kept } = transforms || {}
    next = { ...withoutTransforms, layout, ...(Object.keys(kept).length ? { transforms: kept } : {}) }
  }
  return next
}

/** The project with `look` applied to the open deck. Locked parts stay as they are. */
export function applyLook(state: ProjectState, look: Look, locks: Locks = NO_LOCKS): ProjectState {
  const slides = state.slidesByDevice[state.device] || []
  const rearrange = canRearrange(state.device, state.orientation)
  const base = currentLook(state)
  const merged = withLocks(look, base, locks)
  // An imported font only exists on the machine that imported it.
  const fontId = merged.fontId === 'self-hosted' && !state.importedFont ? state.fontId : merged.fontId
  const { scene: _scene, fontId: _fontId, ...rest } = state
  return {
    ...rest,
    themeId: merged.themeId,
    ...(fontId ? { fontId } : {}),
    ...(sameScene(merged.scene, DEFAULT_SCENE) ? {} : { scene: merged.scene }),
    slidesByDevice: {
      ...state.slidesByDevice,
      [state.device]: slides.map((slide, i) => restyleSlide(slide, i, merged, locks, rearrange)),
    },
  }
}

/** Look id unique within the saved list. */
export function savedLookId(look: Look, saved: Look[]): string {
  let id = look.id === 'current' ? `look-${Date.now().toString(36)}` : look.id
  while (saved.some((s) => s.id === id)) {
    id = `${id}-${Math.floor(Math.random() * 1e4)}`
  }
  return id
}
