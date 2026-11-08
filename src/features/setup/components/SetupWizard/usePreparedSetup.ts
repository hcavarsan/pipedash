import { keepPreviousData, useQuery } from '@tanstack/react-query'

import { DEBOUNCE_DELAYS } from '../../../../constants/intervals'
import { useDebounce } from '../../../../hooks/useDebounce'
import { service } from '../../../../services/service'
import type { SetupDraftResult } from '../../../../types/setup'

import { toSetupDraft } from './setupDraft'
import type { SetupState } from './types'

export function usePreparedSetup(state: SetupState): SetupDraftResult | undefined {
  const settled = useDebounce(state, DEBOUNCE_DELAYS.FILTER)

  return useQuery({
    queryKey: ['setupDraft', settled],
    queryFn: () => service.prepareSetupDraft(toSetupDraft(settled)),
    placeholderData: keepPreviousData,
    gcTime: 0,
    staleTime: 0,
    retry: false,
  }).data
}
