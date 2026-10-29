import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

interface ProviderStoreState {
  addProviderModal: { open: boolean }
  editProviderId: number | null
}

interface ProviderStoreActions {
  openAddProviderModal: () => void
  closeAddProviderModal: () => void
  requestEditProvider: (id: number) => void
  closeEditProviderModal: () => void
}

type ProviderStore = ProviderStoreState & ProviderStoreActions

export const useProviderStore = create<ProviderStore>()(
  devtools(
    (set) => ({
      addProviderModal: { open: false },
      editProviderId: null,

      openAddProviderModal: () => set({ addProviderModal: { open: true } }),

      closeAddProviderModal: () => set({ addProviderModal: { open: false } }),

      requestEditProvider: (id) => set({ editProviderId: id }),

      closeEditProviderModal: () => set({ editProviderId: null }),
    }),
    { name: 'ProviderStore' }
  )
)
