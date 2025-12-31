import { useEffect, useRef } from 'react'

import { events } from '../services'

const METRICS_EVENT_TYPES = [
  'metrics-generated',
  'metrics-config-changed',
  'run-triggered',
  'run-cancelled',
  'pipeline-status-changed',
] as const

type MetricsEventType = (typeof METRICS_EVENT_TYPES)[number]

interface UseMetricsEventListenersOptions {
  pipelineId: string | null
  enabled: boolean
  onRefetch: () => void
  onConfigChanged?: () => void
}

export function useMetricsEventListeners({
  pipelineId,
  enabled,
  onRefetch,
  onConfigChanged,
}: UseMetricsEventListenersOptions): void {
  const onRefetchRef = useRef(onRefetch)
  const onConfigChangedRef = useRef(onConfigChanged)

  useEffect(() => {
    onRefetchRef.current = onRefetch
    onConfigChangedRef.current = onConfigChanged
  }, [onRefetch, onConfigChanged])

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
        onRefetchRef.current()
        
return
      }

      if (payload === pipelineId) {
        if (eventType === 'metrics-config-changed' && onConfigChangedRef.current) {
          onConfigChangedRef.current()
        } else {
          onRefetchRef.current()
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
        console.error('Failed to setup metrics event listeners:', error)
      }
    }

    setupListeners()

    return () => {
      isActive = false
      unlisteners.forEach((unlisten) => {
        try {
          unlisten()
        } catch (error) {
          console.error('Error unlistening:', error)
        }
      })
    }
  }, [enabled, pipelineId])
}
