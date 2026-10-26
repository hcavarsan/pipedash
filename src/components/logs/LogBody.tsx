import { UnstyledButton } from '@mantine/core'
import {
  forwardRef,
  type HTMLAttributes,
  type KeyboardEvent,
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useReducer,
  useRef,
  useState,
} from 'react'
import type { LogDocumentMeta, LogSection } from '../../types/logs'
import { StatusIcon } from '../atoms/StatusBadge'
import { registerScrollToTop } from '../layout/scrollToTop'
import { LogRow, type MatchTriple } from './LogRow'
import classes from './LogViewer.module.css'
import type { LineState } from './lineCache'
import { isOpen, lineOfItem, type RowModel, rowAt, rowNearLine, rowOfSection, sectionAtLine } from './logRows'
import { MAX_SCROLL_PX, needsRebase, OVERSCAN_ROWS, planWindow } from './logViewport'
import { RowHeights } from './rowHeights'
import { visualLines } from './wrapEstimate'

const STEP_EXTRA_PX = 8
const TIMESTAMP_CH = 13
const GUTTER_EXTRA_CH = 4
const REBASE_DELAY_MS = 150
const CENTER_LEAD = 0.3
const TARGET_HOLD_MS = 400
const TARGET_MAX_MS = 6000
const PROBE_CHARS = 10

export type ScrollAlign = 'center' | 'start' | 'end'

export interface LogBodyHandle {
  scrollToRow: (row: number, align?: ScrollAlign) => void
  scrollToEnd: () => void
  getVisibleRange: () => { first: number; last: number }
  focus: () => void
}

export interface LogBodyProps {
  jobName: string
  variant: 'desktop' | 'narrow' | 'phone'
  meta: LogDocumentMeta
  model: RowModel
  overrides: ReadonlySet<number>
  groups: ReadonlyMap<number, LogSection>
  styleClasses: readonly string[]
  getLine: (line: number) => LineState
  wrap: boolean
  timestamps: boolean
  stickyHeaders: boolean
  selection: { from: number; to: number } | null
  matchIndex: ReadonlyMap<number, MatchTriple[]>
  currentMatch: MatchTriple | null
  emptyLabel: string
  onVisibleLines: (lines: number[]) => void
  onViewport: (firstRow: number, lastRow: number) => void
  onToggle: (sectionId: number) => void
  onSelect: (line: number, extend: boolean) => void
  onFullLine: (line: number) => void
  rowEvents: Pick<HTMLAttributes<HTMLDivElement>, 'onContextMenu' | 'onKeyDown'>
  onKeyDown: (event: KeyboardEvent<HTMLElement>) => void
}

interface Metrics {
  rowHeight: number
  charWidth: number
  width: number
}

interface Win {
  base: number
  size: number
  heights: RowHeights
}

interface Range {
  first: number
  last: number
  vis: number
  lastVis: number
}

interface Restore {
  row: number
  offset: number
  end?: boolean
}

const EMPTY_RANGE: Range = { first: 0, last: -1, vis: 0, lastVis: 0 }

interface Target {
  row: number
  align: ScrollAlign
}

const restoreFor = (row: number, align: ScrollAlign, clientHeight: number): Restore => {
  if (align === 'start') {
    return { row, offset: 0 }
  }

  return align === 'end' ? { row, offset: 0, end: true } : { row, offset: -clientHeight * CENTER_LEAD }
}

export const LogBody = forwardRef<LogBodyHandle, LogBodyProps>(function LogBody(props, ref) {
  const { meta, model, wrap, timestamps, variant, jobName } = props
  const scrollerRef = useRef<HTMLDivElement>(null)
  const spacerRef = useRef<HTMLDivElement>(null)
  const probeRef = useRef<HTMLDivElement>(null)
  const probeTextRef = useRef<HTMLSpanElement>(null)
  const winRef = useRef<Win | null>(null)
  const restoreRef = useRef<Restore | null>(null)
  const queuedRef = useRef<Target | null>(null)
  const targetRef = useRef<Target | null>(null)
  const targetSince = useRef(0)
  const renderedRef = useRef<number[]>([])
  const targetTimer = useRef<number | undefined>(undefined)
  const prevRef = useRef<{ model: RowModel; key: string; meta: LogDocumentMeta } | null>(null)
  const stickEnd = useRef(false)
  const frameRef = useRef<number | null>(null)
  const rebaseTimer = useRef<number | undefined>(undefined)
  const observerRef = useRef<ResizeObserver | null>(null)
  const lastLines = useRef('')
  const [metrics, setMetrics] = useState<Metrics | null>(null)
  const [range, setRange] = useState<Range>(EMPTY_RANGE)
  const [, bump] = useReducer((count: number) => count + 1, 0)

  const gutterCh = Math.max(3, String(meta.line_count).length) + GUTTER_EXTRA_CH
  const cols = metrics
    ? Math.max(1, Math.floor(metrics.width / metrics.charWidth) - gutterCh - (timestamps ? TIMESTAMP_CH : 0) - 1)
    : 0
  const layoutKey = metrics ? `${metrics.rowHeight}:${wrap ? cols : 0}:${wrap}` : ''

  const latest = useRef({ ...props, metrics, cols })

  latest.current = { ...props, metrics, cols }

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
  }, [])

  const buildWindow = useCallback((anchorRow: number): Win => {
    const { model: current, metrics: m, cols: columns, wrap: wrapped, getLine } = latest.current
    const rowHeight = m?.rowHeight ?? 0
    const estimate = (row: number) => {
      const item = rowAt(current, row)

      if (item?.kind === 'step') {
        return rowHeight + STEP_EXTRA_PX
      }

      if (item && wrapped) {
        const line = getLine(item.line)

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
    [updateRange]
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
  }, [])

  const restoreOfTarget = useCallback(
    (target: Target): Restore => restoreFor(target.row, target.align, scrollerRef.current?.clientHeight ?? 0),
    []
  )

  const holdTarget = useCallback((target: Target | null) => {
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
  }, [])

  const releaseTarget = useCallback(() => holdTarget(null), [holdTarget])

  const measureRendered = useCallback(() => {
    const win = winRef.current

    if (!win) {
      return
    }

    for (const node of scrollerRef.current?.querySelectorAll<HTMLElement>('[data-row]') ?? []) {
      const rel = Number(node.dataset.row) - win.base
      const height = node.getBoundingClientRect().height

      if (rel >= 0 && rel < win.size && height > 0) {
        win.heights.set(rel, height)
      }
    }
  }, [])

  const onResize = useCallback(
    (entries: ResizeObserverEntry[]) => {
      const win = winRef.current
      const el = scrollerRef.current

      if (!win || !el) {
        return
      }

      const top = el.scrollTop
      const anchorRel = win.heights.indexAt(top)
      const offset = top - win.heights.prefix(anchorRel)
      let above = 0
      let changed = false

      for (const entry of entries) {
        const rel = Number((entry.target as HTMLElement).dataset.row) - win.base
        const height = entry.borderBoxSize?.[0]?.blockSize ?? entry.contentRect.height

        if (rel < 0 || rel >= win.size || height <= 0) {
          continue
        }

        const old = win.heights.get(rel)

        if (Math.abs(height - old) < 0.01) {
          continue
        }

        win.heights.set(rel, height)
        changed = true

        if (rel < anchorRel) {
          above += height - old
        }
      }

      if (!changed) {
        return
      }

      const held = targetRef.current

      if (win.heights.total() >= MAX_SCROLL_PX) {
        const next = held ? restoreOfTarget(held) : { row: win.base + anchorRel, offset }

        rebuildAround(next.row, next)

        return
      }

      if (held) {
        holdTarget(held)
        applyRestore(restoreOfTarget(held))
        bump()

        return
      }

      if (spacerRef.current) {
        spacerRef.current.style.height = `${win.heights.total()}px`
      }

      if (stickEnd.current) {
        el.scrollTop = el.scrollHeight
      } else if (above !== 0) {
        el.scrollTop = top + above
      }

      bump()
    },
    [rebuildAround, holdTarget, applyRestore, restoreOfTarget]
  )

  const rowRef = useCallback(
    (node: HTMLDivElement | null) => {
      if (!node) {
        return
      }

      observerRef.current ??= new ResizeObserver(onResize)
      observerRef.current.observe(node)

      return () => observerRef.current?.unobserve(node)
    },
    [onResize]
  )

  useEffect(() => () => observerRef.current?.disconnect(), [])

  useLayoutEffect(() => {
    const scroller = scrollerRef.current
    const probe = probeRef.current
    const text = probeTextRef.current

    if (!scroller || !probe || !text) {
      return
    }

    const measure = () => {
      if (targetRef.current) {
        applyRestore(restoreOfTarget(targetRef.current))
      }

      const next = {
        rowHeight: probe.getBoundingClientRect().height,
        charWidth: text.getBoundingClientRect().width / PROBE_CHARS,
        width: scroller.clientWidth,
      }

      scroller.style.setProperty('--log-client-width', `${scroller.clientWidth}px`)
      setMetrics((previous) =>
        previous &&
        previous.rowHeight === next.rowHeight &&
        previous.charWidth === next.charWidth &&
        previous.width === next.width
          ? previous
          : next
      )
    }
    const observer = new ResizeObserver(measure)

    measure()
    observer.observe(scroller)
    observer.observe(probe)

    return () => observer.disconnect()
  }, [applyRestore, restoreOfTarget])

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
      measureRendered()
      applyRestore(restore)
    } else if (targetRef.current) {
      measureRendered()
      applyRestore(restoreOfTarget(targetRef.current))
    }
  })

  useEffect(
    () => () => {
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current)
      }

      window.clearTimeout(rebaseTimer.current)
      window.clearTimeout(targetTimer.current)
    },
    []
  )

  const maybeRebase = useCallback(() => {
    const win = winRef.current
    const current = anchor()

    if (win && current && needsRebase(win.base, win.size, latest.current.model.count, current.row)) {
      rebuildAround(current.row, current)
    }
  }, [anchor, rebuildAround])

  const onScroll = useCallback(() => {
    const el = scrollerRef.current

    if (frameRef.current === null) {
      frameRef.current = requestAnimationFrame(() => {
        frameRef.current = null
        updateRange()
      })
    }

    if (el && stickEnd.current && el.scrollHeight - el.scrollTop - el.clientHeight > 4) {
      stickEnd.current = false
    }

    window.clearTimeout(rebaseTimer.current)
    rebaseTimer.current = window.setTimeout(maybeRebase, REBASE_DELAY_MS)
  }, [maybeRebase, updateRange])

  const scrollToRow = useCallback(
    (row: number, align: ScrollAlign = 'center') => {
      const win = winRef.current
      const el = scrollerRef.current
      const { model: current } = latest.current

      if (current.count === 0) {
        return
      }

      const target = Math.min(Math.max(row, 0), current.count - 1)

      if (!win || !el) {
        queuedRef.current = { row: target, align }

        return
      }

      const held = { row: target, align }
      const restore = restoreFor(target, align, el.clientHeight)

      holdTarget(held)
      restoreRef.current = null

      if (target < win.base || target >= win.base + win.size) {
        rebuildAround(target, restore)
      } else {
        applyRestore(restore)
      }
    },
    [applyRestore, rebuildAround, holdTarget]
  )

  useImperativeHandle(
    ref,
    () => ({
      scrollToRow,
      scrollToEnd: () => scrollToRow(latest.current.model.count - 1, 'end'),
      getVisibleRange: () => {
        const win = winRef.current
        const el = scrollerRef.current

        if (!win || !el || win.size === 0) {
          return { first: 0, last: 0 }
        }

        return {
          first: win.base + win.heights.indexAt(el.scrollTop),
          last: win.base + win.heights.indexAt(el.scrollTop + el.clientHeight),
        }
      },
      focus: () => scrollerRef.current?.focus({ preventScroll: true }),
    }),
    [scrollToRow]
  )

  useEffect(() => {
    if (variant !== 'phone') {
      return
    }

    return registerScrollToTop({
      isActive: () => !!scrollerRef.current && !scrollerRef.current.closest('[data-sheet-covered]'),
      scrollToTop: () => scrollToRow(0, 'start'),
    })
  }, [variant, scrollToRow])

  const win = winRef.current
  const size = win?.size ?? 0
  const first = Math.max(0, Math.min(range.first, size - 1))
  const last = Math.min(range.last, size - 1)
  const elements = []
  const lines: number[] = []

  if (win && metrics) {
    for (let index = first; index <= last; index += 1) {
      const row = win.base + index
      const item = rowAt(model, row)

      if (!item) {
        continue
      }

      if (item.kind === 'step') {
        elements.push(
          <div key={row} ref={rowRef} data-row={row}>
            <LogRow
              kind="step"
              line={0}
              data={undefined}
              section={item.section}
              open={isOpen(item.section, props.overrides)}
              styleClasses={props.styleClasses}
              gutterCh={gutterCh}
              timestamps={timestamps}
              selected={false}
              matches={undefined}
              current={null}
              onToggle={props.onToggle}
              onSelect={props.onSelect}
              onFullLine={props.onFullLine}
            />
          </div>
        )
        continue
      }

      const group = props.groups.get(item.line) ?? null
      const { selection, currentMatch } = props

      lines.push(item.line)
      elements.push(
        <div key={row} ref={rowRef} data-row={row}>
          <LogRow
            kind="line"
            line={item.line}
            data={props.getLine(item.line)}
            section={group}
            open={group ? isOpen(group, props.overrides) : false}
            styleClasses={props.styleClasses}
            gutterCh={gutterCh}
            timestamps={timestamps}
            selected={!!selection && item.line >= selection.from && item.line <= selection.to}
            matches={props.matchIndex.get(item.line)}
            current={currentMatch && currentMatch[0] === item.line ? currentMatch : null}
            onToggle={props.onToggle}
            onSelect={props.onSelect}
            onFullLine={props.onFullLine}
          />
        </div>
      )
    }
  }

  renderedRef.current = lines
  const visibleKey = lines.join(',')
  const { onVisibleLines, onViewport } = props
  const baseRow = win?.base ?? 0

  useEffect(() => {
    if (lastLines.current !== visibleKey) {
      lastLines.current = visibleKey
      onVisibleLines(visibleKey ? visibleKey.split(',').map(Number) : [])
    }
  }, [visibleKey, onVisibleLines])

  useEffect(() => {
    if (range.last >= range.first) {
      onViewport(baseRow + range.vis, baseRow + range.lastVis)
    }
  }, [range, baseRow, onViewport])

  const top = win && first >= 0 ? win.heights.prefix(first) : 0
  let sticky: { section: LogSection; row: number } | null = null

  if (props.stickyHeaders && win && range.last >= range.first) {
    const item = rowAt(model, win.base + range.vis)
    const section = item?.kind === 'line' ? sectionAtLine(meta, lineOfItem(item)) : null
    const headerRow = section ? rowOfSection(model, section) : null

    if (section && headerRow !== null && headerRow < win.base + range.vis) {
      sticky = { section, row: headerRow }
    }
  }

  return (
    <div className={classes.bodyWrap}>
      <section
        ref={scrollerRef}
        className={classes.scroller}
        data-variant={variant}
        aria-label={`Log for ${jobName}`}
        // biome-ignore lint/a11y/noNoninteractiveTabindex: a scrollable region must be reachable by keyboard
        tabIndex={0}
        onScroll={onScroll}
        onWheel={releaseTarget}
        onTouchStart={releaseTarget}
        onPointerDown={releaseTarget}
        onKeyDown={(event) => {
          releaseTarget()
          props.onKeyDown(event)
        }}
      >
        <div className={classes.probe} ref={probeRef} aria-hidden>
          <span ref={probeTextRef}>{'0'.repeat(PROBE_CHARS)}</span>
        </div>
        <div ref={spacerRef} className={classes.spacer} style={{ height: win ? win.heights.total() : 0 }} />
        <div
          className={classes.layer}
          data-nowrap={wrap ? undefined : ''}
          style={{ transform: `translateY(${top}px)` }}
          {...props.rowEvents}
        >
          {elements}
        </div>
      </section>
      {sticky && (
        <div className={classes.sticky}>
          <UnstyledButton
            className={`${classes.step} interactive-surface`}
            aria-label={`Go to ${sticky.section.title}`}
            onClick={() => sticky && scrollToRow(sticky.row, 'start')}
          >
            {sticky.section.status && <StatusIcon status={sticky.section.status} />}
            <span className={classes.stepTitle}>{sticky.section.title}</span>
          </UnstyledButton>
        </div>
      )}
      {model.count === 0 && <div className={classes.empty}>{props.emptyLabel}</div>}
    </div>
  )
})
