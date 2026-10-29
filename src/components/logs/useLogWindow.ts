import { type RefObject, useCallback, useEffect, useLayoutEffect, useReducer, useRef, useState } from 'react'
import type { LogDocumentMeta } from '../../types/logs'
import type { LineState } from './lineCache'
import { measureRenderedRows } from './logDomMeasurements'
import { type RowModel, rowAt, rowNearLine, rowOfSection } from './logRows'
import { OVERSCAN_ROWS, planWindow } from './logViewport'
import {
  EMPTY_RANGE,
  type Metrics,
  type Range,
  type Restore,
  restoreFor,
  type Target,
  type Win,
} from './logWindowTypes'
import { RowHeights } from './rowHeights'
import { visualLines } from './wrapEstimate'

const STEP_EXTRA_PX = 8
const TARGET_HOLD_MS = 400
const TARGET_MAX_MS = 6000

interface LatestWindowState {
  model: RowModel
  wrap: boolean
  getLine: (line: number) => LineState
  metrics: Metrics | null
  cols: number
}

export interface LogWindowOptions {
  meta: LogDocumentMeta
  model: RowModel
  wrap: boolean
  getLine: (line: number) => LineState
  metrics: Metrics | null
  cols: number
  layoutKey: string
  scrollerRef: RefObject<HTMLDivElement | null>
  spacerRef: RefObject<HTMLDivElement | null>
  renderedRef: RefObject<number[]>
}

export interface LogWindowController {
  winRef: RefObject<Win | null>
  restoreRef: RefObject<Restore | null>
  queuedRef: RefObject<Target | null>
  targetRef: RefObject<Target | null>
  stickEnd: RefObject<boolean>
  latest: RefObject<LatestWindowState>
  range: Range
  bump: () => void
  updateRange: () => void
  applyRestore: (restore: Restore) => void
  rebuildAround: (row: number, restore: Restore) => void
  anchor: () => Restore | null
  restoreOfTarget: (target: Target) => Restore
  holdTarget: (target: Target | null) => void
  releaseTarget: () => void
}

export function useLogWindow(options: LogWindowOptions): LogWindowController {
  const { meta, model, wrap, getLine, metrics, cols, layoutKey, scrollerRef, spacerRef, renderedRef } = options
  const winRef = useRef<Win | null>(null)
  const restoreRef = useRef<Restore | null>(null)
  const queuedRef = useRef<Target | null>(null)
  const targetRef = useRef<Target | null>(null)
  const targetSince = useRef(0)
  const targetTimer = useRef<number | undefined>(undefined)
  const prevRef = useRef<{ model: RowModel; key: string; meta: LogDocumentMeta } | null>(null)
  const stickEnd = useRef(false)
  const [range, setRange] = useState<Range>(EMPTY_RANGE)
  const [, bump] = useReducer((count: number) => count + 1, 0)
  const latest = useRef<LatestWindowState>({ model, wrap, getLine, metrics, cols })

  latest.current = { model, wrap, getLine, metrics, cols }

  const updateRange = useCallback(() => {
    const win = winRef.current
    const el = scrollerRef.current
    let next = EMPTY_RANGE

    if (win && el && win.size > 0) {
      const vis = win.heights.indexAt(el.scrollTop)
      const lastVis = win.heights.indexAt(el.scrollTop + el.clientHeight)

      next = {
        first: Math.max(0, vis - OVERSCAN_ROWS),
        last: Math.min(win.size - 1, lastVis + OVERSCAN_ROWS),
        vis,
        lastVis,
      }
    }

    setRange((previous) =>
      previous.first === next.first &&
      previous.last === next.last &&
      previous.vis === next.vis &&
      previous.lastVis === next.lastVis
        ? previous
        : next
    )
  }, [scrollerRef])

  const buildWindow = useCallback((anchorRow: number): Win => {
    const { model: current, metrics: m, cols: columns, wrap: wrapped, getLine: lineAt } = latest.current
    const rowHeight = m?.rowHeight ?? 0
    const estimate = (row: number) => {
      const item = rowAt(current, row)

      if (item?.kind === 'step') {
        return rowHeight + STEP_EXTRA_PX
      }

      if (item && wrapped) {
        const line = lineAt(item.line)

        if (typeof line === 'object') {
          return rowHeight * visualLines(line.text, columns)
        }
      }

      return rowHeight
    }
    const { base, size } = planWindow(current.count, anchorRow, estimate)

    return { base, size, heights: new RowHeights(size, (index) => estimate(base + index)) }
  }, [])

  const applyRestore = useCallback(
    (restore: Restore) => {
      const win = winRef.current
      const el = scrollerRef.current

      if (!win || !el || win.size === 0) {
        return
      }

      const rel = Math.min(Math.max(restore.row - win.base, 0), win.size - 1)
      const top = restore.end ? win.heights.prefix(rel + 1) - el.clientHeight : win.heights.prefix(rel) + restore.offset

      stickEnd.current = Boolean(restore.end)

      if (spacerRef.current) {
        spacerRef.current.style.height = `${win.heights.total()}px`
      }

      el.scrollTop = Math.max(0, top)
      updateRange()
    },
    [updateRange, scrollerRef, spacerRef]
  )

  const rebuildAround = useCallback(
    (row: number, restore: Restore) => {
      winRef.current = buildWindow(row)
      restoreRef.current = restore
      bump()
    },
    [buildWindow]
  )

  const anchor = useCallback((): Restore | null => {
    const win = winRef.current
    const el = scrollerRef.current

    if (!win || !el || win.size === 0) {
      return null
    }

    const rel = win.heights.indexAt(el.scrollTop)

    return { row: win.base + rel, offset: el.scrollTop - win.heights.prefix(rel) }
  }, [scrollerRef])

  const restoreOfTarget = useCallback(
    (target: Target): Restore => restoreFor(target.row, target.align, scrollerRef.current?.clientHeight ?? 0),
    [scrollerRef]
  )

  const holdTarget = useCallback(
    (target: Target | null) => {
      window.clearTimeout(targetTimer.current)

      if (target !== targetRef.current) {
        targetSince.current = performance.now()
      }

      targetRef.current = target

      if (!target) {
        return
      }

      const settle = () => {
        const waiting = renderedRef.current.some((line) => typeof latest.current.getLine(line) !== 'object')

        if (waiting && performance.now() - targetSince.current < TARGET_MAX_MS) {
          targetTimer.current = window.setTimeout(settle, TARGET_HOLD_MS)
        } else {
          targetRef.current = null
        }
      }

      targetTimer.current = window.setTimeout(settle, TARGET_HOLD_MS)
    },
    [renderedRef]
  )

  const releaseTarget = useCallback(() => holdTarget(null), [holdTarget])

  useLayoutEffect(() => {
    const el = scrollerRef.current
    const win = winRef.current

    if (!el || !metrics || metrics.rowHeight <= 0 || metrics.charWidth <= 0) {
      return
    }

    const previous = prevRef.current
    const modelChanged = previous?.model !== model
    const keyChanged = previous?.key !== layoutKey

    if (!win || modelChanged || keyChanged) {
      let restore: Restore = { row: 0, offset: 0 }
      const current = win ? anchor() : null

      if (queuedRef.current) {
        holdTarget(queuedRef.current)
        restore = restoreOfTarget(queuedRef.current)
        queuedRef.current = null
      } else if (targetRef.current) {
        restore = restoreOfTarget(targetRef.current)
      } else if (win && previous && current) {
        const atEnd = stickEnd.current || el.scrollHeight - el.scrollTop - el.clientHeight <= metrics.rowHeight + 1

        if (modelChanged && atEnd && !previous.meta.complete && model.count > 0) {
          restore = { row: model.count - 1, offset: 0, end: true }
        } else if (modelChanged) {
          const item = rowAt(previous.model, current.row)
          const row = !item
            ? 0
            : item.kind === 'line'
              ? rowNearLine(model, item.line)
              : (rowOfSection(model, item.section) ?? rowNearLine(model, item.section.start_line))

          restore = { row, offset: current.offset }
        } else {
          restore = current
        }
      }

      prevRef.current = { model, key: layoutKey, meta }
      winRef.current = buildWindow(restore.row)
      restoreRef.current = restore
      bump()

      return
    }

    prevRef.current = { model, key: layoutKey, meta }

    if (restoreRef.current) {
      const restore = restoreRef.current

      restoreRef.current = null
      measureRenderedRows(el, winRef.current)
      applyRestore(restore)
    } else if (targetRef.current) {
      measureRenderedRows(el, winRef.current)
      applyRestore(restoreOfTarget(targetRef.current))
    }
  })

  useEffect(() => () => window.clearTimeout(targetTimer.current), [])

  return {
    winRef,
    restoreRef,
    queuedRef,
    targetRef,
    stickEnd,
    latest,
    range,
    bump,
    updateRange,
    applyRestore,
    rebuildAround,
    anchor,
    restoreOfTarget,
    holdTarget,
    releaseTarget,
  }
}
