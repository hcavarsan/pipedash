import { useEffect, useRef } from 'react'
import { create } from 'zustand'

type RefreshHandler = () => Promise<unknown>

interface PullRefreshState {
  handler: RefreshHandler | null
  setHandler: (handler: RefreshHandler | null) => void
}

export const usePullRefreshStore = create<PullRefreshState>()((set) => ({
  handler: null,
  setHandler: (handler) => set({ handler }),
}))

export function usePullToRefresh(refresh: RefreshHandler | null): void {
  const setHandler = usePullRefreshStore((s) => s.setHandler)
  const latest = useRef(refresh)
  const enabled = refresh !== null

  latest.current = refresh

  useEffect(() => {
    if (!enabled) {
      return
    }

    const handler: RefreshHandler = () => latest.current?.() ?? Promise.resolve()

    setHandler(handler)

    return () => {
      if (usePullRefreshStore.getState().handler === handler) {
        setHandler(null)
      }
    }
  }, [enabled, setHandler])
}
