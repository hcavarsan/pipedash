import { keepPreviousData, useQuery } from '@tanstack/react-query'

import { DEBOUNCE_DELAYS } from '../../../constants/intervals'
import { useDebounce } from '../../../hooks/useDebounce'
import { service } from '../../../services/service'
import type { ProviderDraft, ProviderDraftResult } from '../../../types/provider_draft'

const SIGN_IN_POLL_MS = 2000

export function useProviderDraftCheck(draft: ProviderDraft | null): ProviderDraftResult | undefined {
  const debounced = useDebounce(draft, DEBOUNCE_DELAYS.FILTER)

  return useQuery({
    queryKey: ['providerDraft', debounced],
    queryFn: () => (debounced ? service.validateProviderDraft(debounced) : Promise.reject(new Error('No draft'))),
    enabled: debounced !== null,
    placeholderData: keepPreviousData,
    gcTime: 0,
    staleTime: 0,
    retry: false,
    refetchInterval: (query) =>
      debounced?.auth_mode === 'sign_in' && query.state.data?.credentials_complete === false ? SIGN_IN_POLL_MS : false,
  }).data
}
