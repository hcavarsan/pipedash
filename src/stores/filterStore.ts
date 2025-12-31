import { create } from 'zustand'
import { devtools, persist } from 'zustand/middleware'

interface FilterStoreState {
  selectedProviderId: number | undefined
  selectedProviderName: string | undefined
  statusFilter: string | null
  organizationFilter: string | null
  searchQuery: string
}

interface FilterStoreActions {
  setSelectedProviderId: (id: number | undefined, name?: string | undefined) => void
  setStatusFilter: (status: string | null) => void
  setOrganizationFilter: (org: string | null) => void
  setSearchQuery: (query: string) => void
  clearFilters: () => void
}

type FilterStore = FilterStoreState & FilterStoreActions

const initialState: FilterStoreState = {
  selectedProviderId: undefined,
  selectedProviderName: undefined,
  statusFilter: null,
  organizationFilter: null,
  searchQuery: '',
}

export const useFilterStore = create<FilterStore>()(
  devtools(
    persist(
      (set) => ({
        ...initialState,

        setSelectedProviderId: (id, name) => set({ selectedProviderId: id, selectedProviderName: name }),
        setStatusFilter: (status) => set({ statusFilter: status }),
        setOrganizationFilter: (org) => set({ organizationFilter: org }),
        setSearchQuery: (query) => set({ searchQuery: query }),

        clearFilters: () =>
          set({
            statusFilter: null,
            organizationFilter: null,
            searchQuery: '',
          }),
      }),
      {
        name: 'pipedash-filters',
        partialize: (state) => ({
          selectedProviderId: state.selectedProviderId,
        }),
        onRehydrateStorage: () => (state) => {
          console.debug('[FilterStore] Rehydrated:', state?.selectedProviderId)
        },
      }
    ),
    { name: 'FilterStore' }
  )
)
