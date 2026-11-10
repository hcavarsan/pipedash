import type { QueryClient } from '@tanstack/react-query'

import { queryKeys } from '../../../lib/queryKeys'

export function invalidateRunHistory(queryClient: QueryClient, pipelineId: string, runNumber: number): void {
  void queryClient.invalidateQueries({ queryKey: queryKeys.runs.detail(pipelineId, runNumber) })
  void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) })
}
