import { useEffect, useSyncExternalStore } from 'react'

import { scrollBehavior } from '../lib/motion'

const KEYBOARD_THRESHOLD = 100
const REVEAL_MARGIN = 12
const FIELD_SELECTOR = 'input, textarea, select, [contenteditable="true"]'
const SHEET_SELECTOR = '[data-modal-content]'

let currentInset = 0
const listeners = new Set<() => void>()

export function keyboardInset(layoutHeight: number, viewportHeight: number, viewportOffsetTop: number): number {
  return Math.max(0, Math.round(layoutHeight - viewportHeight - viewportOffsetTop))
}

export const keyboardOpen = (inset: number): boolean => inset > KEYBOARD_THRESHOLD

export function scrollDeltaToReveal(
  targetTop: number,
  targetBottom: number,
  viewTop: number,
  viewBottom: number,
  margin = REVEAL_MARGIN
): number {
  const top = viewTop + margin
  const bottom = viewBottom - margin

  if (targetBottom - targetTop > bottom - top || targetTop < top) {
    return targetTop - top
  }

  return targetBottom > bottom ? targetBottom - bottom : 0
}

export function useKeyboardOpen(): boolean {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener)

      return () => listeners.delete(listener)
    },
    () => keyboardOpen(currentInset)
  )
}

function publishInset(inset: number) {
  currentInset = inset

  for (const listener of listeners) {
    listener()
  }
}

function revealFocusedField() {
  const field = document.activeElement

  if (!(field instanceof HTMLElement) || !field.matches(FIELD_SELECTOR)) {
    return
  }

  const boundary = field.closest(SHEET_SELECTOR) ?? document.body
  const viewport = window.visualViewport
  const visibleTop = viewport?.offsetTop ?? 0
  const visibleBottom = visibleTop + (viewport?.height ?? window.innerHeight)

  for (let parent = field.parentElement; parent && parent !== boundary.parentElement; parent = parent.parentElement) {
    const overflows = parent.scrollHeight > parent.clientHeight

    if (overflows && /(auto|scroll)/.test(getComputedStyle(parent).overflowY)) {
      const target = field.getBoundingClientRect()
      const view = parent.getBoundingClientRect()
      const delta = scrollDeltaToReveal(
        target.top,
        target.bottom,
        Math.max(view.top, visibleTop),
        Math.min(view.bottom, visibleBottom)
      )

      if (delta !== 0) {
        parent.scrollBy({ top: delta, behavior: scrollBehavior() })
      }
    }
  }
}

export function useKeyboardInset(active: boolean): void {
  useEffect(() => {
    const viewport = window.visualViewport

    if (!active || !viewport) {
      return
    }

    const root = document.documentElement
    let frame = 0
    let revealRequested = false

    const apply = () => {
      frame = 0

      const inset = keyboardInset(window.innerHeight, viewport.height, viewport.offsetTop)
      const changed = inset !== currentInset

      if (changed) {
        publishInset(inset)
        root.style.setProperty('--keyboard-inset', `${inset}px`)
        root.toggleAttribute('data-keyboard', keyboardOpen(inset))
      }

      if (inset > 0 && (changed || revealRequested)) {
        revealFocusedField()
      }

      revealRequested = false
    }

    const schedule = () => {
      if (frame === 0) {
        frame = requestAnimationFrame(apply)
      }
    }

    const onFocusIn = () => {
      revealRequested = true
      schedule()
    }

    currentInset = -1
    apply()
    viewport.addEventListener('resize', schedule)
    viewport.addEventListener('scroll', schedule)
    document.addEventListener('focusin', onFocusIn)

    return () => {
      cancelAnimationFrame(frame)
      viewport.removeEventListener('resize', schedule)
      viewport.removeEventListener('scroll', schedule)
      document.removeEventListener('focusin', onFocusIn)
      publishInset(0)
      root.style.removeProperty('--keyboard-inset')
      root.removeAttribute('data-keyboard')
    }
  }, [active])
}
