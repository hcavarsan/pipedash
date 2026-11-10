import { useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'

import { logger } from '../lib/logger'
import { applyProviderStatus } from '../lib/providerStatusSync'
import { queryKeys } from '../lib/queryKeys'
import { setProviderPipelines } from '../queries/usePipelinesQueries'
import { listenEvent } from '../services/events'
import { WS_EVENTS } from '../services/websocket'
import { useAuthStore } from '../stores/authStore'
import type { EventPayloadMap } from '../types/events'
import { useEventBatch } from './useEventBatch'

export function useEventSync(): void {
  const queryClient = useQueryClient()
  const { scheduleBatchedInvalidation, scheduleRunHistoryInvalidation, scheduleViewsInvalidation, cancel } =
    useEventBatch(queryClient)

  useEffect(() => {
    let mounted = true
    const cleanupFns: Array<() => void> = []

    async function register<T>(name: string, handler: (payload: T) => void | Promise<void>) {
      try {
        const unlisten = await listenEvent<T>(name, (payload) => {
          if (!mounted) {
            return
          }

          try {
            void Promise.resolve(handler(payload)).catch((error: unknown) => {
              logger.error('EventSync', `Handler for ${name} failed`, error)
            })
          } catch (error) {
            logger.error('EventSync', `Handler for ${name} failed`, error)
          }
        })

        if (mounted) {
          cleanupFns.push(unlisten)
        } else {
          unlisten()
        }
      } catch (error) {
        logger.error('EventSync', `Failed to listen to ${name}`, error)
      }
    }

    void register<EventPayloadMap['provider-added']>('provider-added', (payload) => {
      if (!payload?.provider) {
        logger.warn('EventSync', 'provider-added: Invalid payload')

        return
      }
      logger.debug('EventSync', 'provider-added', { providerId: payload.provider.id })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.all })
      scheduleBatchedInvalidation(payload.provider.id)
    })

    void register<EventPayloadMap['provider-updated']>('provider-updated', (payload) => {
      if (!payload?.provider) {
        logger.warn('EventSync', 'provider-updated: Invalid payload')

        return
      }
      logger.debug('EventSync', 'provider-updated', { providerId: payload.provider.id })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.providers.detail(payload.provider.id),
      })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
    })

    void register<EventPayloadMap['provider-removed']>('provider-removed', (payload) => {
      if (!payload?.provider) {
        logger.warn('EventSync', 'provider-removed: Invalid payload')

        return
      }
      logger.debug('EventSync', 'provider-removed', { providerId: payload.provider.id })
      queryClient.removeQueries({
        queryKey: queryKeys.providers.detail(payload.provider.id),
      })
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
      scheduleBatchedInvalidation(payload.provider.id)
    })

    void register<EventPayloadMap['provider-status-updated']>('provider-status-updated', async (payload) => {
      if (!payload?.provider) {
        logger.warn('EventSync', 'provider-status-updated: Invalid payload')

        return
      }
      logger.debug('EventSync', 'provider-status-updated', { providerId: payload.provider.id })
      await applyProviderStatus(queryClient, payload.provider, () => mounted)
    })

    void register<EventPayloadMap['providers-changed']>('providers-changed', () => {
      logger.debug('EventSync', 'providers-changed')
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
    })

    void register<EventPayloadMap['pipeline-views-changed']>('pipeline-views-changed', () => {
      logger.debug('EventSync', 'pipeline-views-changed')
      scheduleViewsInvalidation()
    })

    void register<EventPayloadMap['pipeline-status-changed']>('pipeline-status-changed', (payload) => {
      if (payload?.pipeline_ids?.length) {
        scheduleRunHistoryInvalidation(payload.pipeline_ids)
      }
    })

    void register<EventPayloadMap['run-updated']>('run-updated', (payload) => {
      if (payload?.pipelineId && payload.run) {
        scheduleRunHistoryInvalidation([payload.pipelineId])
      }
    })

    void register<EventPayloadMap['metrics-global-config-changed']>('metrics-global-config-changed', () => {
      logger.debug('EventSync', 'metrics-global-config-changed')
      void queryClient.invalidateQueries({ queryKey: queryKeys.metrics.all })
    })

    void register<EventPayloadMap['pipelines-updated']>('pipelines-updated', async (payload) => {
      if (!payload?.pipelines || payload.providerId === undefined) {
        logger.warn('EventSync', 'pipelines-updated: Invalid payload, invalidating cache', payload)
        scheduleBatchedInvalidation(payload?.providerId)

        return
      }

      logger.debug('EventSync', 'pipelines-updated', {
        count: payload.pipelines.length,
        providerId: payload.providerId,
        timestamp: payload.timestamp,
      })

      await queryClient.cancelQueries({
        queryKey: queryKeys.pipelines.list({ providerId: payload.providerId }),
        exact: true,
      })

      if (!mounted) {
        return
      }

      setProviderPipelines(queryClient, payload.providerId, payload.pipelines)

      logger.debug('EventSync', 'pipelines-updated: Cache updated', {
        providerId: payload.providerId,
        pipelineCount: payload.pipelines.length,
      })
    })

    void register<EventPayloadMap['run-history-cache-invalidated']>('run-history-cache-invalidated', (payload) => {
      const { pipeline_id: pipelineId, run_number: runNumber } = payload

      void queryClient.invalidateQueries({
        queryKey: pipelineId ? queryKeys.runs.list(pipelineId) : [...queryKeys.runs.all, 'list'],
      })

      if (pipelineId !== undefined && runNumber !== undefined) {
        void queryClient.invalidateQueries({ queryKey: queryKeys.runs.detail(pipelineId, runNumber) })
        void queryClient.invalidateQueries({ queryKey: queryKeys.runLogs.tree(pipelineId, runNumber) })
      }
    })

    void register<unknown>('storage-updated', () => {
      logger.debug('EventSync', 'storage-updated')
      void queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
      void queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
    })

    void register<unknown>('cache-cleared', () => {
      logger.debug('EventSync', 'cache-cleared')
      void queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })
      scheduleBatchedInvalidation()
    })

    void register<{ providerId: number }>('schema-updated', (payload) => {
      if (!payload?.providerId) {
        logger.warn('EventSync', 'schema-updated: Invalid payload, invalidating all')
        void queryClient.invalidateQueries({ queryKey: queryKeys.tableSchema.all })

        return
      }

      logger.debug('EventSync', 'schema-updated', { providerId: payload.providerId })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.tableSchema.detail(payload.providerId),
      })
    })

    void register<unknown>('vault-unlocked', async () => {
      logger.info('EventSync', 'vault-unlocked: CoreContext now available, refreshing data')

      await queryClient.cancelQueries()

      if (!mounted) {
        return
      }

      await queryClient.invalidateQueries({
        queryKey: queryKeys.vault.all,
        refetchType: 'active',
      })

      if (!mounted) {
        return
      }

      await queryClient.invalidateQueries({
        queryKey: queryKeys.providers.all,
        refetchType: 'active',
      })

      if (!mounted) {
        return
      }

      await queryClient.invalidateQueries({
        queryKey: queryKeys.plugins.all,
        refetchType: 'active',
      })

      if (!mounted) {
        return
      }

      await queryClient.invalidateQueries({
        queryKey: queryKeys.pipelines.all,
        refetchType: 'active',
      })

      if (!mounted) {
        return
      }

      void queryClient.invalidateQueries({
        queryKey: queryKeys.storage.all,
        refetchType: 'none',
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.tableSchema.all,
        refetchType: 'none',
      })
      void queryClient.invalidateQueries({
        queryKey: queryKeys.cache.all,
        refetchType: 'none',
      })
    })

    void register<unknown>('vault-locked', async () => {
      logger.info('EventSync', 'vault-locked: CoreContext shut down, routing to unlock')

      useAuthStore.getState().clearToken()
      await queryClient.cancelQueries()

      if (!mounted) {
        return
      }

      await queryClient.invalidateQueries({ queryKey: queryKeys.vault.all, refetchType: 'active' })
    })

    for (const name of [WS_EVENTS.DEVICE_PAIRED, WS_EVENTS.DEVICE_REVOKED]) {
      void register<unknown>(name, () => {
        void queryClient.invalidateQueries({ queryKey: queryKeys.devices.all })
      })
    }

    return () => {
      mounted = false
      logger.debug('EventSync', 'Cleaning up listeners')

      cancel()

      for (const fn of cleanupFns) {
        try {
          fn()
        } catch (error) {
          logger.error('EventSync', 'Error during listener cleanup', error)
        }
      }
      cleanupFns.length = 0
    }
  }, [queryClient, scheduleBatchedInvalidation, scheduleRunHistoryInvalidation, scheduleViewsInvalidation, cancel])

  useEffect(() => {
    let mounted = true
    let wasDisconnected = false
    let unlisten: (() => void) | undefined

    const handleConnectionStatus = (payload: { status: string; reconnectAttempts: number }) => {
      if (payload.status === 'disconnected' || payload.status === 'reconnecting') {
        wasDisconnected = true
        logger.debug('EventSync', 'Connection lost', { status: payload.status })
      } else if (payload.status === 'connected' && wasDisconnected) {
        logger.info('EventSync', 'Reconnected - invalidating active queries')
        wasDisconnected = false

        void queryClient.invalidateQueries({ refetchType: 'active' })
      }
    }

    listenEvent<{ status: string; reconnectAttempts: number }>(WS_EVENTS.CONNECTION_STATUS, handleConnectionStatus)
      .then((stop) => {
        if (mounted) {
          unlisten = stop
        } else {
          stop()
        }
      })
      .catch((error: unknown) => logger.error('EventSync', 'Failed to listen to connection-status', error))

    return () => {
      mounted = false
      unlisten?.()
    }
  }, [queryClient])
}
