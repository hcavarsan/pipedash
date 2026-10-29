import { useEffect, useEffectEvent } from 'react'

import { logger } from '../../lib/logger'
import { events } from '../../services'

interface RunHistoryEventsOptions {
  pipelineId: string | undefined
  resetPage: () => void
  refetch: () => void
}

export function useRunHistoryEvents({ pipelineId, resetPage, refetch }: RunHistoryEventsOptions): void {
  const onTriggered = useEffectEvent(() => {
    resetPage()
    refetch()
  })
  const onCancelled = useEffectEvent(() => {
    refetch()
  })

  useEffect(() => {
    if (!pipelineId) {
      return
    }

    let mounted = true
    const unlisteners: Array<() => void> = []

    const register = async (name: string, handler: () => void): Promise<void> => {
      const unlisten = await events.listen<string>(name, (payload) => {
        if (mounted && payload === pipelineId) {
          handler()
        }
      })

      if (mounted) {
        unlisteners.push(unlisten)
      } else {
        unlisten()
      }
    }

    Promise.all([register('run-triggered', onTriggered), register('run-cancelled', onCancelled)]).catch(
      (error: unknown) => {
        logger.error('RunHistory', 'Failed to register run listeners', error)
      }
    )

    return () => {
      mounted = false
      for (const unlisten of unlisteners) {
        unlisten()
      }
    }
  }, [pipelineId])
}
