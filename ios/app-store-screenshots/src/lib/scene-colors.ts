function expandHex(hex: string): string {
  const c = hex.replace('#', '')
  return c.length === 3
    ? c
        .split('')
        .map((x) => x + x)
        .join('')
    : c
}

export function shade(hex: string, percent: number): string {
  const num = parseInt(expandHex(hex), 16)
  const amt = Math.round((255 * percent) / 100)
  const ch = (shift: number) => Math.max(0, Math.min(255, ((num >> shift) & 0xff) + amt))
  return `#${((ch(16) << 16) | (ch(8) << 8) | ch(0)).toString(16).padStart(6, '0')}`
}

export function alpha(hex: string, a: number): string {
  const num = parseInt(expandHex(hex), 16)
  if (!Number.isFinite(num)) {
    return `rgba(0,0,0,${a})`
  }
  return `rgba(${(num >> 16) & 0xff}, ${(num >> 8) & 0xff}, ${num & 0xff}, ${Math.max(0, Math.min(1, a))})`
}

// Deterministic 0–1 noise so decorations sit in the same place in the editor,
// thumbnails and every export.
export function hash01(n: number): number {
  const x = Math.sin(n * 127.1 + 311.7) * 43758.5453
  return x - Math.floor(x)
}
