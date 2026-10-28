import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

interface HiddenStoreState {
  hiddenKeys: string[]
}

interface HiddenStoreActions {
  toggleHidden: (pinKey: string) => void
}

type HiddenStore = HiddenStoreState & HiddenStoreActions

const initialState: HiddenStoreState = {
  hiddenKeys: [],
}

export const useHiddenStore = create<HiddenStore>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        toggleHidden: (key) =>
          set((state) => ({
            hiddenKeys: state.hiddenKeys.includes(key)
              ? state.hiddenKeys.filter((hidden) => hidden !== key)
              : [...state.hiddenKeys, key],
          })),
      }),
      {
        name: 'pipedash-hidden-pipelines',
        partialize: (state) => ({
          hiddenKeys: state.hiddenKeys,
        }),
      }
    ),
    { name: 'HiddenStore' }
  )
)
