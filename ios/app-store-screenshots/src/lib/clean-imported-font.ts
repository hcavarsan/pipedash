import { isRecord } from './project-validation'
import type { ImportedFont } from './types'

const FORMATS: readonly ImportedFont['format'][] = ['woff2', 'woff', 'truetype', 'opentype']

const isFormat = (value: unknown): value is ImportedFont['format'] => FORMATS.some((format) => format === value)

const isPrintable = (char: string): boolean => {
  const code = char.charCodeAt(0)
  return code > 0x1f && code !== 0x7f
}

// Display names come from the uploaded file name; keep them short and printable.
export function cleanFontName(value: unknown): string | undefined {
  if (typeof value !== 'string') {
    return undefined
  }
  const name = [...value].filter(isPrintable).join('').replace(/\s+/g, ' ').trim().slice(0, 60)
  return name || undefined
}

export function cleanImportedFont(value: unknown): ImportedFont | undefined {
  if (!isRecord(value)) {
    return undefined
  }
  const { src, format } = value
  if (typeof src !== 'string' || !/^\/fonts\/imported\/[a-z0-9]+\.(woff2|woff|ttf|otf)$/i.test(src)) {
    return undefined
  }
  if (!isFormat(format)) {
    return undefined
  }
  const name = cleanFontName(value.name)
  return { src, format, ...(name ? { name } : {}) }
}
