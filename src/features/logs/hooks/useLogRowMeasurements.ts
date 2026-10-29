import { type RefCallback, type RefObject, useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { MAX_SCROLL_PX } from '../lib/logViewport'
import type { Metrics } from '../lib/logWindowTypes'
import type { LogWindowController } from './useLogWindow'

export const PROBE_CHARS = 10

interface RowMeasurementOptions {
  controllerRef: RefObject<LogWindowController | null>
  scrollerRef: RefObject<HTMLDivElement | null>
  spacerRef: RefObject<HTMLDivElement | null>
}

interface RowMeasurements {
  metrics: Metrics | null
  probeRef: RefObject<HTMLDivElement | null>
  probeTextRef: RefObject<HTMLSpanElement | null>
  rowRef: RefCallback<HTMLDivElement>
}

export function useLogRowMeasurements({
  controllerRef,
  scrollerRef,
  spacerRef,
}: RowMeasurementOptions): RowMeasurements {
  const probeRef = useRef<HTMLDivElement>(null)
  const probeTextRef = useRef<HTMLSpanElement>(null)
  const observerRef = useRef<ResizeObserver | null>(null)
  const [metrics, setMetrics] = useState<Metrics | null>(null)

  const onResize = useCallback(
    (entries: ResizeObserverEntry[]) => {
      const controller = controllerRef.current
      const win = controller?.winRef.current
      const el = scrollerRef.current

      if (!controller || !win || !el) {
        return
      }

      const top = el.scrollTop
      const anchorRel = win.heights.indexAt(top)
      const offset = top - win.heights.prefix(anchorRel)
      let above = 0
      let changed = false

      for (const entry of entries) {
        const target = entry.target

        if (!(target instanceof HTMLElement)) {
          continue
        }

        const rel = Number(target.dataset.row) - win.base
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

      const held = controller.targetRef.current

      if (win.heights.total() >= MAX_SCROLL_PX) {
        const next = held ? controller.restoreOfTarget(held) : { row: win.base + anchorRel, offset }

        controller.rebuildAround(next.row, next)

        return
      }

      if (held) {
        controller.holdTarget(held)
        controller.applyRestore(controller.restoreOfTarget(held))
        controller.bump()

        return
      }

      if (spacerRef.current) {
        spacerRef.current.style.height = `${win.heights.total()}px`
      }

      if (controller.stickEnd.current) {
        el.scrollTop = el.scrollHeight
      } else if (above !== 0) {
        el.scrollTop = top + above
      }

      controller.updateRange()
      controller.bump()
    },
    [controllerRef, scrollerRef, spacerRef]
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
      const controller = controllerRef.current
      const held = controller?.targetRef.current

      if (controller && held) {
        controller.applyRestore(controller.restoreOfTarget(held))
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
  }, [controllerRef, scrollerRef])

  return { metrics, probeRef, probeTextRef, rowRef }
}
