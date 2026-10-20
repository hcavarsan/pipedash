import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

import type { Pipeline } from '../types'

interface PinnedStoreState {
  pinnedKeys: string[]
}

interface PinnedStoreActions {
  togglePin: (pinKey: string) => void
}

type PinnedStore = PinnedStoreState & PinnedStoreActions

const initialState: PinnedStoreState = {
  pinnedKeys: [],
}

export const pinKey = (pipeline: Pipeline, providerName: string) =>
  `${providerName}:${pipeline.id.replace(`__${pipeline.provider_id}__`, '__')}`

export const usePinnedStore = create<PinnedStore>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        togglePin: (key) =>
          set((state) => ({
            pinnedKeys: state.pinnedKeys.includes(key)
              ? state.pinnedKeys.filter((pinned) => pinned !== key)
              : [...state.pinnedKeys, key],
          })),
      }),
      {
        name: 'pipedash-pinned-pipelines',
        partialize: (state) => ({
          pinnedKeys: state.pinnedKeys,
        }),
      }
    ),
    { name: 'PinnedStore' }
  )
)
