import type { LogSpan } from '../../types/logs'

export interface TextSegment {
  text: string
  style: number | null
  match: 0 | 1 | 2
  href?: string
}

export type Highlight = [start: number, end: number, current: boolean]

const URL_PATTERN = /https?:\/\/[^\s<>"'\]]+/g
const URL_TRAILING = /[.,;:)]+$/

export function segmentsFor(text: string, spans: readonly LogSpan[], highlights: readonly Highlight[]): TextSegment[] {
  if (text.length === 0) {
    return []
  }

  const urls: Array<[number, number]> = []

  if (text.includes('http')) {
    for (const found of text.matchAll(URL_PATTERN)) {
      const url = found[0].replace(URL_TRAILING, '')

      if (url.length > 'https://'.length) {
        urls.push([found.index, found.index + url.length])
      }
    }
  }

  if (spans.length === 0 && highlights.length === 0 && urls.length === 0) {
    return [{ text, style: null, match: 0 }]
  }

  const cuts = new Set<number>([0, text.length])
  const clip = (value: number) => Math.min(Math.max(value, 0), text.length)

  for (const [start, end] of spans) {
    cuts.add(clip(start))
    cuts.add(clip(end))
  }

  for (const [start, end] of highlights) {
    cuts.add(clip(start))
    cuts.add(clip(end))
  }

  for (const [start, end] of urls) {
    cuts.add(start)
    cuts.add(end)
  }

  const points = [...cuts].sort((a, b) => a - b)
  const result: TextSegment[] = []
  let spanIndex = 0

  for (let index = 0; index + 1 < points.length; index += 1) {
    const start = points[index]
    const end = points[index + 1]

    if (start === undefined || end === undefined) {
      break
    }

    while (spanIndex < spans.length && (spans[spanIndex]?.[1] ?? Infinity) <= start) {
      spanIndex += 1
    }

    const span = spans[spanIndex]
    const style = span && span[0] <= start && span[1] > start ? span[2] : null
    let match: 0 | 1 | 2 = 0

    for (const [from, to, current] of highlights) {
      if (from <= start && to >= end && to > from) {
        match = current ? 2 : match === 2 ? 2 : 1
      }
    }

    const url = urls.find(([from, to]) => from <= start && to >= end)
    const href = url ? text.slice(url[0], url[1]) : undefined
    const previous = result[result.length - 1]

    if (previous && previous.style === style && previous.match === match && previous.href === href) {
      previous.text += text.slice(start, end)
    } else {
      result.push({ text: text.slice(start, end), style, match, ...(href ? { href } : {}) })
    }
  }

  return result
}
