import { create } from 'zustand'
import { devtools } from 'zustand/middleware'

import type { SignInStart } from '../types/domain'

export interface ProviderSignInSession {
  requestId: number
  providerId: number
  flow: SignInStart | null
  phase: 'starting' | 'waiting' | 'saving' | 'save_failed' | 'failed'
  error: string | null
}

interface ProviderStoreState {
  addProviderModal: { open: boolean }
  editProviderId: number | null
  providerSignIn: ProviderSignInSession | null
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
      providerSignIn: null,

      openAddProviderModal: () => set({ addProviderModal: { open: true } }),

      closeAddProviderModal: () => set({ addProviderModal: { open: false } }),

      requestEditProvider: (id) => set({ editProviderId: id }),

      closeEditProviderModal: () => set({ editProviderId: null }),
    }),
    { name: 'ProviderStore' }
  )
)
