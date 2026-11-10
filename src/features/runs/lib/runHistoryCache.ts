import type { QueryClient } from '@tanstack/react-query'

import { queryKeys } from '../../../lib/queryKeys'

export function invalidateRunHistory(
  queryClient: QueryClient,
  pipelineId: string,
  runNumber: number
): Promise<unknown> {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: queryKeys.runs.detail(pipelineId, runNumber) }),
    queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) }),
  ])
}
