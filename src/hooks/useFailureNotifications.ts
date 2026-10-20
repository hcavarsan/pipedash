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

        lastStatuses.set(pipeline.id, pipeline.status)

        if (enabled && previous !== undefined && previous !== 'failed' && pipeline.status === 'failed') {
          const providerName = providers?.find((p) => p.id === pipeline.provider_id)?.name ?? pipeline.provider_type

          showSystemNotification(
            `${pipeline.name} failed`,
            [pipeline.repository, providerName].filter(Boolean).join(' · ')
          )
        }
      }
    })
  }, [queryClient])
}
