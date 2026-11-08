import { useEffect, useEffectEvent } from 'react'

import { logger } from '../../../lib/logger'
import { listenEvent } from '../../../services/events'
import type { EventPayloadMap } from '../../../types/events'
import type { PipelineRun } from '../../../types/pipelines'

export function useRunUpdatedEvent(
  pipelineId: string | undefined,
  enabled: boolean,
  onUpdate: (run: PipelineRun) => void
): void {
  const handle = useEffectEvent(onUpdate)

  useEffect(() => {
    if (!enabled || !pipelineId) {
      return
    }

    let mounted = true
    let unlisten: (() => void) | undefined

    listenEvent<EventPayloadMap['run-updated']>('run-updated', (payload) => {
      if (mounted && payload?.pipelineId === pipelineId && payload.run) {
        handle(payload.run)
      }
    })
      .then((remove) => {
        if (mounted) {
          unlisten = remove
        } else {
          remove()
        }
      })
      .catch((error: unknown) => {
        logger.error('RunUpdates', 'Failed to listen to run-updated', error)
      })

    return () => {
      mounted = false
      unlisten?.()
    }
  }, [enabled, pipelineId])
}
