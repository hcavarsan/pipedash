import { useBuildWatch } from '../../../queries/useBuildWatchQueries'
import { useReviewRun } from '../../../queries/useRunHistoryQueries'
import type { Pipeline, PipelineRun } from '../../../types/pipelines'
import type { PluginPresentation } from '../../../types/plugin_presentation'
import { buildRunActions, type RunAction, type RunActionsValue, watchStateOf } from '../lib/runActions'
import type { UseRunDetailActionsResult } from './useRunDetailActions'

interface RunDetailActionItemsOptions {
  pipeline: Pipeline | undefined
  presentation: PluginPresentation | undefined
  run: PipelineRun | undefined
  actions: UseRunDetailActionsResult
  cancellationRequested: boolean
  requestRerun: () => void
}

export function useRunDetailActionItems({
  pipeline,
  presentation,
  run,
  actions,
  cancellationRequested,
  requestRerun,
}: RunDetailActionItemsOptions): RunAction[] {
  const buildWatch = useBuildWatch()
  const reviewRun = useReviewRun()

  if (!pipeline || !presentation || !run) {
    return []
  }

  const number = run.run_number
  const value: RunActionsValue = {
    pipeline,
    presentation,
    labels: presentation.terminology,
    canCancel: pipeline.actions.cancel.allowed,
    cancellingRun: actions.cancelPending ? number : null,
    reviewingRun: reviewRun.isPending ? number : null,
    rerunningFailedRun: null,
    rerunningRuns: new Set(actions.rerunPending ? [number] : []),
    watch: watchStateOf(buildWatch),
    startWatch: (runNumber) => buildWatch.start(pipeline.id, runNumber),
    openRun: () => undefined,
    openTrigger: () => undefined,
    cancel: () => actions.cancel(),
    review: async (target, decision) => {
      await reviewRun
        .mutateAsync({ pipelineId: target.pipeline_id, runNumber: target.run_number, decision })
        .catch(() => undefined)
    },
    requestRerun,
    rerunTarget: null,
    closeRerun: () => undefined,
    confirmRerun: actions.rerun,
    triggerBlockedReason: pipeline.actions.trigger.reason ?? undefined,
  }

  const locked = cancellationRequested && !run.actions.force_cancel.allowed

  return buildRunActions(value, run)
    .filter((action) => action.id !== 'details')
    .map((action) => (action.id === 'cancel' && locked ? { ...action, disabled: true } : action))
}
