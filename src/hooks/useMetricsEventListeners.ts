import { useEffect, useRef } from 'react'

import { logger } from '../lib/logger'
import { useInvalidatePipelineMetrics } from '../queries/useMetricsQueries'
import { events } from '../services'
import type { PipelineStatusChangedPayload } from '../types/events'

const METRICS_EVENT_TYPES = [
  'metrics-generated',
  'metrics-config-changed',
  'run-triggered',
  'run-cancelled',
  'pipeline-status-changed',
] as const

type MetricsEventType = (typeof METRICS_EVENT_TYPES)[number]

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
    if (!enabled || !pipelineId) {
      return
    }

    let isActive = true
    const unlisteners: Array<() => void> = []

    const handleEvent = (eventType: MetricsEventType, payload: unknown) => {
      if (!isActive) {
        return
      }

      if (eventType === 'pipeline-status-changed') {
        const ids = (payload as PipelineStatusChangedPayload | null)?.pipeline_ids

        if (ids?.includes(pipelineId)) {
          invalidateRef.current()
        }

        return
      }

      if (payload === pipelineId) {
        if (eventType === 'metrics-config-changed' && onConfigChangedRef.current) {
          onConfigChangedRef.current()
        } else {
          invalidateRef.current()
        }
      }
    }

    const setupListeners = async () => {
      try {
        for (const eventType of METRICS_EVENT_TYPES) {
          const unlisten = await events.listen<unknown>(eventType, (payload) => {
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

    setupListeners()

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
