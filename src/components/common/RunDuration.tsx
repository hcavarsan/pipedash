import type { ReactNode } from 'react'

import { useNow } from '../../hooks/useNow'
import type { PipelineRun } from '../../types/pipelines'
import { formatDuration } from '../../utils/formatDuration'
import { formatLiveRunDuration, isLiveRun } from '../../utils/runTiming'

interface RunDurationProps {
  run: PipelineRun
}

function LiveRunDuration({ run }: RunDurationProps): ReactNode {
  return formatLiveRunDuration(run, useNow(1000))
}

export function RunDuration({ run }: RunDurationProps): ReactNode {
  return isLiveRun(run) ? <LiveRunDuration run={run} /> : formatDuration(run.duration_seconds)
}
