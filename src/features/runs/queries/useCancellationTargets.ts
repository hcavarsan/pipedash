import { type UseQueryResult, useQuery } from '@tanstack/react-query'

import { queryKeys } from '../../../lib/queryKeys'
import { service } from '../../../services/service'
import type { CancellationTargets } from '../../../types/run_action_batch'

export function useCancellationTargets(
  pipelineIds: string[],
  enabled: boolean
): UseQueryResult<CancellationTargets, Error> {
  return useQuery({
    queryKey: queryKeys.runs.unfinished(pipelineIds),
    queryFn: () => service.getCancellationTargets(pipelineIds),
    staleTime: 0,
    gcTime: 0,
    enabled: enabled && pipelineIds.length > 0,
  })
}
