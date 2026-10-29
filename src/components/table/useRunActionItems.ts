import type { PipelineRun } from '../../types'

import { useRunActions } from './RunActionsContext'
import { buildRunActions, type RunAction } from './runActions'

export function useRunActionItems(run: PipelineRun): RunAction[] {
  return buildRunActions(useRunActions(), run)
}
