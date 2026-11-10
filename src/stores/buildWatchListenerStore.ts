import { create } from 'zustand'

interface BuildWatchListenerState {
  error: Error | null
  attempt: number
  retry: () => void
}

export const useBuildWatchListenerStore = create<BuildWatchListenerState>()((set) => ({
  error: null,
  attempt: 0,
  retry: () => set((state) => ({ error: null, attempt: state.attempt + 1 })),
}))
