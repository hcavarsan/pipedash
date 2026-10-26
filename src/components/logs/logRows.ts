import type { LogDocumentMeta, LogSection } from '../../types/logs'

export type RowMeta = Pick<LogDocumentMeta, 'line_count' | 'sections'>

export type LogFilter = 'none' | 'matches' | 'errors'

export type Segment =
  | { row: number; kind: 'lines'; line: number; size: number }
  | { row: number; kind: 'step'; section: LogSection }

export interface RowModel {
  count: number
  segments: Segment[]
}

export type RowItem = { kind: 'step'; section: LogSection } | { kind: 'line'; line: number }

interface Frame {
  list: LogSection[]
  next: number
  cursor: number
  to: number
}

export const isOpen = (section: LogSection, overrides: ReadonlySet<number>): boolean =>
  section.open !== overrides.has(section.id)

const keyOf = (segment: Segment): number => (segment.kind === 'lines' ? segment.line : segment.section.start_line)

const lastKeyAtMost = (segments: readonly Segment[], line: number): number => {
  let low = 0
  let high = segments.length

  while (low < high) {
    const mid = (low + high) >>> 1

    if (keyOf(segments[mid]) <= line) {
      low = mid + 1
    } else {
      high = mid
    }
  }

  return low - 1
}

const lastRowAtMost = (segments: readonly Segment[], row: number): number => {
  let low = 0
  let high = segments.length

  while (low < high) {
    const mid = (low + high) >>> 1

    if (segments[mid].row <= row) {
      low = mid + 1
    } else {
      high = mid
    }
  }

  return low - 1
}

export function buildRows(meta: RowMeta, overrides: ReadonlySet<number>): RowModel {
  const children = new Map<number | null, LogSection[]>()

  for (const section of meta.sections) {
    const list = children.get(section.parent)

    if (list) {
      list.push(section)
    } else {
      children.set(section.parent, [section])
    }
  }

  const segments: Segment[] = []
  let count = 0

  const emit = (from: number, to: number) => {
    if (to <= from) {
      return
    }

    const last = segments[segments.length - 1]

    if (last?.kind === 'lines' && last.line + last.size === from) {
      last.size += to - from
    } else {
      segments.push({ row: count, kind: 'lines', line: from, size: to - from })
    }

    count += to - from
  }

  const stack: Frame[] = [{ list: children.get(null) ?? [], next: 0, cursor: 0, to: meta.line_count }]

  while (stack.length > 0) {
    const frame = stack[stack.length - 1]

    if (frame.next >= frame.list.length) {
      emit(frame.cursor, frame.to)
      stack.pop()
      continue
    }

    const section = frame.list[frame.next++]
    const isGroup = section.kind === 'group'
    const begin = isGroup ? (section.header_line ?? section.start_line) : section.start_line

    if (begin < frame.cursor || begin > frame.to) {
      continue
    }

    emit(frame.cursor, begin)

    if (isGroup) {
      if (section.header_line !== null) {
        emit(begin, begin + 1)
      }
    } else {
      segments.push({ row: count, kind: 'step', section })
      count += 1
    }

    const bodyStart = Math.max(section.start_line, begin + (isGroup && section.header_line !== null ? 1 : 0))
    const end = Math.min(Math.max(section.end_line, bodyStart), frame.to)

    frame.cursor = end

    if (isOpen(section, overrides)) {
      stack.push({ list: children.get(section.id) ?? [], next: 0, cursor: bodyStart, to: end })
    }
  }

  return { count, segments }
}

export function filterRows(lines: readonly number[]): RowModel {
  const ascending = lines.every((line, index) => index === 0 || line >= lines[index - 1])
  const sorted = ascending ? lines : [...lines].sort((a, b) => a - b)
  const segments: Segment[] = []
  let count = 0
  let previous = -1

  for (const line of sorted) {
    if (line === previous) {
      continue
    }

    previous = line

    const last = segments[segments.length - 1]

    if (last?.kind === 'lines' && last.line + last.size === line) {
      last.size += 1
    } else {
      segments.push({ row: count, kind: 'lines', line, size: 1 })
    }

    count += 1
  }

  return { count, segments }
}

export function rowAt(model: RowModel, row: number): RowItem | null {
  if (row < 0 || row >= model.count) {
    return null
  }

  const segment = model.segments[lastRowAtMost(model.segments, row)]

  return segment.kind === 'step'
    ? { kind: 'step', section: segment.section }
    : { kind: 'line', line: segment.line + row - segment.row }
}

export function rowOfLine(model: RowModel, line: number): number | null {
  const segment = model.segments[lastKeyAtMost(model.segments, line)]

  if (segment?.kind === 'lines' && line < segment.line + segment.size) {
    return segment.row + line - segment.line
  }

  return null
}

export function rowNearLine(model: RowModel, line: number): number {
  const segment = model.segments[lastKeyAtMost(model.segments, line)]

  if (!segment) {
    return 0
  }

  if (segment.kind === 'step') {
    return segment.row
  }

  return segment.row + Math.min(line - segment.line, segment.size - 1)
}

export function rowOfSection(model: RowModel, section: LogSection): number | null {
  const { segments } = model
  const wanted = section.node_id ?? section.title
  let low = 0
  let high = segments.length

  while (low < high) {
    const mid = (low + high) >>> 1

    if (keyOf(segments[mid]) < section.start_line) {
      low = mid + 1
    } else {
      high = mid
    }
  }

  for (let index = low; index < segments.length && keyOf(segments[index]) === section.start_line; index += 1) {
    const segment = segments[index]

    if (segment.kind === 'step' && (segment.section.node_id ?? segment.section.title) === wanted) {
      return segment.row
    }
  }

  return null
}

export function lineOfItem(item: RowItem | null): number {
  if (!item) {
    return 0
  }

  return item.kind === 'line' ? item.line : item.section.start_line
}

export function revealLine(meta: RowMeta, overrides: Set<number>, line: number): Set<number> {
  let next: Set<number> | null = null

  for (const section of meta.sections) {
    if (line < section.start_line || line >= section.end_line || isOpen(section, next ?? overrides)) {
      continue
    }

    next ??= new Set(overrides)

    if (!next.delete(section.id)) {
      next.add(section.id)
    }
  }

  return next ?? overrides
}

const stepLists = new WeakMap<readonly LogSection[], LogSection[]>()

export function sectionAtLine(meta: Pick<LogDocumentMeta, 'sections'>, line: number): LogSection | null {
  let steps = stepLists.get(meta.sections)

  if (!steps) {
    steps = meta.sections.filter((section) => section.kind === 'step')
    stepLists.set(meta.sections, steps)
  }

  let low = 0
  let high = steps.length

  while (low < high) {
    const mid = (low + high) >>> 1

    if (steps[mid].start_line <= line) {
      low = mid + 1
    } else {
      high = mid
    }
  }

  const step = steps[low - 1]

  return step && line < step.end_line ? step : null
}

const sectionKeys = (sections: readonly LogSection[]): string[] => {
  const seen = new Map<string, number>()

  return sections.map((section) => {
    const base = `${section.kind}\u0000${section.node_id ?? section.title}`
    const occurrence = seen.get(base) ?? 0

    seen.set(base, occurrence + 1)

    return `${base}\u0000${occurrence}`
  })
}

export function overridesFor(
  meta: Pick<LogDocumentMeta, 'sections'>,
  prevMeta: Pick<LogDocumentMeta, 'sections'>,
  prevOverrides: ReadonlySet<number>
): Set<number> {
  const result = new Set<number>()

  if (prevOverrides.size === 0) {
    return result
  }

  const previous = new Map<string, LogSection>()
  const prevKeys = sectionKeys(prevMeta.sections)

  prevMeta.sections.forEach((section, index) => {
    if (prevOverrides.has(section.id)) {
      previous.set(prevKeys[index], section)
    }
  })

  const keys = sectionKeys(meta.sections)

  meta.sections.forEach((section, index) => {
    const before = previous.get(keys[index])

    if (before && section.open === before.open) {
      result.add(section.id)
    }
  })

  return result
}
