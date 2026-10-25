import { useEffect } from 'react'

import { nearestStop } from '../lib/gestures'
import { prefersReducedMotion } from '../lib/motion'
import {
  detentHeights,
  displayPosition,
  expandLevel,
  FLICK_SPEED,
  type SheetDetent,
  scrimLevel,
  sheetFrame,
  stopPositions,
} from '../lib/sheetDetents'
import { animateSpring, SPRING_DEFAULT, SPRING_MOMENTUM, type SpringAnimation } from '../lib/spring'
import { type GestureStart, trackTouches } from '../lib/touchTracker'
import { iosClasses } from '../theme/ios'

import { type SheetHandle, sheetHandle } from './useSheetStack'

const SHEET_SELECTOR = [iosClasses.sheetContent, iosClasses.menuDropdown, iosClasses.pickerDropdown]
  .map((name) => `.${name}`)
  .join(',')
const OVERLAY_SELECTOR = '.mantine-Modal-overlay, .mantine-Menu-overlay, .mantine-Popover-overlay'
const NO_DRAG_SELECTOR = 'input, textarea, select, .monaco-editor, [data-no-sheet-drag]'
const DISMISS_CLEARANCE = 96
const FADE_MS = 150
const DETENT_ORDER: SheetDetent[] = ['medium', 'large']

interface SheetDrag {
  sheet: HTMLElement
  handle: SheetHandle
  modal: boolean
  scrollers: HTMLElement[]
  expandable: boolean
  active: boolean
  heights: number[]
  top: number
  floor: number
  dismissible: boolean
  base: number
  anchor: number
  current: number
  overlay: HTMLElement | null
  overlayDuration: string
  parent: HTMLElement | null
  locks: { element: HTMLElement; overflow: string }[]
}

interface Settling {
  spring: SpringAnimation
  heights: number[]
}

const settling = new WeakMap<HTMLElement, Settling>()
let candidate: SheetDrag | null = null

function lastOverlay(): HTMLElement | null {
  const overlays = document.querySelectorAll<HTMLElement>(OVERLAY_SELECTOR)

  return overlays[overlays.length - 1] ?? null
}

async function dismissFloating(): Promise<boolean> {
  lastOverlay()?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }))

  return true
}

const floatingHandle: SheetHandle = { requestClose: dismissFloating, closeNow: dismissFloating }

function closeButtonHandle(sheet: HTMLElement): SheetHandle | undefined {
  const button = sheet.querySelector<HTMLElement>('.mantine-Modal-close')

  if (!button) {
    return undefined
  }

  const close = async () => {
    button.click()

    return true
  }

  return { requestClose: close, closeNow: close }
}

function scrollersWithin(from: Element, sheet: Element): HTMLElement[] {
  const found: HTMLElement[] = []

  for (let element: Element | null = from; element && element !== sheet; element = element.parentElement) {
    if (
      element instanceof HTMLElement &&
      element.scrollHeight > element.clientHeight &&
      /(auto|scroll)/.test(getComputedStyle(element).overflowY)
    ) {
      found.push(element)
    }
  }

  return found
}

function begin({ target }: GestureStart): SheetDrag | null {
  const sheet = target?.closest<HTMLElement>(SHEET_SELECTOR)

  if (!target || !sheet) {
    return null
  }

  const modal = sheet.classList.contains(iosClasses.sheetContent)
  const handle = modal ? (sheetHandle(sheet) ?? closeButtonHandle(sheet)) : floatingHandle

  if (!handle) {
    return null
  }

  const inHeader = target.closest(`.${iosClasses.sheetHeader}`) !== null
  const scrollers = scrollersWithin(target, sheet)

  if (!inHeader && (target.closest(NO_DRAG_SELECTOR) || scrollers.some((element) => element.scrollTop > 0))) {
    return null
  }

  const detents = sheet.dataset.detents?.split(',') ?? []

  candidate = {
    sheet,
    handle,
    modal,
    scrollers,
    expandable: detents.length > 1 && sheet.dataset.detent !== 'large',
    active: false,
    heights: [],
    top: 0,
    floor: Number.POSITIVE_INFINITY,
    dismissible: true,
    base: 0,
    anchor: 0,
    current: 0,
    overlay: null,
    overlayDuration: '',
    parent: null,
    locks: [],
  }

  return candidate
}

function measureHeights({ sheet, modal }: SheetDrag): number[] {
  const detents = sheet.dataset.detents?.split(',') as SheetDetent[] | undefined
  const inner = sheet.parentElement

  if (modal && detents && inner) {
    const style = getComputedStyle(inner)

    return detentHeights(detents, inner.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom))
  }

  return [Math.round(sheet.getBoundingClientRect().height)]
}

function activate(drag: SheetDrag, dy: number) {
  const { sheet } = drag
  const prior = settling.get(sheet)

  if (prior) {
    drag.heights = prior.heights
    drag.top = prior.heights[prior.heights.length - 1]
    drag.base = prior.spring.value
    prior.spring.stop()
    settling.delete(sheet)
  } else {
    drag.heights = measureHeights(drag)
    drag.top = drag.heights[drag.heights.length - 1]
    drag.base = drag.top - Math.round(sheet.getBoundingClientRect().height)
  }

  drag.dismissible = !sheet.hasAttribute('data-locked')
  drag.floor = drag.dismissible ? Number.POSITIVE_INFINITY : drag.top - drag.heights[0]
  drag.anchor = dy
  drag.current = drag.base
  drag.overlay = lastOverlay()
  drag.overlayDuration = drag.overlay?.style.transitionDuration ?? ''
  drag.parent = drag.modal ? document.querySelector<HTMLElement>('[data-sheet-covered="1"]') : null

  if (drag.overlay) {
    drag.overlay.style.transitionDuration = '0ms'
  }

  if (drag.parent) {
    drag.parent.style.transitionDuration = '0ms'
  }

  sheet.style.maxHeight = 'none'
  drag.locks = drag.scrollers.map((element) => ({ element, overflow: element.style.overflowY }))

  for (const { element } of drag.locks) {
    element.style.overflowY = 'hidden'
  }

  drag.active = true
}

function render(drag: SheetDrag, position: number) {
  const { sheet, heights, overlay, parent } = drag
  const frame = sheetFrame(position, heights)
  const level = scrimLevel(frame, heights)
  const expand = expandLevel(frame.height, heights)

  if (heights.length > 1 || position < 0) {
    sheet.style.height = `${frame.height}px`
  } else {
    sheet.style.removeProperty('height')
  }

  sheet.style.translate = `0 ${frame.offset}px`

  if (expand !== null) {
    sheet.style.setProperty('--sheet-expand', String(expand))
  }

  if (overlay) {
    overlay.style.opacity = String(level)
  }

  parent?.style.setProperty('--sheet-cover', String(level))
}

function cleanup(drag: SheetDrag) {
  const { sheet, overlay, parent, locks } = drag

  for (const property of ['height', 'max-height', 'translate', '--sheet-expand']) {
    sheet.style.removeProperty(property)
  }

  overlay?.style.removeProperty('opacity')

  if (overlay) {
    overlay.style.transitionDuration = drag.overlayDuration
  }

  parent?.style.removeProperty('--sheet-cover')
  parent?.style.removeProperty('transition-duration')

  for (const { element, overflow } of locks) {
    element.style.overflowY = overflow
  }

  settling.delete(sheet)
}

function commit(drag: SheetDrag, stop: number) {
  const detents = drag.sheet.dataset.detents?.split(',')

  if (detents) {
    const names = DETENT_ORDER.filter((name) => detents.includes(name))
    const index = drag.heights.indexOf(drag.top - stop)

    if (names[index]) {
      drag.sheet.dataset.detent = names[index]
    }
  }

  cleanup(drag)
}

function glide(drag: SheetDrag, to: number, velocity: number): SpringAnimation {
  if (prefersReducedMotion() && Math.abs(to - drag.current) > 1) {
    drag.sheet.animate({ opacity: [0.4, 1] }, { duration: FADE_MS, easing: 'ease-out' })
  }

  const spring = animateSpring({
    from: drag.current,
    to,
    velocity,
    config: Math.abs(velocity) > FLICK_SPEED ? SPRING_MOMENTUM : SPRING_DEFAULT,
    onUpdate: (position) => {
      drag.current = position
      render(drag, position)
    },
  })

  settling.set(drag.sheet, { spring, heights: drag.heights })

  return spring
}

async function settleTo(drag: SheetDrag, stop: number, velocity: number): Promise<boolean> {
  if (!(await glide(drag, stop, velocity).finished)) {
    return false
  }

  commit(drag, stop)

  return true
}

const restStop = (drag: SheetDrag): number => nearestStop(drag.current, 0, stopPositions(drag.heights, false))

async function dismiss(drag: SheetDrag, velocity: number) {
  const { sheet, handle } = drag

  if (prefersReducedMotion()) {
    const fade = sheet.animate({ opacity: [1, 0] }, { duration: FADE_MS, fill: 'forwards' })

    await fade.finished

    if (!(await handle.closeNow())) {
      fade.cancel()
      await settleTo(drag, restStop(drag), 0)
    }

    return
  }

  if (!(await glide(drag, drag.top + DISMISS_CLEARANCE, velocity).finished)) {
    return
  }

  if (await handle.closeNow()) {
    cleanup(drag)
  } else {
    await settleTo(drag, restStop(drag), 0)
  }
}

async function release(drag: SheetDrag, velocity: number) {
  const target = nearestStop(drag.current, velocity, stopPositions(drag.heights, drag.dismissible))

  if (!drag.dismissible || target !== drag.top) {
    await settleTo(drag, target, velocity)

    return
  }

  if (!drag.sheet.hasAttribute('data-dirty')) {
    await dismiss(drag, velocity)

    return
  }

  if (await settleTo(drag, restStop(drag), velocity)) {
    await drag.handle.requestClose()
  }
}

export function useSheetDrag(active: boolean): void {
  useEffect(() => {
    if (!active) {
      return
    }

    return trackTouches<SheetDrag>(document, {
      axis: 'y',
      retry: true,
      begin,
      accept: (_dx, dy) => dy > 0 || (candidate?.expandable === true && dy < 0),
      move: (drag, { dy }) => {
        if (!drag.active) {
          activate(drag, dy)
        }

        drag.current = displayPosition(drag.base + dy - drag.anchor, drag.top, drag.floor)
        render(drag, drag.current)
      },
      end: (drag, _gesture, { vy }) => {
        void release(drag, vy)
      },
      cancel: (drag) => {
        void settleTo(drag, restStop(drag), 0)
      },
    })
  }, [active])
}
