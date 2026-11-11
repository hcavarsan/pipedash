import { createContext, useContext, useLayoutEffect } from 'react'
import { create } from 'zustand'

import type { MobileTab } from '../components/layout/mobileChrome'

export interface TabRoute {
  path: string
  from: string | null
  depth?: number
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

const SCROLL_LIMIT = 40

export function rememberRoute(tabs: TabRoutes, tab: MobileTab, route: TabRoute): TabRoutes {
  const current = tabs[tab]

  return current?.path === route.path && current.from === route.from && current.depth === route.depth
    ? tabs
    : { ...tabs, [tab]: route }
}

export function resumeRoute(tabs: TabRoutes, tab: MobileTab, root: string): TabRoute {
  return tabs[tab] ?? { path: root, from: null }
}

const capped = <T>(entries: Record<string, T>): Record<string, T> => {
  const keys = Object.keys(entries)

  return keys.length > SCROLL_LIMIT ? Object.fromEntries(Object.entries(entries).slice(-SCROLL_LIMIT)) : entries
}

export function rememberScroll(scrolls: Record<string, number>, key: string, top: number): Record<string, number> {
  const next = { ...scrolls }

  delete next[key]

  if (top > 0) {
    next[key] = top
  }

  return capped(next)
}

function rememberTitle(titles: Record<string, string>, key: string, title: string | null): Record<string, string> {
  if ((titles[key] ?? null) === title) {
    return titles
  }

  const next = { ...titles }

  delete next[key]

  if (title !== null) {
    next[key] = title
  }

  return capped(next)
}

interface MobileNavState {
  titles: Record<string, string>
  setTitle: (key: string, title: string | null) => void
  tabs: TabRoutes
  scrolls: Record<string, number>
  rememberRoute: (tab: MobileTab, route: TabRoute) => void
  rememberScroll: (key: string, top: number) => void
  transition: BarTransition | null
  setTransition: (transition: BarTransition | null) => void
  navSlot: HTMLElement | null
  setNavSlot: (slot: HTMLElement | null) => void
}

export const useMobileNavStore = create<MobileNavState>()((set, get) => ({
  titles: {},
  setTitle: (key, title) => {
    const titles = rememberTitle(get().titles, key, title)

    if (titles !== get().titles) {
      set({ titles })
    }
  },
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
  navSlot: null,
  setNavSlot: (navSlot) => set({ navSlot }),
}))

export const RouteKeyContext = createContext<string | null>(null)

export function useMobileTitle(title: string | null | undefined): void {
  const key = useContext(RouteKeyContext)
  const setTitle = useMobileNavStore((s) => s.setTitle)

  useLayoutEffect(() => {
    if (key) {
      setTitle(key, title ?? null)
    }
  }, [key, title, setTitle])
}
