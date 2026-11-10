import type { QueryClient } from '@tanstack/react-query'
import { useCallback, useRef } from 'react'

import { invalidateViews } from '../features/pipelines/queries/usePipelineViewQueries'
import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { invalidateProviderPipelines } from '../queries/usePipelinesQueries'

const BATCH_WINDOW_MS = 300

interface BatchedInvalidation {
  providerIds: Set<number>
  runHistoryPipelineIds: Set<string>
  invalidateAll: boolean
  views: boolean
}

interface EventBatch {
  scheduleBatchedInvalidation: (providerId?: number) => void
  scheduleRunHistoryInvalidation: (pipelineIds: readonly string[]) => void
  scheduleViewsInvalidation: () => void
  cancel: () => void
}

const emptyBatch = (): BatchedInvalidation => ({
  providerIds: new Set(),
  runHistoryPipelineIds: new Set(),
  invalidateAll: false,
  views: false,
})

export function useEventBatch(queryClient: QueryClient): EventBatch {
  const pending = useRef<BatchedInvalidation>(emptyBatch())
  const timeout = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const processBatch = useCallback(() => {
    const batch = pending.current

    pending.current = emptyBatch()
    timeout.current = undefined

    if (batch.invalidateAll) {
      logger.debug('EventSync', 'Processing batched invalidation: ALL providers')
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
    } else if (batch.providerIds.size > 0) {
      logger.debug('EventSync', 'Processing batched invalidation', {
        providerIds: Array.from(batch.providerIds),
      })

      for (const providerId of batch.providerIds) {
        invalidateProviderPipelines(queryClient, providerId)
      }
    }

    for (const pipelineId of batch.runHistoryPipelineIds) {
      void queryClient.invalidateQueries({ queryKey: queryKeys.runs.list(pipelineId) })
    }

    if (batch.views) {
      void invalidateViews(queryClient)
    }
  }, [queryClient])

  const scheduleBatch = useCallback(() => {
    clearTimeout(timeout.current)
    timeout.current = setTimeout(processBatch, BATCH_WINDOW_MS)
  }, [processBatch])

  const scheduleBatchedInvalidation = useCallback(
    (providerId?: number) => {
      if (providerId === undefined) {
        pending.current.invalidateAll = true
      } else {
        pending.current.providerIds.add(providerId)
      }

      scheduleBatch()
    },
    [scheduleBatch]
  )

  const scheduleRunHistoryInvalidation = useCallback(
    (pipelineIds: readonly string[]) => {
      for (const pipelineId of pipelineIds) {
        pending.current.runHistoryPipelineIds.add(pipelineId)
      }

      scheduleBatch()
    },
    [scheduleBatch]
  )

  const scheduleViewsInvalidation = useCallback(() => {
    pending.current.views = true
    scheduleBatch()
  }, [scheduleBatch])

  const cancel = useCallback(() => {
    clearTimeout(timeout.current)
    timeout.current = undefined
    pending.current = emptyBatch()
  }, [])

  return { scheduleBatchedInvalidation, scheduleRunHistoryInvalidation, scheduleViewsInvalidation, cancel }
}
