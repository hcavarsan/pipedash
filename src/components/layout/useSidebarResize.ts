import { useLocalStorage, useViewportSize } from '@mantine/hooks'
import { type KeyboardEvent, type PointerEvent, useEffect, useRef, useState } from 'react'

import {
  clampSidebarWidth,
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_RAIL_WIDTH,
  type SidebarSize,
  sidebarCssWidth,
  sizeAfterStep,
  sizeAtOffset,
} from './sidebarWidth'
import { createSpringValue } from './springValue'

const KEYBOARD_STEP = 16

export const useSidebarResize = (resizable: boolean) => {
  const [compact, setCompact] = useLocalStorage({ key: 'pipedash-sidebar-compact', defaultValue: false })
  const [width, setWidth] = useLocalStorage({ key: 'pipedash-sidebar-width', defaultValue: SIDEBAR_DEFAULT_WIDTH })
  const [presentation, setPresentation] = useState<number | null>(null)
  const [resizing, setResizing] = useState(false)
  const [spring] = useState(() => createSpringValue(SIDEBAR_DEFAULT_WIDTH, setPresentation))
  const { width: viewportWidth } = useViewportSize()
  const drag = useRef<{ origin: number; time: number; velocity: number; width: number } | null>(null)

  useEffect(
    () => () => {
      spring.grab()
    },
    [spring]
  )

  useEffect(() => {
    if (!resizable) {
      drag.current = null
      spring.grab()
      setResizing(false)
      setPresentation(null)
    }
  }, [resizable, spring])

  const save = (next: SidebarSize) => {
    setWidth(next.width)
    setCompact(next.compact)
  }

  const onPointerDown = (event: PointerEvent<HTMLElement>) => {
    if (!resizable || event.button !== 0) {
      return
    }
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    const rendered = event.currentTarget.parentElement?.getBoundingClientRect().width
    const current = rendered || (compact ? SIDEBAR_RAIL_WIDTH : clampSidebarWidth(width, viewportWidth))
    spring.grab()
    spring.set(current)
    drag.current = { origin: event.clientX - current, time: event.timeStamp, velocity: 0, width: current }
    setResizing(true)
  }

  const onPointerMove = (event: PointerEvent<HTMLElement>) => {
    const current = drag.current
    if (!current) {
      return
    }
    const next = Math.max(
      SIDEBAR_RAIL_WIDTH,
      Math.min(event.clientX - current.origin, clampSidebarWidth(SIDEBAR_MAX_WIDTH, viewportWidth))
    )
    const elapsed = event.timeStamp - current.time
    current.velocity = elapsed > 0 ? ((next - current.width) / elapsed) * 1000 : 0
    current.time = event.timeStamp
    current.width = next
    spring.set(next)
  }

  const onLostPointerCapture = () => {
    const current = drag.current
    if (!current) {
      return
    }
    drag.current = null
    setResizing(false)
    const next = sizeAtOffset(current.width, { width, compact }, viewportWidth)
    const target = next.compact ? SIDEBAR_RAIL_WIDTH : next.width
    save(next)
    if (target === current.width) {
      setPresentation(null)
      return
    }
    const velocity = performance.now() - current.time < 100 ? current.velocity : 0
    void spring.to(target, velocity).then((finished) => {
      if (finished) {
        setPresentation(null)
      }
    })
  }

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const step = { ArrowRight: KEYBOARD_STEP, ArrowLeft: -KEYBOARD_STEP }[event.key]
    if (step === undefined) {
      return
    }
    event.preventDefault()
    spring.grab()
    setPresentation(null)
    save(sizeAfterStep(step, { width, compact }, viewportWidth))
  }

  const renderedWidth =
    presentation === null
      ? null
      : Math.max(SIDEBAR_RAIL_WIDTH, Math.min(presentation, clampSidebarWidth(SIDEBAR_MAX_WIDTH, viewportWidth)))

  return {
    compact,
    navbarWidth: renderedWidth ?? (compact ? SIDEBAR_RAIL_WIDTH : sidebarCssWidth(width)),
    resizing,
    instant: presentation !== null,
    toggle: () => {
      spring.grab()
      setPresentation(null)
      setCompact((value) => !value)
    },
    handleProps: {
      role: 'separator',
      tabIndex: 0,
      'aria-label': 'Resize sidebar',
      'aria-orientation': 'vertical',
      'aria-valuemin': SIDEBAR_RAIL_WIDTH,
      'aria-valuemax': clampSidebarWidth(SIDEBAR_MAX_WIDTH, viewportWidth),
      'aria-valuenow': renderedWidth ?? (compact ? SIDEBAR_RAIL_WIDTH : clampSidebarWidth(width, viewportWidth)),
      onPointerDown,
      onPointerMove,
      onLostPointerCapture,
      onKeyDown,
      onDoubleClick: () => {
        spring.grab()
        setPresentation(null)
        save({ width: SIDEBAR_DEFAULT_WIDTH, compact: false })
      },
    },
  } as const
}
