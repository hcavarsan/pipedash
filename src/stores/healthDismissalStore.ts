import { create } from 'zustand'

interface HealthDismissalState {
  dismissed: string[]
  dismiss: (episodeKey: string) => void
  prune: (activeKeys: string[]) => void
}

export const useHealthDismissalStore = create<HealthDismissalState>()((set) => ({
  dismissed: [],

  dismiss: (episodeKey) =>
    set((state) => (state.dismissed.includes(episodeKey) ? state : { dismissed: [...state.dismissed, episodeKey] })),

  prune: (activeKeys) =>
    set((state) => {
      const kept = state.dismissed.filter((key) => activeKeys.includes(key))

      return kept.length === state.dismissed.length ? state : { dismissed: kept }
    }),
}))
