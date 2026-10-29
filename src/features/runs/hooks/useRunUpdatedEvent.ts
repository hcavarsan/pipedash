import { useEffect, useEffectEvent } from 'react'

import { logger } from '../../../lib/logger'
import { events } from '../../../services/service'
import type { PipelineRun } from '../../../types/domain'
import type { EventPayloadMap } from '../../../types/events'

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

    events
      .listen<EventPayloadMap['run-updated']>('run-updated', (payload) => {
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
