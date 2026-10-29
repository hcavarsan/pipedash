import type { PipelineRun } from '../../../types/domain'

import { useRunActions } from '../components/RunActionsContext'
import { buildRunActions, type RunAction } from '../lib/runActions'

export function useRunActionItems(run: PipelineRun): RunAction[] {
  return buildRunActions(useRunActions(), run)
}
