import { useCallback, useEffect, useRef } from 'react'

import { useQueryClient } from '@tanstack/react-query'

import { events, wsClient } from '../services'
import type { EventPayloadMap } from '../types/events'

import { logger } from './logger'
import { queryKeys } from './queryKeys'

const BATCH_WINDOW_MS = 300

interface BatchedInvalidation {
  providerIds: Set<number>
  invalidateAll: boolean
  timestamp: number
}

export function useEventSync() {
  const queryClient = useQueryClient()

  const batchedInvalidations = useRef<BatchedInvalidation>({
    providerIds: new Set(),
    invalidateAll: false,
    timestamp: Date.now(),
  })
  const batchTimeout = useRef<ReturnType<typeof setTimeout> | null>(null)

  const processBatch = useCallback(() => {
    const batch = batchedInvalidations.current

    if (batch.invalidateAll) {
      logger.debug('EventSync', 'Processing batched invalidation: ALL providers')
      queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
    } else if (batch.providerIds.size > 0) {
      logger.debug('EventSync', 'Processing batched invalidation', {
        providerIds: Array.from(batch.providerIds),
      })

      batch.providerIds.forEach((providerId) => {
        queryClient.invalidateQueries({
          queryKey: queryKeys.pipelines.list({ providerId }),
        })
      })
    }

    batchedInvalidations.current = {
      providerIds: new Set(),
      invalidateAll: false,
      timestamp: Date.now(),
    }
    batchTimeout.current = null
  }, [queryClient])

  const scheduleBatchedInvalidation = useCallback(
    (providerId?: number) => {
      if (providerId === undefined) {
        batchedInvalidations.current.invalidateAll = true
      } else {
        batchedInvalidations.current.providerIds.add(providerId)
      }

      if (batchTimeout.current) {
        clearTimeout(batchTimeout.current)
      }

      batchTimeout.current = setTimeout(processBatch, BATCH_WINDOW_MS)
    },
    [processBatch]
  )

  const cleanupFnsRef = useRef<Array<() => void>>([])

  useEffect(() => {
    let mounted = true

    async function setupListeners() {
      const cleanupFns: Array<() => void> = []

      try {
        const unlistenProviderAdded = await events.listen<
          EventPayloadMap['provider-added']
        >('provider-added', (payload) => {
          if (!payload?.provider) {
            logger.warn('EventSync', 'provider-added: Invalid payload')

return
          }
          logger.debug('EventSync', 'provider-added', { providerId: payload.provider.id })
          queryClient.invalidateQueries({ queryKey: queryKeys.providers.all })
          scheduleBatchedInvalidation(payload.provider.id)
        })

        const unlistenProviderUpdated = await events.listen<
          EventPayloadMap['provider-updated']
        >('provider-updated', (payload) => {
          if (!payload?.provider) {
            logger.warn('EventSync', 'provider-updated: Invalid payload')

return
          }
          logger.debug('EventSync', 'provider-updated', { providerId: payload.provider.id })
          queryClient.invalidateQueries({
            queryKey: queryKeys.providers.detail(payload.provider.id),
          })
          queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
        })

        const unlistenProviderRemoved = await events.listen<
          EventPayloadMap['provider-removed']
        >('provider-removed', (payload) => {
          if (!payload?.provider) {
            logger.warn('EventSync', 'provider-removed: Invalid payload')

return
          }
          logger.debug('EventSync', 'provider-removed', { providerId: payload.provider.id })
          queryClient.removeQueries({
            queryKey: queryKeys.providers.detail(payload.provider.id),
          })
          queryClient.invalidateQueries({ queryKey: queryKeys.providers.list() })
          scheduleBatchedInvalidation(payload.provider.id)
        })

        const unlistenPipelinesUpdated = await events.onPipelinesUpdated<
          EventPayloadMap['pipelines-updated']
        >(async (payload) => {
          if (!payload || !payload.pipelines) {
            logger.warn('EventSync', 'pipelines-updated: Invalid payload, invalidating cache', payload)
            scheduleBatchedInvalidation(payload?.providerId ?? undefined)

            return
          }

          const normalizedProviderId = payload.providerId ?? undefined

          logger.debug('EventSync', 'pipelines-updated', {
            count: payload.pipelines.length,
            providerId: normalizedProviderId,
            timestamp: payload.timestamp,
          })

          const cacheKey = queryKeys.pipelines.list({ providerId: normalizedProviderId })

          await queryClient.cancelQueries({ queryKey: queryKeys.pipelines.all })

          queryClient.setQueryData(cacheKey, payload.pipelines)

          logger.debug('EventSync', 'pipelines-updated: Cache updated', {
            cacheKey,
            pipelineCount: payload.pipelines.length,
          })
        })

        const unlistenCacheInvalidated = await events.listen<
          EventPayloadMap['pipeline-cache-invalidated']
        >('pipeline-cache-invalidated', (payload) => {
          if (!payload) {
            logger.warn('EventSync', 'cache-invalidated: Invalid payload, invalidating all')
            queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })

            return
          }

          const normalizedProviderId = payload.providerId ?? undefined

          logger.debug('EventSync', 'cache-invalidated', {
            reason: payload.reason || 'unknown',
            providerId: normalizedProviderId,
          })

          if (normalizedProviderId !== undefined) {
            queryClient.invalidateQueries({
              queryKey: queryKeys.pipelines.list({ providerId: normalizedProviderId }),
            })
          } else {
            queryClient.invalidateQueries({ queryKey: queryKeys.pipelines.all })
          }
        })

        const unlistenStorageUpdated = await events.listen<unknown>(
          'storage-updated',
          () => {
            logger.debug('EventSync', 'storage-updated')
            queryClient.invalidateQueries({ queryKey: queryKeys.storage.all })
            queryClient.invalidateQueries({ queryKey: queryKeys.vault.all })
          }
        )

        const unlistenCacheCleared = await events.listen<unknown>('cache-cleared', () => {
          logger.debug('EventSync', 'cache-cleared')
          queryClient.invalidateQueries({ queryKey: queryKeys.cache.all })
          scheduleBatchedInvalidation()
        })

        const unlistenSchemaUpdated = await events.listen<{ providerId: number }>(
          'schema-updated',
          (payload) => {
            if (!payload?.providerId) {
              logger.warn('EventSync', 'schema-updated: Invalid payload, invalidating all')
              queryClient.invalidateQueries({ queryKey: queryKeys.tableSchema.all })

return
            }

            logger.debug('EventSync', 'schema-updated', { providerId: payload.providerId })
            queryClient.invalidateQueries({
              queryKey: queryKeys.tableSchema.detail(payload.providerId),
            })
          }
        )

        const unlistenVaultUnlocked = await events.listen<unknown>(
          'vault-unlocked',
          async () => {
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

            queryClient.invalidateQueries({
              queryKey: queryKeys.storage.all,
              refetchType: 'none',
            })
            queryClient.invalidateQueries({
              queryKey: queryKeys.tableSchema.all,
              refetchType: 'none',
            })
            queryClient.invalidateQueries({
              queryKey: queryKeys.cache.all,
              refetchType: 'none',
            })
          }
        )

        cleanupFns.push(
          unlistenProviderAdded,
          unlistenProviderUpdated,
          unlistenProviderRemoved,
          unlistenPipelinesUpdated,
          unlistenCacheInvalidated,
          unlistenStorageUpdated,
          unlistenCacheCleared,
          unlistenSchemaUpdated,
          unlistenVaultUnlocked
        )

        if (mounted) {
          cleanupFnsRef.current = cleanupFns
          logger.debug('EventSync', 'All listeners registered successfully')
        } else {
          logger.debug('EventSync', 'Component unmounted during setup, cleaning up listeners')
          cleanupFns.forEach((fn) => {
            try {
              fn()
            } catch (error) {
              logger.error('EventSync', 'Error during immediate cleanup', error)
            }
          })
        }
      } catch (error) {
        logger.error('EventSync', 'Failed to setup listeners', error)
      }
    }

    setupListeners()

    return () => {
      mounted = false
      logger.debug('EventSync', 'Cleaning up listeners')
      cleanupFnsRef.current.forEach((fn) => {
        try {
          fn()
        } catch (error) {
          logger.error('EventSync', 'Error during listener cleanup', error)
        }
      })
      cleanupFnsRef.current = []
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

        queryClient.invalidateQueries({
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
