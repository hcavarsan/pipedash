import type { ReactNode } from 'react'

import { useNow } from '../../hooks/useNow'
import type { PipelineRun } from '../../types/domain'
import { formatDuration } from '../../utils/formatDuration'
import { formatLiveRunDuration, isLiveRun } from '../../utils/runTiming'

interface RunDurationProps {
  run: PipelineRun
  typicalSeconds?: number | null | undefined
}

function LiveRunDuration({ run, typicalSeconds }: RunDurationProps): ReactNode {
  return formatLiveRunDuration(run, useNow(1000), typicalSeconds)
}

export function RunDuration({ run, typicalSeconds }: RunDurationProps): ReactNode {
  return isLiveRun(run) ? (
    <LiveRunDuration run={run} typicalSeconds={typicalSeconds} />
  ) : (
    formatDuration(run.duration_seconds)
  )
}
