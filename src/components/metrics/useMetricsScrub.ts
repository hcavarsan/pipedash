import { type RefObject, useEffect, useEffectEvent, useRef } from 'react'

import { haptic } from '../../lib/haptics'
import { trackTouches } from '../../lib/touchTracker'

import { nearestIndex, type PlotSpan } from './chartScrub'
import { SHELL_MARGIN } from './MetricsPlot'

interface Scrub {
  span: PlotSpan
  last: number | null
}

interface MetricsScrubOptions {
  plotRef: RefObject<HTMLDivElement | null>
  enabled: boolean
  pointCount: number
  band: boolean
  yAxisWidth: number
  selectedIndex: number | null | undefined
  onDataPointClick: ((index: number | null) => void) | undefined
}

const plotSpan = (plot: HTMLElement, yAxisWidth: number): PlotSpan => {
  const rect = plot.getBoundingClientRect()
  const left = SHELL_MARGIN.left + yAxisWidth

  return { left: rect.left + left, width: rect.width - left - SHELL_MARGIN.right }
}

export function useMetricsScrub({
  plotRef,
  enabled,
  pointCount,
  band,
  yAxisWidth,
  selectedIndex,
  onDataPointClick,
}: MetricsScrubOptions): void {
  const scrubbed = useRef(false)

  const toggle = useEffectEvent((index: number) => {
    onDataPointClick?.(selectedIndex === index ? null : index)
  })
  const select = useEffectEvent((index: number) => {
    onDataPointClick?.(index)
  })

  useEffect(() => {
    const plot = plotRef.current

    if (!enabled || !plot) {
      return
    }

    const handleClick = (event: globalThis.MouseEvent) => {
      if (scrubbed.current) {
        scrubbed.current = false

        return
      }

      if (event.target instanceof Element && event.target.closest('[role="button"]')) {
        return
      }

      const index = nearestIndex(event.clientX, plotSpan(plot, yAxisWidth), pointCount, band)

      if (index !== null) {
        toggle(index)
      }
    }

    plot.addEventListener('click', handleClick)

    const untrack = trackTouches<Scrub>(plot, {
      axis: 'x',
      begin: () => {
        scrubbed.current = false

        return { span: plotSpan(plot, yAxisWidth), last: null }
      },
      move: (scrub, drag) => {
        const index = nearestIndex(drag.startX + drag.dx, scrub.span, pointCount, band)

        if (index === null || index === scrub.last) {
          return
        }

        scrub.last = index
        scrubbed.current = true
        haptic('selection')
        select(index)
      },
      end: () => undefined,
    })

    return () => {
      plot.removeEventListener('click', handleClick)
      untrack()
    }
  }, [plotRef, enabled, pointCount, band, yAxisWidth])
}
