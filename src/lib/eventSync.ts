import { useQueryClient } from '@tanstack/react-query'
import { useCallback, useEffect, useRef } from 'react'

import { invalidateProviderPipelines, setProviderPipelines } from '../queries/usePipelinesQueries'
import { events, wsClient } from '../services'
import { useAuthStore } from '../stores/authStore'
import type { EventPayloadMap } from '../types/events'

import { logger } from './logger'
import { applyProviderStatus } from './providerStatusSync'
import { queryKeys } from './queryKeys'

const BATCH_WINDOW_MS = 300

interface BatchedInvalidation {
  providerIds: Set<number>
  invalidateAll: boolean
  timestamp: number
}

export function useEventSync(): void {
  const queryClient = useQueryClient()

  const batchedInvalidations = useRef<BatchedInvalidation>({
    providerIds: new Set(),
    invalidateAll: false,
    timestamp: Date.now(),
  })
  const batchTimeout = useRef<ReturnType<typeof setTimeout> | undefined>(undefined)

  const processBatch = useCallback(() => {
    const batch = batchedInvalidations.current

    if (batch.invalidateAll) {
      logger.debug('EventSync', 'Processing batched invalidation: ALL providers')
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
    } else if (batch.providerIds.size > 0) {
      logger.debug('EventSync', 'Processing batched invalidation', {
        providerIds: Array.from(batch.providerIds),
      })

      for (const providerId of batch.providerIds) {
        void invalidateProviderPipelines(queryClient, providerId)
      }
    }

    batchedInvalidations.current = {
      providerIds: new Set(),
      invalidateAll: false,
      timestamp: Date.now(),
    }
    batchTimeout.current = undefined
  }, [queryClient])

  const scheduleBatchedInvalidation = useCallback(
    (providerId?: number) => {
      if (providerId === undefined) {
        batchedInvalidations.current.invalidateAll = true
      } else {
        batchedInvalidations.current.providerIds.add(providerId)
      }

      clearTimeout(batchTimeout.current)

      batchTimeout.current = setTimeout(processBatch, BATCH_WINDOW_MS)
    },
    [processBatch]
  )

  useEffect(() => {
    let mounted = true
    const cleanupFns: Array<() => void> = []

    async function register<T>(name: string, handler: (payload: T) => void | Promise<void>) {
      try {
        const unlisten = await events.listen<T>(name, (payload) => {
          void Promise.resolve(handler(payload)).catch((error: unknown) => {
            logger.error('EventSync', `Handler for ${name} failed`, error)
          })
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
      await applyProviderStatus(queryClient, payload.provider)
    })

    void register<EventPayloadMap['providers-changed']>('providers-changed', () => {
      logger.debug('EventSync', 'providers-changed')
      void queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
    })

    void register<EventPayloadMap['pipeline-views-changed']>('pipeline-views-changed', () => {
      logger.debug('EventSync', 'pipeline-views-changed')
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pages() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.views() })
      void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.pins() })
      void queryClient.invalidateQueries({ queryKey: [...queryKeys.pipelines.all, 'viewMembers'] })
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
      setProviderPipelines(queryClient, payload.providerId, payload.pipelines)

      logger.debug('EventSync', 'pipelines-updated: Cache updated', {
        providerId: payload.providerId,
        pipelineCount: payload.pipelines.length,
      })
    })

    void register<EventPayloadMap['pipeline-cache-invalidated']>('pipeline-cache-invalidated', (payload) => {
      if (!payload) {
        logger.warn('EventSync', 'cache-invalidated: Invalid payload, invalidating all')
        void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

        return
      }

      const normalizedProviderId = payload.providerId ?? undefined

      logger.debug('EventSync', 'cache-invalidated', {
        reason: payload.reason || 'unknown',
        providerId: normalizedProviderId,
      })

      if (normalizedProviderId !== undefined) {
        void invalidateProviderPipelines(queryClient, normalizedProviderId)
      } else {
        void queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
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

      await queryClient.invalidateQueries({
        queryKey: queryKeys.vault.all,
        refetchType: 'active',
      })

      await queryClient.invalidateQueries({
        queryKey: queryKeys.providers.all,
        refetchType: 'active',
      })

      await queryClient.invalidateQueries({
        queryKey: queryKeys.pipelines.all,
        refetchType: 'active',
      })

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
      await queryClient.invalidateQueries({ queryKey: queryKeys.vault.all, refetchType: 'active' })
    })

    return () => {
      mounted = false
      logger.debug('EventSync', 'Cleaning up listeners')

      clearTimeout(batchTimeout.current)
      batchTimeout.current = undefined

      for (const fn of cleanupFns) {
        try {
          fn()
        } catch (error) {
          logger.error('EventSync', 'Error during listener cleanup', error)
        }
      }
      cleanupFns.length = 0
    }
  }, [queryClient, scheduleBatchedInvalidation])

  useEffect(() => {
    let wasDisconnected = false

    const handleConnectionStatus = (payload: { status: string; reconnectAttempts: number }) => {
      if (payload.status === 'disconnected' || payload.status === 'reconnecting') {
        wasDisconnected = true
        logger.debug('EventSync', 'WebSocket disconnected', { status: payload.status })
      } else if (payload.status === 'connected' && wasDisconnected) {
        logger.info('EventSync', 'WebSocket reconnected - invalidating critical queries')
        wasDisconnected = false

        void queryClient.invalidateQueries({
          queryKey: queryKeys.providers.all,
          refetchType: 'active',
        })

        scheduleBatchedInvalidation()
      }
    }

    const unlisten = wsClient.listen<{ status: string; reconnectAttempts: number }>(
      'connection-status',
      handleConnectionStatus
    )

    return () => {
      unlisten()
    }
  }, [queryClient, scheduleBatchedInvalidation])
}
