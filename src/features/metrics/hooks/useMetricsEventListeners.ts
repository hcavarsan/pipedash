import { useEffect, useRef } from 'react'

import { logger } from '../../../lib/logger'
import { listenEvent } from '../../../services/events'
import { isRecord } from '../../../utils/isRecord'
import { useInvalidatePipelineMetrics } from '../queries/useMetricsQueries'

const GATED_EVENT_TYPES = ['metrics-generated', 'run-triggered', 'run-cancelled', 'pipeline-status-changed'] as const

type GatedEventType = (typeof GATED_EVENT_TYPES)[number]

interface UseMetricsEventListenersOptions {
  pipelineId: string
  enabled: boolean
  onConfigChanged?: () => void
}

export function useMetricsEventListeners({
  pipelineId,
  enabled,
  onConfigChanged,
}: UseMetricsEventListenersOptions): void {
  const invalidateMetrics = useInvalidatePipelineMetrics(pipelineId)
  const invalidateRef = useRef(invalidateMetrics)
  const onConfigChangedRef = useRef(onConfigChanged)

  useEffect(() => {
    invalidateRef.current = invalidateMetrics
    onConfigChangedRef.current = onConfigChanged
  }, [invalidateMetrics, onConfigChanged])

  useEffect(() => {
    if (!pipelineId) {
      return
    }

    let isActive = true
    let unlisten: (() => void) | undefined

    listenEvent<unknown>('metrics-config-changed', (payload) => {
      if (!isActive || payload !== pipelineId) {
        return
      }

      if (onConfigChangedRef.current) {
        onConfigChangedRef.current()
      } else {
        void invalidateRef.current()
      }
    })
      .then((stop) => {
        if (isActive) {
          unlisten = stop
        } else {
          stop()
        }
      })
      .catch((error: unknown) => {
        logger.error('useMetricsEventListeners', 'Failed to listen to metrics-config-changed', error)
      })

    return () => {
      isActive = false
      unlisten?.()
    }
  }, [pipelineId])

  useEffect(() => {
    if (!enabled || !pipelineId) {
      return
    }

    let isActive = true
    const unlisteners: Array<() => void> = []

    const handleEvent = (eventType: GatedEventType, payload: unknown) => {
      if (!isActive) {
        return
      }

      if (eventType === 'pipeline-status-changed') {
        const ids =
          isRecord(payload) && Array.isArray(payload.pipeline_ids)
            ? payload.pipeline_ids.filter((id): id is string => typeof id === 'string')
            : []

        if (ids.includes(pipelineId)) {
          void invalidateRef.current()
        }

        return
      }

      if (payload === pipelineId) {
        void invalidateRef.current()
      }
    }

    const setupListeners = async () => {
      try {
        for (const eventType of GATED_EVENT_TYPES) {
          const unlisten = await listenEvent<unknown>(eventType, (payload) => {
            handleEvent(eventType, payload)
          })

          if (!isActive) {
            unlisten()

            return
          }

          unlisteners.push(unlisten)
        }
      } catch (error) {
        logger.error('useMetricsEventListeners', 'Failed to set up metrics event listeners', error)
      }
    }

    void setupListeners()

    return () => {
      isActive = false
      unlisteners.forEach((unlisten) => {
        try {
          unlisten()
        } catch (error) {
          logger.error('useMetricsEventListeners', 'Failed to remove metrics event listener', error)
        }
      })
    }
  }, [enabled, pipelineId])
}
