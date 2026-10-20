import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

interface PinnedStoreState {
  pinnedIds: string[]
}

interface PinnedStoreActions {
  togglePin: (pipelineId: string) => void
}

type PinnedStore = PinnedStoreState & PinnedStoreActions

const initialState: PinnedStoreState = {
  pinnedIds: [],
}

export const usePinnedStore = create<PinnedStore>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        togglePin: (pipelineId) =>
          set((state) => ({
            pinnedIds: state.pinnedIds.includes(pipelineId)
              ? state.pinnedIds.filter((id) => id !== pipelineId)
              : [...state.pinnedIds, pipelineId],
          })),
      }),
      {
        name: 'pipedash-pinned-pipelines',
        partialize: (state) => ({
          pinnedIds: state.pinnedIds,
        }),
      }
    ),
    { name: 'PinnedStore' }
  )
)
