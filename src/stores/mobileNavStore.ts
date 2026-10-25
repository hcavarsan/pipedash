import { useEffect } from 'react'
import { create } from 'zustand'

import type { MobileTab } from '../components/layout/mobileChrome'

export interface TabRoute {
  path: string
  from: string | null
}

export type TabRoutes = Partial<Record<MobileTab, TabRoute>>

export interface BarSnapshot {
  title: string
  backLabel: string | null
  collapse: number
}

export interface BarTransition {
  kind: 'push' | 'pop' | 'swipe'
  other: BarSnapshot
}

export interface HeldLocation {
  path: string
  from: string | null
}

const SCROLL_LIMIT = 40

export function rememberRoute(tabs: TabRoutes, tab: MobileTab, route: TabRoute): TabRoutes {
  const current = tabs[tab]

  return current?.path === route.path && current.from === route.from ? tabs : { ...tabs, [tab]: route }
}

export function resumeRoute(tabs: TabRoutes, tab: MobileTab, root: string): TabRoute {
  return tabs[tab] ?? { path: root, from: null }
}

export function rememberScroll(scrolls: Record<string, number>, key: string, top: number): Record<string, number> {
  const next = { ...scrolls }

  delete next[key]

  if (top > 0) {
    next[key] = top
  }

  const keys = Object.keys(next)

  return keys.length > SCROLL_LIMIT ? Object.fromEntries(keys.slice(-SCROLL_LIMIT).map((k) => [k, next[k]])) : next
}

interface MobileNavState {
  title: string | null
  setTitle: (title: string | null) => void
  tabs: TabRoutes
  scrolls: Record<string, number>
  rememberRoute: (tab: MobileTab, route: TabRoute) => void
  rememberScroll: (key: string, top: number) => void
  transition: BarTransition | null
  setTransition: (transition: BarTransition | null) => void
  held: HeldLocation | null
  hold: (held: HeldLocation | null) => void
  takeHeld: (path: string) => string | null
}

export const useMobileNavStore = create<MobileNavState>()((set, get) => ({
  title: null,
  setTitle: (title) => set({ title }),
  tabs: {},
  scrolls: {},
  rememberRoute: (tab, route) => {
    const tabs = rememberRoute(get().tabs, tab, route)

    if (tabs !== get().tabs) {
      set({ tabs })
    }
  },
  rememberScroll: (key, top) => set({ scrolls: rememberScroll(get().scrolls, key, top) }),
  transition: null,
  setTransition: (transition) => set({ transition }),
  held: null,
  hold: (held) => set({ held }),
  takeHeld: (path) => {
    const { held } = get()

    set({ held: null })

    return held?.path === path ? held.from : null
  },
}))

export function useMobileTitle(title: string | null | undefined): void {
  const setTitle = useMobileNavStore((s) => s.setTitle)

  useEffect(() => {
    setTitle(title ?? null)

    return () => setTitle(null)
  }, [title, setTitle])
}
