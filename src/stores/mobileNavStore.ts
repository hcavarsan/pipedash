import { useEffect } from 'react'
import { create } from 'zustand'

interface MobileNavState {
  title: string | null
  setTitle: (title: string | null) => void
}

export const useMobileNavStore = create<MobileNavState>()((set) => ({
  title: null,
  setTitle: (title) => set({ title }),
}))

export function useMobileTitle(title: string | null | undefined): void {
  const setTitle = useMobileNavStore((s) => s.setTitle)

  useEffect(() => {
    setTitle(title ?? null)

    return () => setTitle(null)
  }, [title, setTitle])
}
