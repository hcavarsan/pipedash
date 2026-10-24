import { useLocalStorage, useViewportSize } from '@mantine/hooks'
import { type KeyboardEvent, type PointerEvent, useEffect, useRef, useState } from 'react'

import {
  clampSidebarWidth,
  nextSidebarDrag,
  SIDEBAR_DEFAULT_WIDTH,
  SIDEBAR_MAX_WIDTH,
  SIDEBAR_RAIL_WIDTH,
  type SidebarDrag,
  type SidebarSize,
  sidebarCssWidth,
  sizeAfterStep,
} from './sidebarWidth'

const SNAP_TRANSITION_MS = 200
const KEYBOARD_STEP = 16

export const useSidebarResize = (resizable: boolean) => {
  const [compact, setCompact] = useLocalStorage({ key: 'pipedash-sidebar-compact', defaultValue: false })
  const [width, setWidth] = useLocalStorage({ key: 'pipedash-sidebar-width', defaultValue: SIDEBAR_DEFAULT_WIDTH })
  const [drag, setDrag] = useState<SidebarDrag>({ resizable, draft: null })
  const [snapping, setSnapping] = useState(false)
  const { width: viewportWidth } = useViewportSize()
  const navbarLeft = useRef(0)
  const snapTimer = useRef<ReturnType<typeof setTimeout>>(undefined)

  useEffect(() => () => clearTimeout(snapTimer.current), [])

  if (drag.resizable !== resizable) {
    setDrag(nextSidebarDrag(drag, { type: 'resizable', resizable }))
  }

  const draft = drag.draft
  const size = draft ?? { width, compact }

  const save = (next: SidebarSize) => {
    setWidth(next.width)
    setCompact(next.compact)
  }

  const onPointerDown = (event: PointerEvent<HTMLElement>) => {
    if (event.button !== 0) {
      return
    }
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    navbarLeft.current = event.currentTarget.parentElement?.getBoundingClientRect().left ?? 0
    setDrag(nextSidebarDrag(drag, { type: 'start', size: { width, compact } }))
  }

  const onPointerMove = (event: PointerEvent<HTMLElement>) => {
    const next = nextSidebarDrag(drag, {
      type: 'move',
      offset: event.clientX - navbarLeft.current,
      viewportWidth,
    })
    if (next.draft?.compact !== draft?.compact) {
      clearTimeout(snapTimer.current)
      setSnapping(true)
      snapTimer.current = setTimeout(() => setSnapping(false), SNAP_TRANSITION_MS)
    }
    setDrag(next)
  }

  const onLostPointerCapture = () => {
    if (draft) {
      save(draft)
      setDrag(nextSidebarDrag(drag, { type: 'stop' }))
    }
  }

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const step = { ArrowRight: KEYBOARD_STEP, ArrowLeft: -KEYBOARD_STEP }[event.key]
    if (step === undefined) {
      return
    }
    event.preventDefault()
    save(sizeAfterStep(step, { width, compact }, viewportWidth))
  }

  return {
    compact: size.compact,
    navbarWidth: size.compact ? SIDEBAR_RAIL_WIDTH : sidebarCssWidth(size.width),
    resizing: draft !== null,
    instant: draft !== null && !snapping,
    toggle: () => setCompact((value) => !value),
    handleProps: {
      role: 'separator',
      tabIndex: 0,
      'aria-label': 'Resize sidebar',
      'aria-orientation': 'vertical',
      'aria-valuemin': SIDEBAR_RAIL_WIDTH,
      'aria-valuemax': clampSidebarWidth(SIDEBAR_MAX_WIDTH, viewportWidth),
      'aria-valuenow': size.compact ? SIDEBAR_RAIL_WIDTH : clampSidebarWidth(size.width, viewportWidth),
      onPointerDown,
      onPointerMove,
      onLostPointerCapture,
      onKeyDown,
      onDoubleClick: () => save({ width: SIDEBAR_DEFAULT_WIDTH, compact: false }),
    },
  } as const
}
