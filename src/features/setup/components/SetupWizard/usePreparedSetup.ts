import { keepPreviousData, useQuery } from '@tanstack/react-query'

import { DEBOUNCE_DELAYS } from '../../../../constants/intervals'
import { useDebounce } from '../../../../hooks/useDebounce'
import { service } from '../../../../services/service'
import type { SetupDraftResult } from '../../../../types/setup'

import { toSetupDraft } from './setupDraft'
import type { SetupState } from './types'

interface PreparedSetup {
  prepared: SetupDraftResult | undefined
  current: boolean
  error: Error | null
  isFetching: boolean
  refetch: () => void
}

export function usePreparedSetup(state: SetupState, firstRun: boolean): PreparedSetup {
  const settled = useDebounce(state, DEBOUNCE_DELAYS.FILTER)

  const query = useQuery({
    queryKey: ['setupDraft', firstRun, settled],
    queryFn: () => service.prepareSetupDraft(toSetupDraft(settled), firstRun),
    placeholderData: keepPreviousData,
    gcTime: 0,
    staleTime: 0,
    retry: false,
  })

  return {
    prepared: query.data,
    current: !query.isPlaceholderData && settled === state,
    error: query.error,
    isFetching: query.isFetching,
    refetch: () => {
      void query.refetch()
    },
  }
}
