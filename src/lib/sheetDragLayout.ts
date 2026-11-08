import { createSearchReveal, type Reveal } from '../components/layout/searchReveal'
import { type SheetHandle, sheetHandle } from '../hooks/useSheetStack'
import { iosClasses } from '../theme/ios'
import { isOneOf } from '../utils/isOneOf'
import { detentHeights, expandLevel, SHEET_DETENTS, type SheetDetent, scrimLevel, sheetFrame } from './sheetDetents'
import type { SpringAnimation } from './spring'
import type { GestureStart } from './touchTracker'

const SHEET_SELECTOR = [iosClasses.sheetContent, iosClasses.menuDropdown, iosClasses.pickerDropdown]
  .map((name) => `.${name}`)
  .join(',')
const OVERLAY_SELECTOR = '.mantine-Modal-overlay, .mantine-Menu-overlay, .mantine-Popover-overlay'
const NO_DRAG_SELECTOR = 'input, textarea, select, .monaco-editor, [data-no-sheet-drag]'
const DETENT_ORDER: SheetDetent[] = ['medium', 'large']

export interface SheetDrag {
  sheet: HTMLElement
  handle: SheetHandle
  modal: boolean
  scrollers: HTMLElement[]
  expandable: boolean
  active: boolean
  disposed: boolean
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
  reveal: Reveal | null
  revealOrigin: number | null
}

interface Settling {
  spring: SpringAnimation
  heights: number[]
}

export const settling = new WeakMap<HTMLElement, Settling>()
export const live = new Set<SheetDrag>()
let candidate: SheetDrag | null = null
let revealRoot: HTMLElement | null = null
export const reveals = createSearchReveal(() => revealRoot)

export const expandableCandidate = (): boolean => candidate?.expandable === true

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

export function begin({ target }: GestureStart): SheetDrag | null {
  const sheet = target?.closest<HTMLElement>(SHEET_SELECTOR)

  if (!target || !sheet) {
    return null
  }

  const modal = iosClasses.sheetContent !== undefined && sheet.classList.contains(iosClasses.sheetContent)
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
  revealRoot = sheet
  const reveal = inHeader ? null : reveals.closed()

  candidate = {
    sheet,
    handle,
    modal,
    scrollers,
    expandable: detents.length > 1 && sheet.dataset.detent !== 'large',
    active: false,
    disposed: false,
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
    reveal,
    revealOrigin: null,
  }

  return candidate
}

function measureHeights({ sheet, modal }: SheetDrag): number[] {
  const detents = sheet.dataset.detents?.split(',').filter((value) => isOneOf(SHEET_DETENTS, value))
  const inner = sheet.parentElement

  if (modal && detents && inner) {
    const style = getComputedStyle(inner)

    return detentHeights(detents, inner.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom))
  }

  return [Math.round(sheet.getBoundingClientRect().height)]
}

export function activate(drag: SheetDrag, dy: number): void {
  const { sheet } = drag

  live.add(drag)
  const prior = settling.get(sheet)

  if (prior) {
    drag.heights = prior.heights
    drag.top = prior.heights.at(-1) ?? 0
    drag.base = prior.spring.value
    prior.spring.stop()
    settling.delete(sheet)
  } else {
    drag.heights = measureHeights(drag)
    drag.top = drag.heights.at(-1) ?? 0
    drag.base = drag.top - Math.round(sheet.getBoundingClientRect().height)
  }

  drag.dismissible = !sheet.hasAttribute('data-locked')
  drag.floor = drag.dismissible ? Number.POSITIVE_INFINITY : drag.top - (drag.heights[0] ?? 0)
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

export function render(drag: SheetDrag, position: number): void {
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

export function cleanup(drag: SheetDrag): void {
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
  live.delete(drag)
}

export function commit(drag: SheetDrag, stop: number): void {
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
