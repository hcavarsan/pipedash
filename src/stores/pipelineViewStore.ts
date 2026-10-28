import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

import type { ViewPipeline } from '../types'

interface ViewEditorState {
  open: boolean
  viewId: string | null
  initialMembers: string[]
  parent: string | null
}

interface PipelineViewStoreState {
  editor: ViewEditorState
  addToView: ViewPipeline[] | null
  manageOpen: boolean
}

interface PipelineViewStoreActions {
  openEditor: (viewId: string | null, initialMembers?: string[], parent?: string | null) => void
  closeEditor: () => void
  openAddToView: (pipelines: ViewPipeline[]) => void
  closeAddToView: () => void
  openManage: () => void
  closeManage: () => void
}

type PipelineViewStore = PipelineViewStoreState & PipelineViewStoreActions

const initialState: PipelineViewStoreState = {
  editor: { open: false, viewId: null, initialMembers: [], parent: null },
  addToView: null,
  manageOpen: false,
}

export const usePipelineViewStore = create<PipelineViewStore>()(
  devtools(
    (set) => ({
      ...initialState,

      openEditor: (viewId, initialMembers = [], parent = null) =>
        set({ editor: { open: true, viewId, initialMembers, parent } }),

      closeEditor: () => set({ editor: { open: false, viewId: null, initialMembers: [], parent: null } }),

      openAddToView: (pipelines) => set({ addToView: pipelines }),

      closeAddToView: () => set({ addToView: null }),

      openManage: () => set({ manageOpen: true }),

      closeManage: () => set({ manageOpen: false }),
    }),
    { name: 'PipelineViewStore' }
  )
)
