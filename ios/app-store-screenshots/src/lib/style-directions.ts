import type { Scene, ScreenshotFontId, SlideLayout } from './types'

export type Rng = () => number

// mulberry32: tiny, fast and good enough for picking design options.
export function rng(seed: number): Rng {
  let a = seed >>> 0
  return () => {
    a = (a + 0x6d2b79f5) >>> 0
    let t = a
    t = Math.imul(t ^ (t >>> 15), t | 1)
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61)
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296
  }
}

export const pick = <T>(r: Rng, items: readonly T[]): T => {
  const item = items[Math.floor(r() * items.length) % items.length]
  if (item === undefined) {
    throw new Error('cannot pick from an empty list')
  }
  return item
}
const between = (r: Rng, min: number, max: number) => Math.round(min + r() * (max - min))
const signed = (r: Rng, min: number, max: number) => (r() < 0.5 ? -1 : 1) * between(r, min, max)

type Direction = {
  id: string
  name: string
  themes: string[]
  fonts: ScreenshotFontId[]
  layouts: SlideLayout[][]
  inverted: boolean[][]
  headlineScale: [number, number]
  scene: (r: Rng) => Omit<Scene, 'headlineScale'>
}

export const DIRECTIONS: Direction[] = [
  {
    id: 'editorial',
    name: 'Editorial',
    themes: [
      'warm-editorial',
      'magazine-cover-editorial',
      'bloom-roast',
      'quiet-japandi',
      'hand-drawn-editorial-tasks',
    ],
    fonts: ['baskerville', 'palatino', 'template-serif'],
    layouts: [
      ['device-bottom', 'hero'],
      ['hero', 'device-bottom', 'device-top'],
    ],
    inverted: [[false, false, true], [false]],
    headlineScale: [0.95, 1.1],
    scene: (r) => ({
      backdrop: pick(r, ['lines', 'solid', 'gradient'] as const),
      span: false,
      decoration: pick(r, ['none', 'rings'] as const),
      shadow: between(r, 25, 45),
      glow: 0,
      tilt: 0,
      headlineWeight: pick(r, [500, 600]),
      headlineCase: 'as-typed',
      captionAlign: 'left',
    }),
  },
  {
    id: 'playful',
    name: 'Playful',
    themes: [
      'candy-pop-social',
      'toybox-primary',
      'paper-sticker-skeuomorphic',
      'retro-rubberhose-mascot',
      'dreamy-pastel-couples',
    ],
    fonts: ['futura', 'avenir-next', 'template-default'],
    layouts: [
      ['hero', 'two-devices', 'device-top'],
      ['device-bottom', 'device-top'],
    ],
    inverted: [[false], [false, true]],
    headlineScale: [1.05, 1.2],
    scene: (r) => ({
      backdrop: pick(r, ['dots', 'gradient', 'aurora'] as const),
      span: r() < 0.5,
      decoration: pick(r, ['sparkles', 'blobs'] as const),
      shadow: between(r, 40, 60),
      glow: between(r, 10, 30),
      tilt: signed(r, 8, 16),
      headlineWeight: pick(r, [800, 900]),
      headlineCase: 'as-typed',
      captionAlign: 'center',
    }),
  },
  {
    id: 'cinematic',
    name: 'Cinematic',
    themes: [
      'midnight-glow-pro',
      'neon-athletic-night',
      'dark-bold',
      'moody-curated-dating',
      'glossy-3d-kbeauty-creator',
    ],
    fonts: ['helvetica-neue', 'system-sans', 'avenir-next'],
    layouts: [['hero'], ['hero', 'device-bottom']],
    inverted: [[false]],
    headlineScale: [0.9, 1],
    scene: (r) => ({
      backdrop: pick(r, ['spotlight', 'aurora'] as const),
      span: r() < 0.4,
      decoration: pick(r, ['none', 'rings'] as const),
      shadow: between(r, 60, 85),
      glow: between(r, 45, 80),
      tilt: r() < 0.35 ? 0 : signed(r, 8, 18),
      headlineWeight: pick(r, [700, 800]),
      headlineCase: pick(r, ['upper', 'as-typed'] as const),
      captionAlign: 'center',
    }),
  },
  {
    id: 'minimal',
    name: 'Minimal',
    themes: ['clean-light', 'quiet-japandi', 'swiss-grid-bold', 'ocean-fresh', 'bento-keynote-grid'],
    fonts: ['helvetica-neue', 'system-sans', 'optima', 'template-default'],
    layouts: [['device-bottom'], ['device-bottom', 'device-top']],
    inverted: [[false]],
    headlineScale: [0.9, 1],
    scene: (r) => ({
      backdrop: pick(r, ['solid', 'grid'] as const),
      span: false,
      decoration: 'none',
      shadow: between(r, 15, 30),
      glow: 0,
      tilt: 0,
      headlineWeight: pick(r, [600, 700]),
      headlineCase: 'as-typed',
      captionAlign: pick(r, ['center', 'left'] as const),
    }),
  },
  {
    id: 'dreamy',
    name: 'Dreamy',
    themes: ['dreamy-pastel-couples', 'liquid-glass-aurora', 'soft-clay-wellness', 'ocean-fresh'],
    fonts: ['avenir-next', 'optima', 'palatino'],
    layouts: [['hero', 'device-top'], ['hero']],
    inverted: [[false]],
    headlineScale: [1, 1.1],
    scene: (r) => ({
      backdrop: 'aurora',
      span: true,
      decoration: pick(r, ['blobs', 'sparkles'] as const),
      shadow: between(r, 30, 50),
      glow: between(r, 30, 60),
      tilt: signed(r, 6, 12),
      headlineWeight: pick(r, [600, 700]),
      headlineCase: 'as-typed',
      captionAlign: 'center',
    }),
  },
  {
    id: 'swiss',
    name: 'Swiss Bold',
    themes: ['swiss-grid-bold', 'bento-keynote-grid', 'risograph-zine', 'neon-athletic-night'],
    fonts: ['helvetica-neue', 'futura'],
    layouts: [['device-bottom', 'device-top']],
    inverted: [
      [false, true],
      [false, false, true],
    ],
    headlineScale: [0.85, 0.95],
    scene: (r) => ({
      backdrop: 'grid',
      span: r() < 0.5,
      decoration: pick(r, ['none', 'rings'] as const),
      shadow: between(r, 30, 50),
      glow: 0,
      tilt: r() < 0.5 ? 0 : signed(r, 8, 12),
      headlineWeight: 900,
      headlineCase: 'upper',
      captionAlign: 'left',
    }),
  },
  {
    id: 'poster',
    name: 'Vintage Poster',
    themes: ['vintage-travel-poster', 'risograph-zine', 'hand-drawn-editorial-tasks', 'warm-editorial'],
    fonts: ['american-typewriter', 'futura', 'baskerville'],
    layouts: [['hero', 'device-top']],
    inverted: [[false, false, true]],
    headlineScale: [0.9, 1.05],
    scene: (r) => ({
      backdrop: pick(r, ['lines', 'dots'] as const),
      span: false,
      decoration: pick(r, ['rings', 'sparkles'] as const),
      shadow: between(r, 35, 55),
      glow: 0,
      tilt: r() < 0.5 ? 0 : signed(r, 4, 8),
      headlineWeight: pick(r, [700, 800]),
      headlineCase: 'upper',
      captionAlign: 'center',
    }),
  },
  {
    id: 'panorama',
    name: 'Panorama',
    themes: ['ocean-fresh', 'liquid-glass-aurora', 'midnight-glow-pro', 'candy-pop-social', 'bloom-roast'],
    fonts: ['system-sans', 'avenir-next', 'template-default'],
    layouts: [
      ['hero', 'device-bottom'],
      ['device-bottom', 'hero', 'device-top'],
    ],
    inverted: [[false]],
    headlineScale: [1, 1.1],
    scene: (r) => ({
      backdrop: pick(r, ['aurora', 'gradient'] as const),
      span: true,
      decoration: pick(r, ['blobs', 'rings'] as const),
      shadow: between(r, 40, 60),
      glow: between(r, 15, 40),
      tilt: signed(r, 10, 20),
      headlineWeight: pick(r, [700, 800]),
      headlineCase: 'as-typed',
      captionAlign: 'center',
    }),
  },
]
