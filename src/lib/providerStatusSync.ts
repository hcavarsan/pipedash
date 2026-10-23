import type { QueryClient } from '@tanstack/react-query'

import type { ProviderSummary } from '../types'
import { patchProviderList } from '../utils/patchProviderList'

import { queryKeys } from './queryKeys'

export async function applyProviderStatus(queryClient: QueryClient, summary: ProviderSummary): Promise<void> {
  await queryClient.cancelQueries({ queryKey: queryKeys.providers.list(), exact: true })
  queryClient.setQueryData<ProviderSummary[]>(queryKeys.providers.list(), (list) => patchProviderList(list, summary))
}
