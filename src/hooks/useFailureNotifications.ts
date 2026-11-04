import { matchQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { isTauri } from '../services/isTauri'
import { events } from '../services/service'
import { type CenterNotificationTone, useNotificationCenterStore } from '../stores/notificationCenterStore'
import type { Pipeline, ProviderSummary } from '../types/domain'
import type { EventPayloadMap } from '../types/events'
import { showSystemNotification, systemDeliveryEnabled } from '../utils/systemNotifications'

const alertTitle = (pipeline: Pipeline) =>
  pipeline.status === 'degraded' ? `${pipeline.name} is degraded` : `${pipeline.name} failed`

const isOlderObservation = (pipeline: Pipeline, previous: Pipeline | undefined): boolean =>
  previous !== undefined && Date.parse(pipeline.last_updated) < Date.parse(previous.last_updated)

const notify = (
  pipeline: Pipeline,
  tone: CenterNotificationTone,
  title: string,
  provider: ProviderSummary | undefined,
  system: boolean
) => {
  const body = [pipeline.repository, provider?.name || pipeline.provider_type, pipeline.resource_state?.message]
    .filter(Boolean)
    .join(' · ')
  const key = `pipeline:${pipeline.id}`

  useNotificationCenterStore.getState().push({
    key,
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
    showSystemNotification(title, body, { key, pipelineId: pipeline.id, providerId: pipeline.provider_id })
  }
}

const pipelineListsFilter = { queryKey: [...queryKeys.pipelines.all, 'list'] }

export function useFailureNotifications(): void {
  const queryClient = useQueryClient()

  useEffect(() => {
    const native = isTauri()
    const lastPipelines = new Map<string, Pipeline>()
    const notifiedOverdueRuns = new Map<string, string>()
    const failedPipelines = new Set<string>()
    let active = true
    let unlisten: (() => void) | undefined

    if (native) {
      events
        .listen<EventPayloadMap['pipelines-failed']>('pipelines-failed', ({ pipelines }) => {
          if (!active) {
            return
          }
          const enabled = systemDeliveryEnabled()
          const providers = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list())

          for (const pipeline of pipelines) {
            if (isOlderObservation(pipeline, lastPipelines.get(pipeline.id))) {
              continue
            }
            lastPipelines.set(pipeline.id, pipeline)
            if (pipeline.status === 'failed') {
              failedPipelines.add(pipeline.id)
            }
            const provider = providers?.find((item) => item.id === pipeline.provider_id)
            notify(pipeline, 'danger', alertTitle(pipeline), provider, enabled)
          }
        })
        .then((fn) => {
          if (active) {
            unlisten = fn
          } else {
            fn()
          }
        })
        .catch((error: unknown) =>
          logger.warn('useFailureNotifications', "Couldn't listen for pipeline failures", error)
        )
    }

    const unsubscribe = queryClient.getQueryCache().subscribe((event) => {
      if (event.type !== 'updated' || event.action.type !== 'success') {
        return
      }

      if (!matchQuery(pipelineListsFilter, event.query)) {
        return
      }

      const pipelines = queryClient.getQueryData<Pipeline[]>(event.query.queryKey)

      if (!pipelines) {
        return
      }

      const enabled = systemDeliveryEnabled()
      const providers = queryClient.getQueryData<ProviderSummary[]>(queryKeys.providers.list())

      for (const pipeline of pipelines) {
        const previous = lastPipelines.get(pipeline.id)
        if (isOlderObservation(pipeline, previous)) {
          continue
        }
        const overdueRun = pipeline.overdue_run
        const newOverdueRun =
          previous !== undefined && !!overdueRun && notifiedOverdueRuns.get(pipeline.id) !== overdueRun
        const failed =
          !native &&
          previous !== undefined &&
          (pipeline.status === 'failed' || pipeline.status === 'degraded') &&
          (previous.status !== pipeline.status || previous.last_run !== pipeline.last_run)
        const recovered = pipeline.status === 'success' && failedPipelines.has(pipeline.id)

        if (pipeline.status === 'failed') {
          failedPipelines.add(pipeline.id)
        } else if (pipeline.status === 'success') {
          failedPipelines.delete(pipeline.id)
        }

        lastPipelines.set(pipeline.id, pipeline)
        if (overdueRun) {
          notifiedOverdueRuns.set(pipeline.id, overdueRun)
        }

        if (!failed && !newOverdueRun && !recovered) {
          continue
        }

        const provider = providers?.find((p) => p.id === pipeline.provider_id)
        if (failed) {
          notify(pipeline, 'danger', alertTitle(pipeline), provider, enabled)
        }
        if (recovered) {
          notify(pipeline, 'success', `${pipeline.name} passed again`, provider, enabled)
        }
        if (newOverdueRun) {
          notify(pipeline, 'warning', `${pipeline.name} is running longer than usual`, provider, enabled)
        }
      }
    })

    return () => {
      active = false
      unlisten?.()
      unsubscribe()
    }
  }, [queryClient])
}
