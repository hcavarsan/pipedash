export const SIDEBAR_RAIL_WIDTH = 68
export const SIDEBAR_MIN_WIDTH = 200
export const SIDEBAR_MAX_WIDTH = 420
export const SIDEBAR_DEFAULT_WIDTH = 248

const MAX_VIEWPORT_SHARE = 0.4
const SNAP_TO_RAIL_BELOW = (SIDEBAR_RAIL_WIDTH + SIDEBAR_MIN_WIDTH) / 2

export interface SidebarSize {
  width: number
  compact: boolean
}

export const clampSidebarWidth = (width: number, viewportWidth: number) => {
  const max = Math.max(SIDEBAR_MIN_WIDTH, Math.min(SIDEBAR_MAX_WIDTH, Math.floor(viewportWidth * MAX_VIEWPORT_SHARE)))

  return Math.min(Math.max(Math.round(width), SIDEBAR_MIN_WIDTH), max)
}

export const sizeAtOffset = (offset: number, current: SidebarSize, viewportWidth: number): SidebarSize =>
  offset < SNAP_TO_RAIL_BELOW
    ? { width: current.width, compact: true }
    : { width: clampSidebarWidth(offset, viewportWidth), compact: false }

export interface SidebarDrag {
  resizable: boolean
  draft: SidebarSize | null
}

export type SidebarDragEvent =
  | { type: 'start'; size: SidebarSize }
  | { type: 'move'; offset: number; viewportWidth: number }
  | { type: 'stop' }
  | { type: 'resizable'; resizable: boolean }

export const nextSidebarDrag = (drag: SidebarDrag, event: SidebarDragEvent): SidebarDrag => {
  switch (event.type) {
    case 'start':
      return drag.resizable ? { ...drag, draft: event.size } : drag
    case 'move':
      return drag.draft ? { ...drag, draft: sizeAtOffset(event.offset, drag.draft, event.viewportWidth) } : drag
    case 'stop':
      return { ...drag, draft: null }
    case 'resizable':
      return { resizable: event.resizable, draft: event.resizable ? drag.draft : null }
  }
}

export const sizeAfterStep = (step: number, current: SidebarSize, viewportWidth: number): SidebarSize => {
  if (current.compact) {
    return step > 0 ? { width: current.width, compact: false } : current
  }
  const width = clampSidebarWidth(current.width, viewportWidth)
  if (width + step < SIDEBAR_MIN_WIDTH) {
    return { width, compact: true }
  }

  return { width: clampSidebarWidth(width + step, viewportWidth), compact: false }
}

export const sidebarCssWidth = (width: number) => {
  const stored = Number.isFinite(width) ? Math.min(width, SIDEBAR_MAX_WIDTH) : SIDEBAR_DEFAULT_WIDTH

  return `clamp(${SIDEBAR_MIN_WIDTH}px, ${stored}px, ${MAX_VIEWPORT_SHARE * 100}vw)`
}
