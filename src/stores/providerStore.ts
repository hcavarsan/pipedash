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
}

interface ProviderStoreActions {
  openAddProviderModal: () => void
  closeAddProviderModal: () => void
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

      openEditProviderModal: (provider) =>
        set({
          editProviderModal: { open: true, provider },
        }),

      closeEditProviderModal: () =>
        set({
          editProviderModal: { open: false, provider: null },
        }),
    }),
    { name: 'ProviderStore' }
  )
)
