import { UnstyledButton } from '@mantine/core'
import {
  type HTMLAttributes,
  type ReactNode,
  type Ref,
  useEffect,
  useEffectEvent,
  useLayoutEffect,
  useRef,
} from 'react'
import type { LogDocumentMeta, LogSection } from '../../types/logs'
import { StatusIcon } from '../atoms/StatusBadge'
import { LogRow, type MatchTriple } from './LogRow'
import classes from './LogViewer.module.css'
import type { LineState } from './lineCache'
import { isOpen, lineOfItem, type RowModel, rowAt, rowOfSection, sectionAtLine } from './logRows'
import type { LogBodyHandle } from './logWindowTypes'
import { PROBE_CHARS, useLogRowMeasurements } from './useLogRowMeasurements'
import { useLogScroll } from './useLogScroll'
import { type LogWindowController, useLogWindow } from './useLogWindow'

const TIMESTAMP_CH = 13
const GUTTER_EXTRA_CH = 4

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
  onKeyDown: (event: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'preventDefault'>) => void
  ref?: Ref<LogBodyHandle>
}

export function LogBody({ ref, ...props }: LogBodyProps): ReactNode {
  const { meta, model, wrap, timestamps, variant, jobName, getLine, onVisibleLines, onViewport } = props
  const scrollerRef = useRef<HTMLDivElement>(null)
  const spacerRef = useRef<HTMLDivElement>(null)
  const renderedRef = useRef<number[]>([])
  const focusInside = useRef(false)
  const lastLines = useRef('')
  const controllerRef = useRef<LogWindowController | null>(null)

  useLayoutEffect(() => {
    if (focusInside.current && document.activeElement === document.body) {
      scrollerRef.current?.focus({ preventScroll: true })
    }
  })

  const { metrics, probeRef, probeTextRef, rowRef } = useLogRowMeasurements({ controllerRef, scrollerRef, spacerRef })

  const gutterCh = Math.max(3, String(meta.line_count).length) + GUTTER_EXTRA_CH
  const cols = metrics
    ? Math.max(1, Math.floor(metrics.width / metrics.charWidth) - gutterCh - (timestamps ? TIMESTAMP_CH : 0) - 1)
    : 0
  const layoutKey = metrics ? `${metrics.rowHeight}:${wrap ? cols : 0}:${wrap}` : ''

  const controller = useLogWindow({
    meta,
    model,
    wrap,
    getLine,
    metrics,
    cols,
    layoutKey,
    scrollerRef,
    spacerRef,
    renderedRef,
  })

  controllerRef.current = controller

  const { onScroll, scrollToRow } = useLogScroll({ controller, scrollerRef, variant, ref })
  const { range, winRef, releaseTarget } = controller

  const onInput = useEffectEvent(() => {
    releaseTarget()
  })
  const onKey = useEffectEvent((event: KeyboardEvent) => {
    releaseTarget()
    props.onKeyDown(event)
  })

  useEffect(() => {
    const el = scrollerRef.current

    if (!el) {
      return undefined
    }

    const onFocusIn = (): void => {
      focusInside.current = true
    }

    const onFocusOut = (event: FocusEvent): void => {
      if (event.target instanceof Node && event.target.isConnected) {
        focusInside.current = false
      }
    }

    const release = (): void => {
      onInput()
    }

    const keydown = (event: KeyboardEvent): void => {
      onKey(event)
    }

    el.addEventListener('focusin', onFocusIn)
    el.addEventListener('focusout', onFocusOut)
    el.addEventListener('wheel', release, { passive: true })
    el.addEventListener('touchstart', release, { passive: true })
    el.addEventListener('pointerdown', release)
    el.addEventListener('keydown', keydown)

    return () => {
      el.removeEventListener('focusin', onFocusIn)
      el.removeEventListener('focusout', onFocusOut)
      el.removeEventListener('wheel', release)
      el.removeEventListener('touchstart', release)
      el.removeEventListener('pointerdown', release)
      el.removeEventListener('keydown', keydown)
    }
  }, [])

  const win = winRef.current
  const size = win?.size ?? 0
  const first = Math.max(0, Math.min(range.first, size - 1))
  const last = Math.min(range.last, size - 1)
  const elements: ReactNode[] = []
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
          <div key={`step:${item.section.id}`} ref={rowRef} data-row={row}>
            <LogRow
              kind="step"
              section={item.section}
              open={isOpen(item.section, props.overrides)}
              styleClasses={props.styleClasses}
              gutterCh={gutterCh}
              timestamps={timestamps}
              onToggle={props.onToggle}
            />
          </div>
        )
        continue
      }

      const group = props.groups.get(item.line) ?? null
      const { selection, currentMatch } = props

      lines.push(item.line)
      elements.push(
        <div key={`line:${item.line}`} ref={rowRef} data-row={row}>
          <LogRow
            kind="line"
            line={item.line}
            data={getLine(item.line)}
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
        tabIndex={-1}
        onScroll={onScroll}
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
}
