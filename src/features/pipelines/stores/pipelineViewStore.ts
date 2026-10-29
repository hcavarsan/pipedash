import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

export interface ViewEditorState {
  open: boolean
  viewId: string | null
  initialMembers: string[]
  parent: string | null
}

interface PipelineViewStoreState {
  editor: ViewEditorState
  addToViewIds: string[] | null
  manageOpen: boolean
}

interface PipelineViewStoreActions {
  openEditor: (viewId: string | null, initialMembers?: string[], parent?: string | null) => void
  closeEditor: () => void
  openAddToView: (ids: string[]) => void
  closeAddToView: () => void
  openManage: () => void
  closeManage: () => void
}

type PipelineViewStore = PipelineViewStoreState & PipelineViewStoreActions

const initialState: PipelineViewStoreState = {
  editor: { open: false, viewId: null, initialMembers: [], parent: null },
  addToViewIds: null,
  manageOpen: false,
}

export const usePipelineViewStore = create<PipelineViewStore>()(
  devtools(
    (set) => ({
      ...initialState,

      openEditor: (viewId, initialMembers = [], parent = null) =>
        set({ editor: { open: true, viewId, initialMembers, parent } }),

      closeEditor: () => set({ editor: { open: false, viewId: null, initialMembers: [], parent: null } }),

      openAddToView: (ids) => set({ addToViewIds: ids }),

      closeAddToView: () => set({ addToViewIds: null }),

      openManage: () => set({ manageOpen: true }),

      closeManage: () => set({ manageOpen: false }),
    }),
    { name: 'PipelineViewStore' }
  )
)
