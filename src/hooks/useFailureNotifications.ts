import { matchQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { queryKeys } from '../lib/queryKeys'
import { isTauri } from '../services/isTauri'
import { events } from '../services/service'
import { type CenterNotificationTone, useNotificationCenterStore } from '../stores/notificationCenterStore'
import { shouldNotify, useNotificationStore } from '../stores/notificationStore'
import type { AlertKind, Pipeline, PipelineStatus, ProviderSummary } from '../types/domain'
import type { EventPayloadMap } from '../types/events'
import { showSystemNotification, systemDeliveryEnabled } from '../utils/systemNotifications'

const alertTitle = (pipeline: Pipeline) =>
  pipeline.status === 'degraded' ? `${pipeline.name} is degraded` : `${pipeline.name} failed`

const FINISH_OUTCOMES: Partial<Record<PipelineStatus, string>> = {
  success: 'succeeded',
  cancelled: 'was cancelled',
  skipped: 'was skipped',
}

const attentionKind = (pipeline: Pipeline): AlertKind => (pipeline.status === 'degraded' ? 'degraded' : 'failed')

const notify = (
  pipeline: Pipeline,
  kind: AlertKind,
  tone: CenterNotificationTone,
  title: string,
  provider: ProviderSummary | undefined,
  system: boolean
) => {
  const finish = kind === 'failed' || kind === 'finished' ? { at: new Date().toISOString(), previous: null } : null
  const allowed = shouldNotify(kind, [pipeline.provider_id], pipeline.id, finish)

  if (finish) {
    useNotificationStore.getState().consumeNextFinish(pipeline.id)
  }
  if (!allowed) {
    return
  }

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
    const unlisteners: Array<() => void> = []
    let active = true

    const listen = (event: 'pipelines-failed' | 'pipelines-finished', handle: (pipeline: Pipeline) => void): void => {
      events
        .listen<EventPayloadMap[typeof event]>(event, ({ pipelines }) => {
          if (active) {
            pipelines.forEach(handle)
          }
        })
        .then((fn) => {
          if (active) {
            unlisteners.push(fn)
          } else {
            fn()
          }
        })
        .catch((error: unknown) => logger.warn('useFailureNotifications', `Couldn't listen for ${event}`, error))
    }

    const providerOf = (pipeline: Pipeline) =>
      queryClient
        .getQueryData<ProviderSummary[]>(queryKeys.providers.list())
        ?.find((item) => item.id === pipeline.provider_id)

    if (native) {
      listen('pipelines-failed', (pipeline) =>
        notify(
          pipeline,
          attentionKind(pipeline),
          'danger',
          alertTitle(pipeline),
          providerOf(pipeline),
          systemDeliveryEnabled()
        )
      )
    }

    listen('pipelines-finished', (pipeline) => {
      const outcome = FINISH_OUTCOMES[pipeline.status]

      if (outcome) {
        notify(
          pipeline,
          'finished',
          'success',
          `${pipeline.name} ${outcome}`,
          providerOf(pipeline),
          systemDeliveryEnabled()
        )
      }
    })

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
        const overdueRun = pipeline.overdue_run
        const newOverdueRun =
          previous !== undefined && !!overdueRun && notifiedOverdueRuns.get(pipeline.id) !== overdueRun
        const failed =
          !native &&
          previous !== undefined &&
          (pipeline.status === 'failed' || pipeline.status === 'degraded') &&
          (previous.status !== pipeline.status || previous.last_run !== pipeline.last_run)

        lastPipelines.set(pipeline.id, pipeline)
        if (overdueRun) {
          notifiedOverdueRuns.set(pipeline.id, overdueRun)
        }

        if (!failed && !newOverdueRun) {
          continue
        }

        const provider = providers?.find((p) => p.id === pipeline.provider_id)
        if (failed) {
          notify(pipeline, attentionKind(pipeline), 'danger', alertTitle(pipeline), provider, enabled)
        }
        if (newOverdueRun) {
          notify(pipeline, 'long_run', 'warning', `${pipeline.name} is running longer than usual`, provider, enabled)
        }
      }
    })

    return () => {
      active = false
      for (const unlisten of unlisteners) {
        unlisten()
      }
      unsubscribe()
    }
  }, [queryClient])
}
