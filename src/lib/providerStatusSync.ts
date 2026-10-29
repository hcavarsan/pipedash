import type { QueryClient } from '@tanstack/react-query'

import type { ProviderSummary } from '../types/domain'
import { patchProviderList } from '../utils/patchProviderList'

import { queryKeys } from './queryKeys'

export async function applyProviderStatus(
  queryClient: QueryClient,
  summary: ProviderSummary,
  isActive: () => boolean
): Promise<void> {
  await queryClient.cancelQueries({ queryKey: queryKeys.providers.list(), exact: true })

  if (!isActive()) {
    return
  }

  queryClient.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), (list) => patchProviderList(list, summary))
}
