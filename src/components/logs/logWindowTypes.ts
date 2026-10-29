import type { RowHeights } from './rowHeights'

const CENTER_LEAD = 0.3

export type ScrollAlign = 'center' | 'start' | 'end'

export interface LogBodyHandle {
  scrollToRow: (row: number, align?: ScrollAlign) => void
  scrollToEnd: () => void
  getVisibleRange: () => { first: number; last: number }
  focus: () => void
}

export interface Metrics {
  rowHeight: number
  charWidth: number
  width: number
}

export interface Win {
  base: number
  size: number
  heights: RowHeights
}

export interface Range {
  first: number
  last: number
  vis: number
  lastVis: number
}

export interface Restore {
  row: number
  offset: number
  end?: boolean
}

export interface Target {
  row: number
  align: ScrollAlign
}

export const EMPTY_RANGE: Range = { first: 0, last: -1, vis: 0, lastVis: 0 }

export const restoreFor = (row: number, align: ScrollAlign, clientHeight: number): Restore => {
  if (align === 'start') {
    return { row, offset: 0 }
  }

  return align === 'end' ? { row, offset: 0, end: true } : { row, offset: -clientHeight * CENTER_LEAD }
}
