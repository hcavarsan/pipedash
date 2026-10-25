import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

import type { ProviderConfig } from '../types'

interface AddProviderModalState {
  open: boolean
}

interface EditProviderModalState {
  open: boolean
  provider: (ProviderConfig & { id: number }) | null
}

interface ProviderStoreState {
  addProviderModal: AddProviderModalState
  editProviderModal: EditProviderModalState
  editRequestId: number | null
}

interface ProviderStoreActions {
  openAddProviderModal: () => void
  closeAddProviderModal: () => void
  requestEditProvider: (id: number) => void
  openEditProviderModal: (provider: ProviderConfig & { id: number }) => void
  closeEditProviderModal: () => void
}

type ProviderStore = ProviderStoreState & ProviderStoreActions

const initialState: ProviderStoreState = {
  addProviderModal: {
    open: false,
  },
  editProviderModal: {
    open: false,
    provider: null,
  },
  editRequestId: null,
}

export const useProviderStore = create<ProviderStore>()(
  devtools(
    (set) => ({
      ...initialState,

      openAddProviderModal: () =>
        set({
          addProviderModal: { open: true },
        }),

      closeAddProviderModal: () =>
        set({
          addProviderModal: { open: false },
        }),

      requestEditProvider: (id) => set({ editRequestId: id }),

      openEditProviderModal: (provider) =>
        set({
          editProviderModal: { open: true, provider },
          editRequestId: null,
        }),

      closeEditProviderModal: () =>
        set({
          editProviderModal: { open: false, provider: null },
          editRequestId: null,
        }),
    }),
    { name: 'ProviderStore' }
  )
)
