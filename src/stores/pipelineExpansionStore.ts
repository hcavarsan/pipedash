import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

interface PipelineExpansionState {
  expanded: Record<string, Record<string, boolean>>
}

interface PipelineExpansionActions {
  setExpanded: (view: string, changes: Record<string, boolean>) => void
}

export const usePipelineExpansionStore = create<PipelineExpansionState & PipelineExpansionActions>()(
  devtools(
    persist(
      (set) => ({
        expanded: {},

        setExpanded: (view, changes) =>
          set((state) => ({ expanded: { ...state.expanded, [view]: { ...state.expanded[view], ...changes } } })),
      }),
      {
        name: 'pipedash-pipeline-expansion',
        partialize: (state) => ({ expanded: state.expanded }),
      }
    ),
    { name: 'PipelineExpansionStore' }
  )
)
