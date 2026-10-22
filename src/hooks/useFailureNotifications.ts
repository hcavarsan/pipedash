import { matchQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'

import { queryKeys } from '../lib/queryKeys'
import { useNotificationStore } from '../stores/notificationStore'
import type { Pipeline, PipelineStatus, ProviderSummary } from '../types'
import { showSystemNotification } from '../utils/systemNotifications'

const pipelineListsFilter = { queryKey: [...queryKeys.pipelines.all, 'list'] }

export function useFailureNotifications() {
  const queryClient = useQueryClient()

  useEffect(() => {
    const lastStatuses = new Map<string, PipelineStatus>()
    const lastOverdue = new Map<string, boolean>()

    return queryClient.getQueryCache().subscribe((event) => {
      if (event.type !== 'updated' || event.action.type !== 'success') {
        return
      }

      if (!matchQuery(pipelineListsFilter, event.query)) {
        return
      }

      const pipelines = event.query.state.data as Pipeline[] | undefined

      if (!pipelines) {
        return
      }

      const enabled = useNotificationStore.getState().failureNotificationsEnabled
      const providers = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list())

      for (const pipeline of pipelines) {
        const previous = lastStatuses.get(pipeline.id)
        const wasOverdue = lastOverdue.get(pipeline.id)
        const isOverdue = pipeline.overdue === true

        lastStatuses.set(pipeline.id, pipeline.status)
        lastOverdue.set(pipeline.id, isOverdue)

        if (!enabled) {
          continue
        }

        const providerName = providers?.find((p) => p.id === pipeline.provider_id)?.name ?? pipeline.provider_type
        const details = [pipeline.repository, providerName].filter(Boolean).join(' · ')

        if (previous !== undefined && previous !== 'failed' && pipeline.status === 'failed') {
          showSystemNotification(`${pipeline.name} failed`, details)
        }

        if (wasOverdue === false && isOverdue) {
          showSystemNotification(`${pipeline.name} is running longer than usual`, details)
        }
      }
    })
  }, [queryClient])
}
