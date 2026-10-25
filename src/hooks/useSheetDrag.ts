import { useEffect } from 'react'

import { decideSheetDismiss } from '../lib/gestures'
import { trackTouches } from '../lib/touchTracker'
import { iosClasses } from '../theme/ios'

const SHEET_SELECTOR = [iosClasses.sheetContent, iosClasses.menuDropdown, iosClasses.pickerDropdown]
  .map((name) => `.${name}`)
  .join(',')
const OVERLAY_SELECTOR = '.mantine-Modal-overlay, .mantine-Menu-overlay, .mantine-Popover-overlay'
const NO_DRAG_SELECTOR = 'input, textarea, select, .monaco-editor, [data-no-sheet-drag]'
const SLIDE_MS = 220
const CLOSE_GRACE_MS = 450

interface SheetDrag {
  sheet: HTMLElement
  height: number
  modal: boolean
}

function lastOverlay(): HTMLElement | null {
  const overlays = document.querySelectorAll<HTMLElement>(OVERLAY_SELECTOR)

  return overlays[overlays.length - 1] ?? null
}

function scrolledWithin(from: Element, sheet: Element): boolean {
  for (let element: Element | null = from; element && element !== sheet; element = element.parentElement) {
    if (element.scrollTop > 0 && element.scrollHeight > element.clientHeight) {
      return true
    }
  }

  return false
}

function begin(event: TouchEvent): SheetDrag | null {
  const target = event.target instanceof Element ? event.target : null
  const sheet = target?.closest<HTMLElement>(SHEET_SELECTOR)

  if (!target || !sheet || sheet.classList.contains(iosClasses.alertContent)) {
    return null
  }

  const modal = sheet.classList.contains(iosClasses.sheetContent)

  if (modal && !sheet.querySelector('.mantine-Modal-close')) {
    return null
  }

  const inHeader = target.closest(`.${iosClasses.sheetHeader}`) !== null

  if (!inHeader && (target.closest(NO_DRAG_SELECTOR) || scrolledWithin(target, sheet))) {
    return null
  }

  return { sheet, height: sheet.getBoundingClientRect().height, modal }
}

function slide(sheet: HTMLElement, from: number, to: number, onDone?: () => void) {
  const animation = sheet.animate([{ transform: `translateY(${from}px)` }, { transform: `translateY(${to}px)` }], {
    duration: SLIDE_MS,
    easing: 'cubic-bezier(0.32, 0.72, 0, 1)',
    fill: 'forwards',
  })

  animation.onfinish = () => onDone?.()

  return animation
}

function reset(sheet: HTMLElement) {
  for (const animation of sheet.getAnimations()) {
    animation.cancel()
  }
  sheet.style.transform = ''
  sheet.style.transition = ''

  const overlay = lastOverlay()

  if (overlay) {
    overlay.style.opacity = ''
  }
}

function close({ sheet, modal }: SheetDrag) {
  if (modal) {
    sheet.querySelector<HTMLElement>('.mantine-Modal-close')?.click()
  } else {
    lastOverlay()?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))
  }

  window.setTimeout(() => {
    if (sheet.isConnected) {
      reset(sheet)
    }
  }, CLOSE_GRACE_MS)
}

export function useSheetDrag(active: boolean): void {
  useEffect(() => {
    if (!active) {
      return
    }

    return trackTouches<SheetDrag>(document, {
      axis: 'y',
      begin,
      accept: (_dx, dy) => dy > 0,
      move: ({ sheet, height }, { dy }) => {
        sheet.style.transition = 'none'
        sheet.style.transform = `translateY(${Math.max(0, dy)}px)`

        const overlay = lastOverlay()

        if (overlay) {
          overlay.style.opacity = String(Math.max(0, 1 - dy / height))
        }
      },
      end: (drag, { dy }, { vy }) => {
        if (decideSheetDismiss(dy, vy, drag.height) === 'dismiss') {
          slide(drag.sheet, dy, drag.height, () => close(drag))
        } else {
          slide(drag.sheet, dy, 0, () => reset(drag.sheet))
        }
      },
      cancel: ({ sheet }) => reset(sheet),
    })
  }, [active])
}
