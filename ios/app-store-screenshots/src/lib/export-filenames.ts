import type { Device, Slide } from './types'

export function slugify(s: string): string {
  return (
    s
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/(^-|-$)/g, '') || 'screenshots'
  )
}

export function slideNeedsScreenshot(device: Device, slide: Slide): boolean {
  if (device === 'feature-graphic') {
    return false
  }
  return slide.layout !== 'no-device' && slide.layout !== 'feature-graphic'
}

export function stamp(): string {
  const d = new Date()
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}${pad(d.getMonth() + 1)}${pad(d.getDate())}-${pad(d.getHours())}${pad(d.getMinutes())}`
}
