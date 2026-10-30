import { useEffect } from 'react'
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

function rememberTitle(titles: Record<string, string>, path: string, title: string): Record<string, string> {
  if (titles[path] === title) {
    return titles
  }

  const next = { ...titles }

  delete next[path]
  next[path] = title

  return capped(next)
}

interface MobileNavState {
  title: string | null
  setTitle: (title: string | null) => void
  tabs: TabRoutes
  scrolls: Record<string, number>
  rememberRoute: (tab: MobileTab, route: TabRoute) => void
  rememberScroll: (key: string, top: number) => void
  pageTitles: Record<string, string>
  rememberTitle: (path: string, title: string) => void
  transition: BarTransition | null
  setTransition: (transition: BarTransition | null) => void
  navSlot: HTMLElement | null
  setNavSlot: (slot: HTMLElement | null) => void
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
  pageTitles: {},
  rememberTitle: (path, title) => {
    const pageTitles = rememberTitle(get().pageTitles, path, title)

    if (pageTitles !== get().pageTitles) {
      set({ pageTitles })
    }
  },
  transition: null,
  setTransition: (transition) => set({ transition }),
  navSlot: null,
  setNavSlot: (navSlot) => set({ navSlot }),
}))

export function useMobileTitle(title: string | null | undefined): void {
  const setTitle = useMobileNavStore((s) => s.setTitle)

  useEffect(() => {
    setTitle(title ?? null)

    return () => setTitle(null)
  }, [title, setTitle])
}
