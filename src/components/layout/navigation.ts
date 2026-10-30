import { rubberband } from '../../lib/spring'
import type { MobileTab } from './mobileChrome'

export type TransitionKind = 'push' | 'pop' | 'none'

export interface ScreenRef {
  tab: MobileTab
  depth: number
}

export function transitionKind(prev: ScreenRef, next: ScreenRef): TransitionKind {
  if (prev.tab !== next.tab || prev.depth === next.depth) {
    return 'none'
  }

  return next.depth > prev.depth ? 'push' : 'pop'
}

const entries = new Map<number, string>()

export function historyIndex(): number {
  const idx: unknown = window.history.state?.idx

  return typeof idx === 'number' ? idx : 0
}

export function recordEntry(index: number, path: string): void {
  for (const key of entries.keys()) {
    if (key > index) {
      entries.delete(key)
    }
  }
  entries.set(index, path)
}

export function previousEntry(index: number): string | undefined {
  return entries.get(index - 1)
}

export function resolveBack(index: number, previous: string | undefined, parent: string): 'pop' | 'replace' {
  return index > 0 && previous === parent ? 'pop' : 'replace'
}

export type TabAction = 'switch' | 'pop-to-root' | 'scroll-top'

export function tabAction(active: MobileTab, pressed: MobileTab, pushed: boolean): TabAction {
  if (pressed !== active) {
    return 'switch'
  }

  return pushed ? 'pop-to-root' : 'scroll-top'
}

const clamp01 = (value: number): number => Math.min(1, Math.max(0, value))

export function titleCollapse(titleBottom: number, barBottom: number, titleHeight: number): number {
  const fade = Math.max(1, titleHeight * 0.5)

  return clamp01(1 - (titleBottom - barBottom) / fade)
}

const EDGE_RANGE = 16

export function scrollEdge(scrollTop: number): number {
  return clamp01(scrollTop / EDGE_RANGE)
}

export interface TabBarScroll {
  minimized: boolean
  anchor: number
}

const MINIMIZE_DISTANCE = 12

export function tabBarScroll(state: TabBarScroll, top: number): TabBarScroll {
  if (top <= 0) {
    return { minimized: false, anchor: 0 }
  }

  if (state.minimized ? top > state.anchor : top < state.anchor) {
    return { minimized: state.minimized, anchor: top }
  }

  if (Math.abs(top - state.anchor) < MINIMIZE_DISTANCE) {
    return state
  }

  return { minimized: top > state.anchor, anchor: top }
}

export function edgeOffset(raw: number, width: number): number {
  if (raw < 0) {
    return rubberband(raw, width)
  }

  return raw > width ? width + rubberband(raw - width, width) : raw
}

export const swipeProgress = (offset: number, width: number): number => (width > 0 ? clamp01(offset / width) : 0)

export interface BarMetrics {
  bar: number
  leading: number
  label: number
  short: number
  trailing: number
}

export interface BarLayout {
  label: 'full' | 'short'
  labelMax: number
  side: number
}

const TITLE_MIN = 96
const LABEL_MIN = 56

export function layoutBar({ bar, leading, label, short, trailing }: BarMetrics): BarLayout {
  if (label <= 0) {
    return { label: 'full', labelMax: 0, side: Math.max(leading, trailing) }
  }

  const room = Math.max(0, (bar - TITLE_MIN) / 2 - leading)

  if (room >= label) {
    return { label: 'full', labelMax: label, side: Math.max(leading + label, trailing) }
  }

  if (room >= LABEL_MIN) {
    return { label: 'full', labelMax: room, side: Math.max(leading + room, trailing) }
  }

  return { label: 'short', labelMax: short, side: Math.max(leading + short, trailing) }
}
