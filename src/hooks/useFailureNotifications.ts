import { matchQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'

import { queryKeys } from '../lib/queryKeys'
import { type CenterNotificationTone, useNotificationCenterStore } from '../stores/notificationCenterStore'
import { useNotificationStore } from '../stores/notificationStore'
import type { Pipeline, PipelineStatus, ProviderSummary } from '../types'
import { isOutageLevel, providerAttention } from '../utils/providerAttention'
import { showSystemNotification } from '../utils/systemNotifications'

const notify = (pipeline: Pipeline, tone: CenterNotificationTone, title: string, body: string, system: boolean) => {
  const id = useNotificationCenterStore.getState().push({
    key: `pipeline:${pipeline.id}`,
    at: Date.now(),
    tone,
    level: null,
    title,
    body,
    providerIds: [],
    pipelineId: pipeline.id,
    link: null,
  })

  if (system) {
    showSystemNotification(title, body, id)
  }
}

const pipelineListsFilter = { queryKey: [...queryKeys.pipelines.all, 'list'] }

export function useFailureNotifications() {
  const queryClient = useQueryClient()

  useEffect(() => {
    const lastStatuses = new Map<string, PipelineStatus>()
    const notifiedOverdueRuns = new Map<string, string>()

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
        const overdueRun = pipeline.overdue_run
        const newOverdueRun =
          previous !== undefined && !!overdueRun && notifiedOverdueRuns.get(pipeline.id) !== overdueRun

        lastStatuses.set(pipeline.id, pipeline.status)
        if (overdueRun) {
          notifiedOverdueRuns.set(pipeline.id, overdueRun)
        }

        const failed = previous !== undefined && previous !== 'failed' && pipeline.status === 'failed'

        if (!failed && !newOverdueRun) {
          continue
        }

        const provider = providers?.find((p) => p.id === pipeline.provider_id)
        const providerName = provider?.name ?? pipeline.provider_type
        const outage = provider && isOutageLevel(provider.health.level) ? providerAttention(provider).headline : null
        const details = [[pipeline.repository, providerName].filter(Boolean).join(' · '), outage]
          .filter(Boolean)
          .join(' · ')

        if (failed) {
          notify(pipeline, 'danger', `${pipeline.name} failed`, details, enabled)
        }

        if (newOverdueRun) {
          notify(pipeline, 'warning', `${pipeline.name} is running longer than usual`, details, enabled)
        }
      }
    })
  }, [queryClient])
}
