import { type Ref, type RefObject, useCallback, useEffect, useImperativeHandle, useRef } from 'react'
import { registerScrollToTop } from '../layout/scrollToTop'
import { needsRebase } from './logViewport'
import { type LogBodyHandle, restoreFor, type ScrollAlign } from './logWindowTypes'
import type { LogWindowController } from './useLogWindow'

const REBASE_DELAY_MS = 150

interface LogScrollOptions {
  controller: LogWindowController
  scrollerRef: RefObject<HTMLDivElement | null>
  variant: 'desktop' | 'narrow' | 'phone'
  ref?: Ref<LogBodyHandle> | undefined
}

interface LogScroll {
  onScroll: () => void
  scrollToRow: LogBodyHandle['scrollToRow']
}

export function useLogScroll({ controller, scrollerRef, variant, ref }: LogScrollOptions): LogScroll {
  const {
    winRef,
    restoreRef,
    queuedRef,
    stickEnd,
    latest,
    updateRange,
    applyRestore,
    rebuildAround,
    anchor,
    holdTarget,
  } = controller
  const frameRef = useRef<number | null>(null)
  const rebaseTimer = useRef<number | undefined>(undefined)

  useEffect(
    () => () => {
      if (frameRef.current !== null) {
        cancelAnimationFrame(frameRef.current)
      }

      window.clearTimeout(rebaseTimer.current)
    },
    []
  )

  const maybeRebase = useCallback(() => {
    const win = winRef.current
    const current = anchor()

    if (win && current && needsRebase(win.base, win.size, latest.current.model.count, current.row)) {
      rebuildAround(current.row, current)
    }
  }, [anchor, rebuildAround, winRef, latest])

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
  }, [maybeRebase, updateRange, scrollerRef, stickEnd])

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
    [applyRestore, rebuildAround, holdTarget, winRef, scrollerRef, latest, queuedRef, restoreRef]
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
    [scrollToRow, latest, winRef, scrollerRef]
  )

  useEffect(() => {
    if (variant !== 'phone') {
      return
    }

    return registerScrollToTop({
      isActive: () => !!scrollerRef.current && !scrollerRef.current.closest('[data-sheet-covered]'),
      scrollToTop: () => scrollToRow(0, 'start'),
    })
  }, [variant, scrollToRow, scrollerRef])

  return { onScroll, scrollToRow }
}
